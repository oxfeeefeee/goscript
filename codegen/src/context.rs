// Copyright 2022 The Goscript Authors. All rights reserved.
// Use of this source code is governed by a BSD-style
// license that can be found in the LICENSE file.

use super::consts::Consts;
use super::types::TypeLookup;
use goscript_parser::ast::*;
use goscript_parser::objects::{IdentKey, Objects as AstObjects};
use goscript_parser::{Map, Pos};
use goscript_types::{ObjKey as TCObjKey, TypeKey as TCTypeKey};
use goscript_vm::ffi::*;
use goscript_vm::value::*;
use std::convert::TryFrom;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Addr {
    Const(usize),
    LocalVar(usize),
    Regsiter(usize),
    Imm(OpIndex),
    PkgMemberIndex(PackageKey, IdentKey), // deferred resolve
    Label(TCObjKey),                      // deferred resolve
    UntypedNil,                           // will be typed when assigned to var
    Void,
}

impl Addr {
    pub fn as_var_index(self) -> usize {
        match self {
            Self::LocalVar(i) => i,
            _ => unreachable!(),
        }
    }

    pub fn as_reg_index(self) -> usize {
        match self {
            Self::Regsiter(i) => i,
            _ => unreachable!(),
        }
    }

    fn into_index(
        self,
        reg_base: usize,
        ast_objs: &AstObjects,
        packages: &PackageObjs,
        inst_index: usize,
        labels: &Map<TCObjKey, usize>,
        cst_map: &Map<usize, usize>,
    ) -> OpIndex {
        // Zero values are the first batch of consts
        match self {
            Self::Const(i) => -(cst_map[&i] as OpIndex) - 1,
            Self::LocalVar(i) => i as OpIndex,
            Self::Regsiter(i) => (reg_base + i) as OpIndex,
            Self::PkgMemberIndex(key, ident) => {
                let pkg = &packages[key];
                let id = &ast_objs.idents[ident];
                *pkg.get_member_index(&id.name).unwrap()
            }
            Self::Label(key) => {
                let label_offset = labels[&key];
                (label_offset as OpIndex) - (inst_index as OpIndex) - 1
            }
            Self::Imm(i) => i,
            Self::UntypedNil => unreachable!(),
            Self::Void => std::i32::MAX,
        }
    }
}

#[derive(Clone, Debug)]
pub enum VirtualAddr {
    Direct(Addr),
    UpValue(Addr),
    SliceEntry(Addr, Addr),
    ArrayEntry(Addr, Addr),
    MapEntry(Addr, Addr, Addr),
    StructMember(Addr, Addr),
    StructEmbedded(Addr, Addr),
    PackageMember(Addr, Addr),
    Pointee(Addr),
    Blank,
}

impl VirtualAddr {
    pub fn try_as_direct_addr(&self) -> Option<Addr> {
        match self {
            Self::Direct(a) => Some(*a),
            _ => None,
        }
    }

    pub fn as_direct_addr(&self) -> Addr {
        self.try_as_direct_addr().unwrap()
    }

    pub fn as_up_value_addr(&self) -> Addr {
        match self {
            Self::UpValue(a) => *a,
            _ => unreachable!(),
        }
    }

    pub fn is_blank(&self) -> bool {
        match self {
            Self::Blank => true,
            _ => false,
        }
    }
}

#[derive(Clone, Debug)]
pub enum ExprMode {
    Load,
    Store(VirtualAddr, Option<TCTypeKey>),
    Discard,
}

impl ExprMode {
    pub fn as_store(&self) -> (VirtualAddr, Option<TCTypeKey>) {
        match self {
            Self::Store(va, t) => (va.clone(), *t),
            _ => unreachable!(),
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct ExprCtx {
    pub mode: ExprMode,
    pub cur_reg: usize,
    pub load_addr: Addr,
}

impl ExprCtx {
    pub fn new(mode: ExprMode, init_reg: usize) -> Self {
        Self {
            mode,
            cur_reg: init_reg,
            load_addr: Addr::Void,
        }
    }

    pub fn lhs_type(&self) -> Option<TCTypeKey> {
        match self.mode {
            ExprMode::Load => None,
            ExprMode::Store(_, t) => t,
            ExprMode::Discard => None,
        }
    }

    /// Load, store or discard based on current ExprMode, by calling f.
    pub fn assign_with<F>(
        &mut self,
        fctx: &mut FuncCtx,
        cast_index: Option<OpIndex>,
        pos: Option<Pos>,
        f: F,
    ) where
        F: FnOnce(&mut FuncCtx, Addr, Option<Pos>),
    {
        match self.mode.clone() {
            ExprMode::Load => {
                self.load_addr = self.inc_cur_reg();
                f(fctx, self.load_addr, pos);
            }
            ExprMode::Store(va, _) => match va {
                VirtualAddr::Direct(d) => {
                    f(fctx, d, pos);
                    Self::cast_to_iface(cast_index, fctx, d, d, pos);
                }
                _ => {
                    let d = self.inc_cur_reg();
                    f(fctx, d, pos);
                    Self::cast_to_iface(cast_index, fctx, d, d, pos);
                    fctx.emit_assign(va.clone(), d, None, pos);
                    self.dec_cur_reg(); // done with the reg
                }
            },
            ExprMode::Discard => {
                self.load_addr = self.inc_cur_reg();
                f(fctx, self.load_addr, pos);
                self.dec_cur_reg(); // done with the reg
            }
        }
    }

    /// Load or store 'src' based on current ExprMode
    pub fn direct_assign(
        &mut self,
        fctx: &mut FuncCtx,
        src: Addr,
        cast_index: Option<OpIndex>,
        pos: Option<Pos>,
    ) {
        match self.mode.clone() {
            ExprMode::Load => {
                self.load_addr = src;
            }
            ExprMode::Store(va, _) => match va {
                VirtualAddr::Direct(d) => {
                    if !Self::cast_to_iface(cast_index, fctx, d, src, pos) {
                        fctx.emit_assign(va, src, None, pos)
                    }
                }
                _ => {
                    let reg = self.inc_cur_reg();
                    let src = if Self::cast_to_iface(cast_index, fctx, reg, src, pos) {
                        reg
                    } else {
                        src
                    };
                    fctx.emit_assign(va, src, None, pos);
                    self.dec_cur_reg(); // done with the reg
                }
            },
            ExprMode::Discard => {}
        }
    }

    pub fn inc_cur_reg(&mut self) -> Addr {
        let r = Addr::Regsiter(self.cur_reg);
        self.cur_reg += 1;
        r
    }

    pub fn dec_cur_reg(&mut self) {
        assert!(self.cur_reg > 0);
        self.cur_reg -= 1;
    }

    fn cast_to_iface(
        cast_index: Option<OpIndex>,
        fctx: &mut FuncCtx,
        dst: Addr,
        src: Addr,
        pos: Option<Pos>,
    ) -> bool {
        match cast_index {
            Some(index) => {
                fctx.emit_cast_iface(dst, src, index, pos);
                true
            }
            None => false,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct InterInst {
    pub op0: Opcode,
    pub op1: Opcode,
    pub t0: ValueType,
    pub t1: ValueType,
    pub d: Addr,
    pub s0: Addr,
    pub s1: Addr,
}

impl InterInst {
    pub fn with_op(op: Opcode) -> Self {
        InterInst {
            op0: op,
            op1: Opcode::VOID,
            t0: ValueType::Void,
            t1: ValueType::Void,
            d: Addr::Void,
            s0: Addr::Void,
            s1: Addr::Void,
        }
    }

    pub fn with_op_index(op: Opcode, d: Addr, s0: Addr, s1: Addr) -> Self {
        Self::with_op_t_index(op, None, None, d, s0, s1)
    }

    pub fn with_op_t(op: Opcode, t0: Option<ValueType>, t1: Option<ValueType>) -> Self {
        Self::with_op_t_index(op, t0, t1, Addr::Void, Addr::Void, Addr::Void)
    }

    pub fn with_op_t_index(
        op: Opcode,
        t0: Option<ValueType>,
        t1: Option<ValueType>,
        d: Addr,
        s0: Addr,
        s1: Addr,
    ) -> Self {
        Self {
            op0: op,
            op1: Opcode::VOID,
            t0: t0.unwrap_or(ValueType::Void),
            t1: t1.unwrap_or(ValueType::Void),
            d,
            s0,
            s1,
        }
    }

    pub fn set_op1_with_t(&mut self, t: ValueType) {
        self.op1 = unsafe { std::mem::transmute(t) }
    }

    pub fn into_runtime_inst(
        self,
        reg_base: usize,
        ast_objs: &AstObjects,
        packages: &PackageObjs,
        inst_index: usize,
        labels: &Map<TCObjKey, usize>,
        cst_map: &Map<usize, usize>,
    ) -> Instruction {
        Instruction {
            op0: self.op0,
            op1: self.op1,
            t0: self.t0,
            t1: self.t1,
            d: self
                .d
                .into_index(reg_base, ast_objs, packages, inst_index, labels, cst_map),
            s0: self
                .s0
                .into_index(reg_base, ast_objs, packages, inst_index, labels, cst_map),
            s1: self
                .s1
                .into_index(reg_base, ast_objs, packages, inst_index, labels, cst_map),
        }
    }
}

pub enum RightHandSide<'a> {
    Nothing,
    Values(&'a Vec<Expr>),
    Range(&'a Expr),
    SelectRecv(Addr, bool),
}

#[derive(Clone, Copy, Debug)]
pub enum CallStyle {
    Default,
    Async,
    Defer,
}

impl CallStyle {
    pub fn into_flag(&self) -> ValueType {
        match self {
            CallStyle::Default => ValueType::FlagA,
            CallStyle::Async => ValueType::FlagB,
            CallStyle::Defer => ValueType::FlagC,
        }
    }
}

pub(crate) struct FuncCtx<'c> {
    pub f_key: FunctionKey,
    pub tc_key: Option<TCTypeKey>, // for casting return values to interfaces
    consts: &'c Consts,

    code: Vec<InterInst>,
    pos: Vec<Option<usize>>,
    pub up_ptrs: Vec<ValueDesc>,
    local_zeros: Vec<GosValue>,

    entities: Map<TCObjKey, Addr>,
    uv_entities: Map<TCObjKey, Addr>,
    local_alloc: usize,
}

impl<'a> FuncCtx<'a> {
    pub fn new(f_key: FunctionKey, tc_key: Option<TCTypeKey>, consts: &'a Consts) -> Self {
        Self {
            f_key,
            tc_key,
            consts,
            code: vec![],
            pos: vec![],
            up_ptrs: vec![],
            local_zeros: vec![],
            entities: Map::new(),
            uv_entities: Map::new(),
            local_alloc: 0,
        }
    }

    pub fn is_ctor(&self, funcs: &FunctionObjs) -> bool {
        funcs[self.f_key].is_ctor()
    }

    pub fn offset(&self, loc: usize) -> OpIndex {
        // todo: don't crash if OpIndex overflows
        OpIndex::try_from((self.code.len() - loc) as isize).unwrap()
    }

    pub fn next_code_index(&self) -> usize {
        self.code.len()
    }

    pub fn inst_mut(&mut self, i: usize) -> &mut InterInst {
        self.code.get_mut(i).unwrap()
    }

    pub fn entity_index(&self, entity: &TCObjKey) -> Option<&Addr> {
        self.entities.get(entity)
    }

    pub fn add_const(&self, cst: GosValue) -> Addr {
        Addr::Const(self.consts.add_const(cst))
    }

    pub fn add_method(&self, obj_type: Meta, index: usize) -> Addr {
        Addr::Const(self.consts.add_method(obj_type, index))
    }

    pub fn add_metadata(&mut self, meta: Meta) -> Addr {
        self.add_const(FfiCtx::new_metadata(meta))
    }

    pub fn add_package(&mut self, pkg: PackageKey) -> Addr {
        self.add_const(FfiCtx::new_package(pkg))
    }

    pub fn add_const_var(&mut self, entity: TCObjKey, cst: GosValue) -> Addr {
        let addr = Addr::Const(self.consts.add_const(cst));
        let old = self.entities.insert(entity, addr);
        assert_eq!(old, None);
        addr
    }

    pub fn add_local(&mut self, entity: Option<TCObjKey>, zero_val: Option<GosValue>) -> Addr {
        let addr = Addr::LocalVar(self.local_alloc);
        if let Some(key) = entity {
            let old = self.entities.insert(key, addr);
            assert_eq!(old, None);
        };
        self.local_alloc += 1;

        if let Some(zero) = zero_val {
            self.local_zeros.push(zero);
        }
        addr
    }

    pub(crate) fn add_upvalue(&mut self, entity: &TCObjKey, uv: ValueDesc) -> VirtualAddr {
        let addr = match self.uv_entities.get(entity) {
            Some(i) => *i,
            None => {
                self.up_ptrs.push(uv);
                let i = (self.up_ptrs.len() - 1).try_into().unwrap();
                let et = Addr::Imm(i);
                self.uv_entities.insert(*entity, et);
                et
            }
        };
        VirtualAddr::UpValue(addr)
    }

    pub(crate) fn add_params(
        &mut self,
        fl: &FieldList,
        o: &AstObjects,
        t_lookup: &TypeLookup,
    ) -> usize {
        fl.list
            .iter()
            .map(|f| {
                let names = &o.fields[*f].names;
                if names.len() == 0 {
                    self.add_local(None, None);
                    1
                } else {
                    names
                        .iter()
                        .map(|n| {
                            let key = t_lookup.object_def(*n);
                            self.add_local(Some(key), None);
                        })
                        .count()
                }
            })
            .sum()
    }

    pub fn emit_assign(
        &mut self,
        lhs: VirtualAddr,
        rhs: Addr,
        op_ex: Option<(Opcode, ValueType, Option<ValueType>)>,
        pos: Option<usize>,
    ) {
        if lhs.is_blank() {
            return;
        }
        let mut direct = false;
        let mut inst_ex = None;
        let mut inst = match lhs {
            VirtualAddr::Direct(l) => {
                direct = true;
                match op_ex {
                    Some((op, t0, t1)) => {
                        let ass_op = match op {
                            Opcode::ADD => Opcode::ADD_ASSIGN,         // +=
                            Opcode::SUB => Opcode::SUB_ASSIGN,         // -=
                            Opcode::MUL => Opcode::MUL_ASSIGN,         // *=
                            Opcode::QUO => Opcode::QUO_ASSIGN,         // /=
                            Opcode::REM => Opcode::REM_ASSIGN,         // %=
                            Opcode::AND => Opcode::AND_ASSIGN,         // &=
                            Opcode::OR => Opcode::OR_ASSIGN,           // |=
                            Opcode::XOR => Opcode::XOR_ASSIGN,         // ^=
                            Opcode::SHL => Opcode::SHL_ASSIGN,         // <<=
                            Opcode::SHR => Opcode::SHR_ASSIGN,         // >>=
                            Opcode::AND_NOT => Opcode::AND_NOT_ASSIGN, // &^=
                            Opcode::INC => Opcode::INC,
                            Opcode::DEC => Opcode::DEC,
                            _ => {
                                dbg!(op);
                                unreachable!()
                            }
                        };
                        InterInst::with_op_t_index(ass_op, Some(t0), t1, l, rhs, Addr::Void)
                    }
                    None => InterInst::with_op_index(Opcode::DUPLICATE, l, rhs, Addr::Void),
                }
            }
            VirtualAddr::UpValue(l) => {
                InterInst::with_op_index(Opcode::STORE_UP_VALUE, l, rhs, Addr::Void)
            }
            VirtualAddr::SliceEntry(s, i) => {
                InterInst::with_op_index(Opcode::STORE_SLICE, s, i, rhs)
            }
            VirtualAddr::ArrayEntry(a, i) => {
                InterInst::with_op_index(Opcode::STORE_ARRAY, a, i, rhs)
            }
            VirtualAddr::MapEntry(m, k, zero) => {
                inst_ex = Some(InterInst::with_op_index(
                    Opcode::VOID,
                    Addr::Void,
                    zero,
                    Addr::Void,
                ));
                InterInst::with_op_index(Opcode::STORE_MAP, m, k, rhs)
            }
            VirtualAddr::StructMember(s, i) => {
                InterInst::with_op_index(Opcode::STORE_STRUCT, s, i, rhs)
            }
            VirtualAddr::StructEmbedded(s, i) => {
                InterInst::with_op_index(Opcode::STORE_EMBEDDED, s, i, rhs)
            }
            VirtualAddr::PackageMember(p, i) => {
                InterInst::with_op_index(Opcode::STORE_PKG, p, i, rhs)
            }
            VirtualAddr::Pointee(p) => {
                InterInst::with_op_index(Opcode::STORE_POINTER, p, rhs, Addr::Void)
            }
            VirtualAddr::Blank => unreachable!(),
        };
        if !direct {
            if let Some((op, t0, t1)) = op_ex {
                inst.op1 = op;
                inst.t0 = t0;
                if let Some(t) = t1 {
                    inst.t1 = t;
                }
            };
        }
        self.emit_inst(inst, pos);
        if let Some(i) = inst_ex {
            self.emit_inst(i, pos);
        }
    }

    pub fn emit_load_pkg(&mut self, d: Addr, pkg: Addr, index: Addr, pos: Option<usize>) {
        let inst = InterInst::with_op_index(Opcode::LOAD_PKG, d, pkg, index);
        self.emit_inst(inst, pos);
    }

    pub fn emit_literal(
        &mut self,
        d: Addr,
        begin: usize,
        count: usize,
        meta: Addr,
        pos: Option<usize>,
    ) {
        let inst = InterInst::with_op_index(
            Opcode::LITERAL,
            d,
            Addr::Regsiter(begin),
            Addr::Imm(count as OpIndex),
        );
        self.emit_inst(inst, pos);
        let mut inst_ex = InterInst::with_op(Opcode::VOID);
        inst_ex.s0 = meta;
        self.emit_inst(inst_ex, pos);
    }

    pub fn emit_cast(
        &mut self,
        d: Addr,
        s0: Addr,
        s1: Addr,
        to_type: ValueType,
        from_type: Option<ValueType>,
        extra_type: Option<ValueType>,
        pos: Option<usize>,
    ) {
        let mut inst =
            InterInst::with_op_t_index(Opcode::CAST, Some(to_type), from_type, d, s0, s1);
        if let Some(t) = extra_type {
            inst.set_op1_with_t(t);
        }
        self.emit_inst(inst, pos);
    }

    pub fn emit_cast_iface(&mut self, d: Addr, s: Addr, index: OpIndex, pos: Option<usize>) {
        self.emit_cast(
            d,
            s,
            Addr::Imm(index),
            ValueType::Interface,
            None,
            None,
            pos,
        );
    }

    pub fn emit_closure(&mut self, d: Addr, s: Addr, pos: Option<usize>) {
        let inst = InterInst::with_op_index(Opcode::CLOSURE, d, s, Addr::Void);
        self.emit_inst(inst, pos);
    }

    pub fn emit_jump(&mut self, offset: OpIndex, pos: Option<usize>) {
        let inst =
            InterInst::with_op_index(Opcode::JUMP, Addr::Imm(offset), Addr::Void, Addr::Void);
        self.emit_inst(inst, pos);
    }

    pub fn emit_call(
        &mut self,
        cls: Addr,
        stack_base: usize,
        style: CallStyle,
        pos: Option<usize>,
    ) {
        let flag = style.into_flag();
        let inst = InterInst::with_op_t_index(
            Opcode::CALL,
            Some(flag),
            None,
            cls,
            Addr::Regsiter(stack_base),
            Addr::Void,
        );
        self.emit_inst(inst, pos);
    }

    pub fn emit_return(
        &mut self,
        pkg: Option<PackageKey>,
        pos: Option<usize>,
        fobjs: &FunctionObjs,
    ) {
        let flag = match fobjs[self.f_key].flag {
            FuncFlag::Default => ValueType::FlagA,
            FuncFlag::PkgCtor => ValueType::FlagB,
            FuncFlag::HasDefer => ValueType::FlagC,
        };
        let mut inst = InterInst::with_op_t(Opcode::RETURN, Some(flag), None);
        if let Some(p) = pkg {
            inst.d = self.add_package(p);
        }
        self.emit_inst(inst, pos);
    }

    pub fn emit_import(&mut self, pkg: PackageKey, pos: Option<usize>) {
        let pkg_addr = self.add_package(pkg);
        let zero_addr = Addr::Const(self.consts.add_const(0i32.into()));
        let imm0 = Addr::Imm(0);
        let reg0 = Addr::Regsiter(0);
        let reg1 = Addr::Regsiter(1);
        let cd = vec![
            InterInst::with_op_index(Opcode::LOAD_PKG, reg0, pkg_addr, imm0),
            InterInst::with_op_t_index(
                Opcode::CALL,
                Some(CallStyle::Default.into_flag()),
                None,
                reg0,
                reg0,
                Addr::Void,
            ),
            // call init functions
            // 1. init a temp var at reg0 as 0
            InterInst::with_op_index(Opcode::DUPLICATE, reg0, zero_addr, Addr::Void),
            // 2. load function to reg1 and do reg0++
            //  or jump 2 if loading failed
            InterInst::with_op_index(Opcode::LOAD_INIT_FUNC, reg1, pkg_addr, reg0),
            InterInst::with_op_t_index(
                Opcode::CALL,
                Some(CallStyle::Default.into_flag()),
                None,
                reg1,
                reg1,
                Addr::Void,
            ),
            // jump back to LOAD_PKG_INIT_FUNC
            InterInst::with_op_index(Opcode::JUMP, Addr::Imm(-3), Addr::Void, Addr::Void),
        ];
        let offset = Addr::Imm(cd.len() as OpIndex);
        let inst = InterInst::with_op_index(Opcode::IMPORT, offset, pkg_addr, Addr::Void);
        self.emit_inst(inst, pos);
        for i in cd.into_iter() {
            self.emit_inst(i, pos);
        }
    }

    pub fn into_runtime_func(
        self,
        asto: &AstObjects,
        vmctx: &mut CodeGenVMCtx,
        labels: &Map<TCObjKey, usize>,
        cst_map: &Map<usize, usize>,
    ) {
        let code: Vec<Instruction> = self
            .code
            .into_iter()
            .enumerate()
            .map(|(i, x)| {
                x.into_runtime_inst(self.local_alloc, asto, vmctx.packages(), i, labels, cst_map)
            })
            .collect();
        let func = &mut vmctx.functions_mut()[self.f_key];
        func.pos = self.pos;
        func.up_ptrs = self.up_ptrs;
        func.max_write_index = Instruction::max_write_index(&code);
        func.local_zeros = self.local_zeros;
        func.code = code;
    }

    pub fn emit_inst(&mut self, i: InterInst, pos: Option<usize>) {
        self.code.push(i);
        self.pos.push(pos);
    }
}
