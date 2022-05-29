// Copyright 2022 The Goscript Authors. All rights reserved.
// Use of this source code is governed by a BSD-style
// license that can be found in the LICENSE file.

use super::consts::Consts;
use super::types::TypeLookup;
use goscript_parser::ast::*;
use goscript_parser::objects::{IdentKey, Objects as AstObjects};
use goscript_types::TypeKey as TCTypeKey;
use goscript_vm::instruction::Instruction;
use goscript_vm::objects::FunctionObjs;
use goscript_vm::value::*;
use slotmap::{Key, KeyData};
use std::collections::HashMap;
use std::convert::TryFrom;

struct ExprCtx {
    pub cur_reg: OpIndex,
    pub max_reg: OpIndex,
    pub store_to: Option<OpIndex>,
}

impl ExprCtx {
    fn new(init_reg: OpIndex, store_to: Option<OpIndex>) -> Self {
        Self {
            cur_reg: init_reg,
            max_reg: init_reg,
            store_to,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Addr {
    Const(OpIndex),
    LocalVar(OpIndex),
    Regsiter(OpIndex),
    Imm(OpIndex),
    PkgMemberIndex(PackageKey, IdentKey), // deferred resolve
    Void,
}

impl Addr {
    fn into_index(
        self,
        reg_base: OpIndex,
        ast_objs: &AstObjects,
        packages: &PackageObjs,
    ) -> OpIndex {
        match self {
            Self::Const(i) => -i - 1,
            Self::LocalVar(i) => i,
            Self::Regsiter(i) => reg_base + 1,
            Self::PkgMemberIndex(key, ident) => {
                let pkg = &packages[key];
                let id = &ast_objs.idents[ident];
                *pkg.get_member_index(&id.name).unwrap()
            }
            _ => unreachable!(),
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
    fn with_op(op: Opcode) -> Self {
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
    ) -> Instruction {
        Instruction {
            op0: self.op0,
            op1: self.op1,
            t0: self.t0,
            t1: self.t1,
            d: self.d.into_index(reg_base, ast_objs, packages),
            s0: self.s0.into_index(reg_base, ast_objs, packages),
            s1: self.s1.into_index(reg_base, ast_objs, packages),
        }
    }
}

#[derive(Clone, Debug)]
pub enum Entry {
    Direct(Addr),
    SliceEntry(Addr, Addr),
    ArrayEntry(Addr, Addr),
    MapEntry(Addr, Addr),
    StructMember(Addr, Addr),
    StructEmbedded(Addr, Addr),
    PackageMember(Addr, Addr),
    Pointee(Addr),
    UpValue(Addr),
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

pub(crate) struct FuncCtx<'a> {
    pub f_key: FunctionKey,
    pub tc_key: Option<TCTypeKey>, // for casting return values to interfaces
    consts: &'a Consts,
    pub max_reg_num: OpIndex, // how many temporary spots (register) on stack needed

    stack_temp_types: Vec<ValueType>,
    code: Vec<InterInst>,
    pos: Vec<Option<usize>>,
    up_ptrs: Vec<ValueDesc>,
    local_zeros: Vec<GosValue>,

    entities: HashMap<KeyData, Addr>,
    uv_entities: HashMap<KeyData, Addr>,
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

    pub fn entity_index(&self, entity: &KeyData) -> Option<&Addr> {
        self.entities.get(entity)
    }

    pub fn add_const(&mut self, entity: Option<KeyData>, cst: GosValue) -> Entry {
        let i = self.consts.add_const(cst);
        let addr = Addr::Const(i);
        if let Some(e) = entity {
            let old = self.entities.insert(e, addr);
            assert_eq!(old, None);
        }
        Entry::Direct(addr)
    }

    pub fn add_local(
        &mut self,
        entity: Option<KeyData>,
        zero_val_type: Option<(GosValue, ValueType)>,
    ) -> Entry {
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

        Entry::Direct(addr)
    }

    pub fn add_upvalue(&mut self, entity: &KeyData, uv: ValueDesc) -> Entry {
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
        Entry::UpValue(addr)
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
                            self.add_local(Some(key.data()), None);
                        })
                        .count()
                }
            })
            .sum()
    }

    pub fn emit_pre_call(&mut self, stack_base: OpIndex, param_count: OpIndex, pos: Option<usize>) {
        let inst = InterInst::with_op_index(
            Opcode::PRE_CALL,
            Addr::Void,
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
        let index = pkg.map(|p| self.consts.add_package(p)).unwrap_or(0);
        let mut inst = InterInst::with_op(Opcode::CALL);
        inst.t0 = flag;
        inst.d = Addr::Const(index);
        self.push_inst_pos(inst, pos);
    }

    pub fn emit_import(&mut self, pkg: PackageKey, pos: Option<usize>) {
        // self.f
        //     .emit_inst(Opcode::IMPORT, [None, None, None], Some(index), pos);
        // let cd = vec![
        //     // init package vars
        //     Instruction::new(
        //         Opcode::LOAD_PKG_FIELD,
        //         Some(ValueType::Int),
        //         None,
        //         None,
        //         Some(0),
        //     ),
        //     Instruction::from_u64(key_to_u64(pkg)),
        //     Instruction::new(Opcode::PRE_CALL, Some(ValueType::Closure), None, None, None),
        //     Instruction::new(Opcode::CALL, None, None, None, None),
        //     // call init functions
        //     Instruction::new(
        //         Opcode::PUSH_IMM,
        //         Some(ValueType::Int32),
        //         None,
        //         None,
        //         Some(0),
        //     ),
        //     Instruction::new(Opcode::LOAD_PKG_INIT, None, None, None, Some(0)),
        //     Instruction::from_u64(key_to_u64(pkg)),
        //     Instruction::new(Opcode::JUMP_IF_NOT, None, None, None, Some(3)),
        //     Instruction::new(Opcode::PRE_CALL, Some(ValueType::Closure), None, None, None),
        //     Instruction::new(Opcode::CALL, None, None, None, None),
        //     Instruction::new(Opcode::JUMP, None, None, None, Some(-6)),
        // ];
        // let offset = cd.len() as OpIndex;
        // self.f
        //     .emit_inst(Opcode::JUMP_IF_NOT, [None, None, None], Some(offset), pos);
        // for i in cd.into_iter() {
        //     self.f.add_inst_pos(i, pos);
        //}
    }

    pub fn into_runtime_func(self, asto: &AstObjects, vmo: &mut VMObjects) {
        let func = &mut vmo.functions[self.f_key];
        func.stack_temp_types.append(&mut self.stack_temp_types);
        func.pos = self.pos;
        func.up_ptrs = self.up_ptrs;
        func.local_zeros = self.local_zeros;
        func.code = self
            .code
            .into_iter()
            .map(|x| x.into_runtime_inst(self.local_alloc, asto, &vmo.packages))
            .collect();
    }

    pub fn push_inst_pos(&mut self, i: InterInst, pos: Option<usize>) {
        self.code.push(i);
        self.pos.push(pos);
    }
}
