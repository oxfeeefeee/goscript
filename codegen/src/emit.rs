#![allow(dead_code)]

use super::package::PkgVarPairs;
use super::types::TypeLookup;
use goscript_parser::ast::*;
use goscript_parser::objects::Objects as AstObjects;
use goscript_vm::instruction::*;
use goscript_vm::objects::{key_to_u64, EntIndex, FunctionVal};
use goscript_vm::value::*;
use slotmap::KeyData;
use std::convert::TryFrom;

#[derive(Clone, Copy, Debug)]
pub enum CallStyle {
    Default,
    Async,
    Defer,
}

#[derive(Clone, Copy, Debug)]
pub enum IndexSelType {
    Indexing,
    StructField,
}

#[derive(Clone, Copy, Debug)]
pub struct IndexSelInfo {
    pub index: i8,
    pub imm_index: Option<OpIndex>, // for IMM instructions
    pub t1: ValueType,
    pub t2: Option<ValueType>, // for non-IMM instructions
    pub typ: IndexSelType,
}

impl IndexSelInfo {
    pub fn new(
        index: i8,
        imm_index: Option<OpIndex>,
        t1: ValueType,
        t2: Option<ValueType>,
        typ: IndexSelType,
    ) -> IndexSelInfo {
        IndexSelInfo {
            index: index,
            imm_index: imm_index,
            t1: t1,
            t2: t2,
            typ: typ,
        }
    }

    pub fn with_index(&self, i: OpIndex) -> IndexSelInfo {
        let mut v = *self;
        v.index = i8::try_from(i).unwrap();
        v
    }

    pub fn stack_space(&self) -> OpIndex {
        if self.t2.is_some() {
            2
        } else {
            1
        }
    }
}

/// LeftHandSide represents the left hand side of an assign stmt
/// Primitive stores index of lhs variable
/// IndexSelExpr stores the info of index or selection lhs
/// Deref stores the index of lhs on the stack
#[derive(Clone, Debug)]
pub enum LeftHandSide {
    Primitive(EntIndex),
    IndexSelExpr(IndexSelInfo),
    Deref(OpIndex),
}

pub enum RightHandSide<'a> {
    Nothing,
    Values(&'a Vec<Expr>),
    Range(&'a Expr),
    SelectRecv(&'a Expr),
}

pub struct Emitter<'a> {
    pub f: &'a mut FunctionVal,
}

impl<'a> Emitter<'a> {
    pub fn new(f: &mut FunctionVal) -> Emitter {
        Emitter { f }
    }

    pub fn add_const(&mut self, entity: Option<KeyData>, cst: GosValue) -> EntIndex {
        self.f.add_const(entity, cst)
    }

    pub fn add_params(&mut self, fl: &FieldList, o: &AstObjects, t_lookup: &TypeLookup) -> usize {
        fl.list
            .iter()
            .map(|f| {
                let names = &o.fields[*f].names;
                if names.len() == 0 {
                    self.f.add_local(None);
                    1
                } else {
                    names
                        .iter()
                        .map(|n| {
                            let key = t_lookup.get_def_object(*n);
                            self.f.add_local(Some(key.into()));
                        })
                        .count()
                }
            })
            .sum()
    }

    #[inline]
    fn try_imm<T: TryInto<OpIndex>>(&mut self, i: T, typ: ValueType, pos: Option<usize>) -> bool {
        match T::try_into(i) {
            Ok(imm) => {
                self.emit_push_imm(typ, imm, pos);
                true
            }
            Err(_) => false,
        }
    }

    pub fn emit_load(
        &mut self,
        index: EntIndex,
        patch_info: Option<(&mut PkgVarPairs, FunctionKey)>,
        typ: ValueType,
        pos: Option<usize>,
    ) {
        match index {
            EntIndex::Const(i) => {
                let done = match self.f.const_val(i).clone() {
                    GosValue::Bool(b) => {
                        let op = if b {
                            Opcode::PUSH_TRUE
                        } else {
                            Opcode::PUSH_FALSE
                        };
                        self.f.emit_code(op, pos);
                        true
                    }
                    GosValue::Int(i) => self.try_imm(i, typ, pos),
                    GosValue::Int8(i) => self.try_imm(i, typ, pos),
                    GosValue::Int16(i) => self.try_imm(i, typ, pos),
                    GosValue::Int32(i) => self.try_imm(i, typ, pos),
                    GosValue::Int64(i) => self.try_imm(i, typ, pos),
                    GosValue::Uint(i) => self.try_imm(i, typ, pos),
                    GosValue::Uint8(i) => self.try_imm(i, typ, pos),
                    GosValue::Uint16(i) => self.try_imm(i, typ, pos),
                    GosValue::Uint32(i) => self.try_imm(i, typ, pos),
                    GosValue::Uint64(i) => self.try_imm(i, typ, pos),
                    _ => false,
                };
                if !done {
                    self.f
                        .emit_inst(Opcode::PUSH_CONST, [Some(typ), None, None], Some(i), pos);
                }
            }
            EntIndex::LocalVar(i) => {
                self.f
                    .emit_inst(Opcode::LOAD_LOCAL, [Some(typ), None, None], Some(i), pos);
            }
            EntIndex::UpValue(i) => {
                self.f
                    .emit_inst(Opcode::LOAD_UPVALUE, [Some(typ), None, None], Some(i), pos);
            }
            EntIndex::PackageMember(pkg, ident) => {
                self.f.emit_inst(
                    Opcode::LOAD_PKG_FIELD,
                    [Some(typ), None, None],
                    Some(0),
                    pos,
                );
                self.f.emit_raw_inst(key_to_u64(pkg), pos);
                let (pairs, func) = patch_info.unwrap();
                pairs.add_pair(pkg, ident.into(), func, self.f.code().len() - 2, false);
            }
            EntIndex::BuiltInVal(op) => self.f.emit_code(op, pos),
            EntIndex::BuiltInType(m) => {
                let i = self.f.add_const(None, GosValue::Metadata(m));
                self.emit_load(i, None, ValueType::Metadata, pos);
            }
            EntIndex::Blank => unreachable!(),
        }
    }

    pub fn emit_store(
        &mut self,
        lhs: &LeftHandSide,
        rhs_index: OpIndex,
        op: Option<(Opcode, Option<ValueType>)>,
        patch_info: Option<(&mut PkgVarPairs, FunctionKey)>,
        typ: ValueType,
        pos: Option<usize>,
    ) {
        if let LeftHandSide::Primitive(index) = lhs {
            if EntIndex::Blank == *index {
                return;
            }
        }

        let mut pkg_info = None;
        let (code, int32, t1, t2, int8) = match lhs {
            LeftHandSide::Primitive(index) => match index {
                EntIndex::Const(_) => unreachable!(),
                EntIndex::LocalVar(i) => (Opcode::STORE_LOCAL, *i, None, None, None),
                EntIndex::UpValue(i) => (Opcode::STORE_UPVALUE, *i, None, None, None),
                EntIndex::PackageMember(pkg, ident) => {
                    pkg_info = Some((*pkg, *ident));
                    (
                        Opcode::STORE_PKG_FIELD,
                        0,
                        Some(ValueType::Package),
                        None,
                        None,
                    )
                }
                EntIndex::BuiltInVal(_) => unreachable!(),
                EntIndex::BuiltInType(_) => unreachable!(),
                EntIndex::Blank => unreachable!(),
            },
            LeftHandSide::IndexSelExpr(info) => match info.typ {
                IndexSelType::Indexing => match info.imm_index {
                    Some(i) => (
                        Opcode::STORE_INDEX_IMM,
                        i,
                        Some(info.t1),
                        None,
                        Some(info.index),
                    ),

                    None => (
                        Opcode::STORE_INDEX,
                        info.index as i32,
                        Some(info.t1),
                        info.t2,
                        None,
                    ),
                },
                IndexSelType::StructField => (
                    Opcode::STORE_STRUCT_FIELD,
                    info.imm_index.unwrap(),
                    Some(info.t1),
                    None,
                    Some(info.index),
                ),
            },
            LeftHandSide::Deref(i) => (Opcode::STORE_DEREF, *i, None, None, None),
        };
        let mut inst = Instruction::new(code, Some(typ), t1, t2, None);
        if let Some(i) = int8 {
            inst.set_t2_with_index(i);
        }
        assert!(rhs_index == -1 || op.is_none());
        let imm0 = op.map_or(rhs_index, |(code, shift_t)| {
            if let Some(t) = shift_t {
                // there is no space left to store the type of the rhs operand.
                // emit a (possibly temporary) ZERO to carry it.
                // only used by SHL SHR
                self.emit_cast(ValueType::Uint32, t, None, -1, 0, pos);
            }
            Instruction::code2index(code)
        });
        inst.set_imm824(imm0, int32);
        self.f.push_inst_pos(inst, pos);
        if let Some((pkg, ident)) = pkg_info {
            self.f.emit_raw_inst(key_to_u64(pkg), pos);
            let (pairs, func) = patch_info.unwrap();
            pairs.add_pair(pkg, ident.into(), func, self.f.code().len() - 2, true);
        }
    }

    pub fn emit_cast(
        &mut self,
        t0: ValueType,
        t1: ValueType,
        t2: Option<ValueType>,
        rhs: OpIndex,
        m_index: OpIndex,
        pos: Option<usize>,
    ) {
        let mut inst = Instruction::new(Opcode::CAST, Some(t0), Some(t1), t2, None);
        inst.set_imm824(rhs, m_index);
        self.f.push_inst_pos(inst, pos);
    }

    pub fn emit_import(&mut self, index: OpIndex, pkg: PackageKey, pos: Option<usize>) {
        self.f
            .emit_inst(Opcode::IMPORT, [None, None, None], Some(index), pos);
        let cd = vec![
            // init package vars
            Instruction::new(
                Opcode::LOAD_PKG_FIELD,
                Some(ValueType::Int),
                None,
                None,
                Some(0),
            ),
            Instruction::from_u64(key_to_u64(pkg)),
            Instruction::new(Opcode::PRE_CALL, Some(ValueType::Closure), None, None, None),
            Instruction::new(Opcode::CALL, None, None, None, None),
            // call init functions
            Instruction::new(Opcode::PUSH_IMM, Some(ValueType::Uint), None, None, Some(0)),
            Instruction::new(Opcode::LOAD_PKG_INIT, None, None, None, Some(0)),
            Instruction::from_u64(key_to_u64(pkg)),
            Instruction::new(Opcode::JUMP_IF_NOT, None, None, None, Some(3)),
            Instruction::new(Opcode::PRE_CALL, Some(ValueType::Closure), None, None, None),
            Instruction::new(Opcode::CALL, None, None, None, None),
            Instruction::new(Opcode::JUMP, None, None, None, Some(-6)),
        ];
        let offset = cd.len() as OpIndex;
        self.f
            .emit_inst(Opcode::JUMP_IF_NOT, [None, None, None], Some(offset), pos);
        for i in cd.into_iter() {
            self.f.push_inst_pos(i, pos);
        }
    }

    pub fn emit_pop(&mut self, count: OpIndex, pos: Option<usize>) {
        if count > 0 {
            self.f
                .emit_inst(Opcode::POP, [None, None, None], Some(count), pos);
        }
    }

    pub fn emit_load_struct_field(&mut self, imm: OpIndex, typ: ValueType, pos: Option<usize>) {
        self.f.emit_inst(
            Opcode::LOAD_STRUCT_FIELD,
            [Some(typ), None, None],
            Some(imm),
            pos,
        );
    }

    pub fn emit_load_index(
        &mut self,
        typ: ValueType,
        index_type: ValueType,
        comma_ok: bool,
        pos: Option<usize>,
    ) {
        let mut inst =
            Instruction::new(Opcode::LOAD_INDEX, Some(typ), Some(index_type), None, None);
        inst.set_t2_with_index(if comma_ok { 1 } else { 0 });
        self.f.push_inst_pos(inst, pos);
    }

    pub fn emit_load_index_imm(
        &mut self,
        imm: OpIndex,
        typ: ValueType,
        comma_ok: bool,
        pos: Option<usize>,
    ) {
        let mut inst = Instruction::new(Opcode::LOAD_INDEX_IMM, Some(typ), None, None, Some(imm));
        inst.set_t2_with_index(if comma_ok { 1 } else { 0 });
        self.f.push_inst_pos(inst, pos);
    }

    pub fn emit_return(&mut self, pkg_index: Option<OpIndex>, pos: Option<usize>) {
        let inst_flag = match self.f.flag {
            FuncFlag::Default => ValueType::Zero,
            FuncFlag::PkgCtor => ValueType::FlagA,
            FuncFlag::HasDefer => ValueType::FlagB,
        };
        self.f.emit_inst(
            Opcode::RETURN,
            [Some(inst_flag), None, None],
            pkg_index,
            pos,
        );
    }

    pub fn emit_pre_call(&mut self, pos: Option<usize>) {
        self.f
            .emit_inst(Opcode::PRE_CALL, [None, None, None], None, pos);
    }

    pub fn emit_call(&mut self, style: CallStyle, pack: bool, pos: Option<usize>) {
        let style_flag = match style {
            CallStyle::Default => ValueType::Zero,
            CallStyle::Async => ValueType::FlagA,
            CallStyle::Defer => ValueType::FlagB,
        };
        let pack_flag = if pack { Some(ValueType::FlagA) } else { None };
        self.f
            .emit_inst(Opcode::CALL, [Some(style_flag), pack_flag, None], None, pos);
    }

    pub fn emit_literal(&mut self, typ: ValueType, index: OpIndex, pos: Option<usize>) {
        self.f
            .emit_inst(Opcode::LITERAL, [Some(typ), None, None], Some(index), pos);
    }

    pub fn emit_push_imm(&mut self, typ: ValueType, imm: OpIndex, pos: Option<usize>) {
        self.f
            .emit_inst(Opcode::PUSH_IMM, [Some(typ), None, None], Some(imm), pos);
    }

    pub fn emit_push_zero_val(&mut self, imm: OpIndex, pos: Option<usize>) {
        self.f
            .emit_inst(Opcode::PUSH_ZERO_VALUE, [None, None, None], Some(imm), pos);
    }

    pub fn emit_raw_inst(&mut self, u: u64, pos: Option<usize>) {
        self.f.emit_raw_inst(u, pos);
    }

    pub fn emit_unwrap(&mut self, index: OpIndex, pos: Option<usize>) {
        self.f
            .emit_inst(Opcode::UNWRAP, [None, None, None], Some(index), pos);
    }

    pub fn emit_wrap(
        &mut self,
        t: ValueType,
        index: OpIndex,
        meta: Option<u64>,
        pos: Option<usize>,
    ) {
        let t1 = meta.map(|_| ValueType::FlagA);
        self.f
            .emit_inst(Opcode::WRAP, [Some(t), t1, None], Some(index), pos);
        if let Some(m) = meta {
            self.f.emit_raw_inst(m, pos);
        }
    }

    pub fn emit_ops(
        &mut self,
        code: Opcode,
        t0: ValueType,
        t1: Option<ValueType>,
        t0_inner: Option<ValueType>,
        t1_inner: Option<ValueType>,
        pos: Option<usize>,
    ) {
        let t1 = match t1_inner {
            Some(t) => {
                self.emit_unwrap(-1, pos);
                Some(t)
            }
            None => t1,
        };

        let t0 = match t0_inner {
            Some(t) => {
                let i = t1.map_or(-1, |_| -2);
                self.emit_unwrap(i, pos);
                t
            }
            None => t0,
        };

        self.f.emit_code_with_type2(code, t0, t1, pos);

        if let Some(t) = t0_inner {
            self.emit_wrap(t, -1, None, pos);
        }
    }
}
