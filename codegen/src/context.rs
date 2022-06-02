// Copyright 2022 The Goscript Authors. All rights reserved.
// Use of this source code is governed by a BSD-style
// license that can be found in the LICENSE file.

use super::consts::Consts;
use super::types::TypeLookup;
use goscript_parser::ast::*;
use goscript_parser::objects::{IdentKey, Objects as AstObjects};
use goscript_parser::Pos;
use goscript_types::{ObjKey as TCObjKey, TypeKey as TCTypeKey};
use goscript_vm::instruction::Instruction;
use goscript_vm::metadata::*;
use goscript_vm::objects::FunctionObjs;
use goscript_vm::value::*;
use std::collections::HashMap;
use std::convert::TryFrom;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Addr {
    Const(OpIndex),
    LocalVar(OpIndex),
    Regsiter(OpIndex),
    Imm(OpIndex),
    PkgMemberIndex(PackageKey, IdentKey), // deferred resolve
    Label(TCObjKey),                      // deferred resolve
    Void,
}

impl Addr {
    pub fn as_var_index(self) -> OpIndex {
        match self {
            Self::LocalVar(i) => i,
            _ => unreachable!(),
        }
    }

    fn into_index(
        self,
        reg_base: OpIndex,
        ast_objs: &AstObjects,
        packages: &PackageObjs,
        inst_index: usize,
        labels: &HashMap<TCObjKey, usize>,
    ) -> OpIndex {
        match self {
            Self::Const(i) => -i - 1,
            Self::LocalVar(i) => i,
            Self::Regsiter(i) => reg_base + i,
            Self::PkgMemberIndex(key, ident) => {
                let pkg = &packages[key];
                let id = &ast_objs.idents[ident];
                *pkg.get_member_index(&id.name).unwrap()
            }
            Self::Label(key) => {
                let label_offset = labels[&key];
                (label_offset as OpIndex) - (inst_index as OpIndex) - 1
            }
            _ => unreachable!(),
        }
    }
}

#[derive(Clone, Debug)]
pub enum VirtualAddr {
    Direct(Addr),
    UpValue(Addr),
    SliceEntry(Addr, Addr),
    ArrayEntry(Addr, Addr),
    MapEntry(Addr, Addr),
    StructMember(Addr, Addr),
    StructEmbedded(Addr, Addr),
    PackageMember(Addr, Addr),
    Pointee(Addr),
    Blank,
    ZeroValue,
}

impl VirtualAddr {
    pub fn new_reg(reg_index: OpIndex) -> VirtualAddr {
        VirtualAddr::Direct(Addr::Regsiter(reg_index))
    }

    pub fn as_direct_addr(&self) -> Addr {
        match self {
            Self::Direct(a) => *a,
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
}

pub struct ExprCtx {
    pub mode: ExprMode,
    pub cur_reg: OpIndex,
    pub load_addr: Addr,
}

impl ExprCtx {
    pub fn new(mode: ExprMode, init_reg: OpIndex) -> Self {
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
        }
    }

    pub fn copy_result_from(&mut self, other: &Self) {
        self.cur_reg = other.cur_reg;
        self.load_addr = other.load_addr;
    }

    pub fn emit_assign<F>(
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
                self.load_addr = self.alloc_reg();
                f(fctx, self.load_addr, pos);
            }
            ExprMode::Store(va, _) => match va {
                VirtualAddr::Direct(d) => {
                    f(fctx, d, pos);
                    Self::cast_to_iface(cast_index, fctx, d, d, pos);
                }
                _ => {
                    let d = self.alloc_reg();
                    f(fctx, d, pos);
                    Self::cast_to_iface(cast_index, fctx, d, d, pos);
                    fctx.emit_assign(va.clone(), d, pos);
                }
            },
        }
    }

    pub fn emit_direct_assign(
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
                        fctx.emit_assign(va, src, pos)
                    }
                }
                _ => {
                    let reg = self.alloc_reg();
                    let src = if Self::cast_to_iface(cast_index, fctx, reg, src, pos) {
                        reg
                    } else {
                        src
                    };
                    fctx.emit_assign(VirtualAddr::Direct(reg), src, pos);
                }
            },
        }
    }

    fn alloc_reg(&mut self) -> Addr {
        let r = Addr::Regsiter(self.cur_reg);
        self.cur_reg += 1;
        r
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
                fctx.emit_cast(dst, src, Addr::Imm(index), ValueType::Interface, None, pos);
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
        reg_base: OpIndex,
        ast_objs: &AstObjects,
        packages: &PackageObjs,
        inst_index: usize,
        labels: &HashMap<TCObjKey, usize>,
    ) -> Instruction {
        Instruction {
            op0: self.op0,
            op1: self.op1,
            t0: self.t0,
            t1: self.t1,
            d: self
                .d
                .into_index(reg_base, ast_objs, packages, inst_index, labels),
            s0: self
                .s0
                .into_index(reg_base, ast_objs, packages, inst_index, labels),
            s1: self
                .s1
                .into_index(reg_base, ast_objs, packages, inst_index, labels),
        }
    }
}

pub enum RightHandSide<'a> {
    Nothing,
    Values(&'a Vec<Expr>),
    Range(&'a Expr),
    SelectRecv(&'a Expr),
}

#[derive(Clone, Copy, Debug)]
pub enum CallStyle {
    Default,
    Async,
    Defer,
}

pub struct FuncCtx<'c> {
    pub f_key: FunctionKey,
    pub tc_key: Option<TCTypeKey>, // for casting return values to interfaces
    consts: &'c Consts,
    pub max_reg_num: OpIndex, // how many temporary spots (register) on stack needed

    stack_temp_types: Vec<ValueType>,
    code: Vec<InterInst>,
    pos: Vec<Option<usize>>,
    up_ptrs: Vec<ValueDesc>,
    local_zeros: Vec<GosValue>,

    entities: HashMap<TCObjKey, Addr>,
    uv_entities: HashMap<TCObjKey, Addr>,
    local_alloc: OpIndex,
}

impl<'a> FuncCtx<'a> {
    pub fn new(f_key: FunctionKey, tc_key: Option<TCTypeKey>, consts: &'a Consts) -> Self {
        Self {
            f_key,
            tc_key,
            consts,
            max_reg_num: 0,
            stack_temp_types: vec![],
            code: vec![],
            pos: vec![],
            up_ptrs: vec![],
            local_zeros: vec![],
            entities: HashMap::new(),
            uv_entities: HashMap::new(),
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

    pub fn add_const(&mut self, cst: GosValue) -> Addr {
        Addr::Const(self.consts.add_const(cst))
    }

    pub fn add_metadata(&mut self, meta: Meta) -> Addr {
        self.add_const(GosValue::new_metadata(meta))
    }

    pub fn add_package(&mut self, pkg: PackageKey) -> Addr {
        self.add_const(GosValue::new_package(pkg))
    }

    pub fn add_function(&self, obj_meta: Meta, index: OpIndex) -> Addr {
        Addr::Const(self.consts.add_function(obj_meta, index))
    }

    pub fn add_const_var(&mut self, entity: TCObjKey, cst: GosValue) -> Addr {
        let addr = Addr::Const(self.consts.add_const(cst));
        let old = self.entities.insert(entity, addr);
        assert_eq!(old, None);
        addr
    }

    pub fn add_local(
        &mut self,
        entity: Option<TCObjKey>,
        zero_val_type: Option<(GosValue, ValueType)>,
    ) -> Addr {
        let addr = Addr::LocalVar(self.local_alloc);
        if let Some(key) = entity {
            let old = self.entities.insert(key, addr);
            assert_eq!(old, None);
        };
        self.local_alloc += 1;

        if let Some((zero, typ)) = zero_val_type {
            self.local_zeros.push(zero);
            self.stack_temp_types.push(typ);
        }
        addr
    }

    pub fn add_upvalue(&mut self, entity: &TCObjKey, uv: ValueDesc) -> VirtualAddr {
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

    pub fn add_params(&mut self, fl: &FieldList, o: &AstObjects, t_lookup: &TypeLookup) -> usize {
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

    pub fn emit_assign(&mut self, lhs: VirtualAddr, rhs: Addr, pos: Option<usize>) {
        if lhs.is_blank() {
            return;
        }
        let inst = match lhs {
            VirtualAddr::Direct(l) => InterInst::with_op_index(Opcode::ASSIGN, l, rhs, Addr::Void),
            VirtualAddr::UpValue(l) => {
                InterInst::with_op_index(Opcode::STORE_UP_VALUE, l, rhs, Addr::Void)
            }
            VirtualAddr::SliceEntry(s, i) => {
                InterInst::with_op_index(Opcode::STORE_SLICE, s, i, rhs)
            }
            VirtualAddr::ArrayEntry(a, i) => {
                InterInst::with_op_index(Opcode::STORE_ARRAY, a, i, rhs)
            }
            VirtualAddr::MapEntry(m, k) => InterInst::with_op_index(Opcode::STORE_MAP, m, k, rhs),
            VirtualAddr::StructMember(s, i) => {
                InterInst::with_op_index(Opcode::STORE_STRUCT, s, i, rhs)
            }
            VirtualAddr::StructEmbedded(s, i) => {
                InterInst::with_op_index(Opcode::STORE_STRUCT_EMBEDDED, s, i, rhs)
            }
            VirtualAddr::PackageMember(p, i) => {
                InterInst::with_op_index(Opcode::STORE_PKG, p, i, rhs)
            }
            VirtualAddr::Pointee(p) => {
                InterInst::with_op_index(Opcode::STORE_POINTER, p, rhs, Addr::Void)
            }
            VirtualAddr::Blank => unreachable!(),
            VirtualAddr::ZeroValue => unreachable!(),
        };
        self.push_inst_pos(inst, pos);
    }

    pub fn emit_load_pkg(&mut self, d: Addr, pkg: Addr, index: Addr, pos: Option<usize>) {
        let inst = InterInst::with_op_index(Opcode::LOAD_PKG, d, pkg, index);
        self.push_inst_pos(inst, pos);
    }

    pub fn emit_literal(
        &mut self,
        d: Addr,
        begin: OpIndex,
        count: OpIndex,
        meta: Addr,
        pos: Option<usize>,
    ) {
        let inst = InterInst::with_op_index(Opcode::LITERAL, d, Addr::Imm(begin), Addr::Imm(count));
        self.push_inst_pos(inst, pos);
        let mut inst2 = InterInst::with_op(Opcode::VOID);
        inst2.s0 = meta;
        self.push_inst_pos(inst2, pos);
    }

    pub fn emit_cast(
        &mut self,
        d: Addr,
        s0: Addr,
        s1: Addr,
        to_type: ValueType,
        from_type: Option<ValueType>,
        pos: Option<usize>,
    ) {
        let inst = InterInst::with_op_t_index(Opcode::CAST, Some(to_type), from_type, d, s0, s1);
        self.push_inst_pos(inst, pos);
    }

    pub fn emit_closure(&mut self, d: Addr, s: Addr, pos: Option<usize>) {
        let inst = InterInst::with_op_index(Opcode::CLOSURE, d, s, Addr::Void);
        self.push_inst_pos(inst, pos);
    }

    pub fn emit_jump(&mut self, offset: OpIndex, pos: Option<usize>) {
        let inst =
            InterInst::with_op_index(Opcode::JUMP, Addr::Imm(offset), Addr::Void, Addr::Void);
        self.push_inst_pos(inst, pos);
    }

    pub fn emit_pre_call(
        &mut self,
        cls: Addr,
        stack_base: OpIndex,
        param_count: OpIndex,
        pos: Option<usize>,
    ) {
        let inst = InterInst::with_op_index(
            Opcode::PRE_CALL,
            cls,
            Addr::Imm(stack_base),
            Addr::Imm(param_count),
        );
        self.push_inst_pos(inst, pos);
    }

    pub fn emit_call(&mut self, style: CallStyle, pos: Option<usize>) {
        let flag = match style {
            CallStyle::Default => ValueType::Void,
            CallStyle::Async => ValueType::FlagA,
            CallStyle::Defer => ValueType::FlagB,
        };
        let mut inst = InterInst::with_op(Opcode::CALL);
        inst.t0 = flag;
        self.push_inst_pos(inst, pos);
    }

    pub fn emit_return(
        &mut self,
        pkg: Option<PackageKey>,
        pos: Option<usize>,
        fobjs: &FunctionObjs,
    ) {
        let flag = match fobjs[self.f_key].flag {
            FuncFlag::Default => ValueType::Void,
            FuncFlag::PkgCtor => ValueType::FlagA,
            FuncFlag::HasDefer => ValueType::FlagB,
        };
        let mut inst = InterInst::with_op(Opcode::CALL);
        inst.t0 = flag;
        if let Some(p) = pkg {
            inst.d = self.add_package(p);
        }
        self.push_inst_pos(inst, pos);
    }

    pub fn emit_import(&mut self, pkg: PackageKey, pos: Option<usize>) {
        let pkg_addr = self.add_package(pkg);
        let zero_addr = Addr::Const(self.consts.add_const(GosValue::new_int32(0)));
        let imm0 = Addr::Imm(0);
        let cd = vec![
            InterInst::with_op_index(Opcode::LOAD_PKG, Addr::Regsiter(0), pkg_addr, imm0),
            InterInst::with_op_index(Opcode::PRE_CALL, Addr::Regsiter(0), imm0, imm0),
            InterInst::with_op_t(Opcode::CALL, Some(ValueType::Closure), None),
            // call init functions
            // 1. init a temp var at reg0 as 0
            InterInst::with_op_index(Opcode::ASSIGN, Addr::Regsiter(0), zero_addr, Addr::Void),
            // 2. load function to reg1 and do reg0++
            //  or jump 3 if loading failed
            InterInst::with_op_index(
                Opcode::LOAD_PKG_INIT_FUNC,
                Addr::Regsiter(1),
                pkg_addr,
                Addr::Regsiter(0),
            ),
            InterInst::with_op_index(Opcode::PRE_CALL, Addr::Regsiter(1), imm0, imm0),
            InterInst::with_op(Opcode::CALL),
            // jump back to LOAD_PKG_INIT_FUNC
            InterInst::with_op_index(Opcode::JUMP, Addr::Imm(-4), Addr::Void, Addr::Void),
        ];
        let offset = Addr::Imm(cd.len() as OpIndex);
        let inst = InterInst::with_op_index(Opcode::IMPORT, offset, pkg_addr, Addr::Void);
        self.push_inst_pos(inst, pos);
        for i in cd.into_iter() {
            self.push_inst_pos(i, pos);
        }

        self.update_max_reg(2);
    }

    pub fn into_runtime_func(
        mut self,
        asto: &AstObjects,
        vmo: &mut VMObjects,
        labels: &HashMap<TCObjKey, usize>,
    ) {
        let func = &mut vmo.functions[self.f_key];
        func.stack_temp_types.append(&mut self.stack_temp_types);
        func.pos = self.pos;
        func.up_ptrs = self.up_ptrs;
        func.local_zeros = self.local_zeros;
        func.code = self
            .code
            .into_iter()
            .enumerate()
            .map(|(i, x)| x.into_runtime_inst(self.local_alloc, asto, &vmo.packages, i, labels))
            .collect();
    }

    pub fn push_inst_pos(&mut self, i: InterInst, pos: Option<usize>) {
        self.code.push(i);
        self.pos.push(pos);
    }

    pub fn update_max_reg(&mut self, max: OpIndex) {
        if self.max_reg_num < max {
            self.max_reg_num = max
        }
    }
}
