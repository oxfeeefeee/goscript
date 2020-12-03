#![allow(dead_code)]

use super::package::PkgVarPairs;
use goscript_parser::ast::*;
use goscript_parser::objects::Objects as AstObjects;
use goscript_vm::instruction::*;
use goscript_vm::objects::{key_to_u64, EntIndex, FunctionVal};
use goscript_vm::value::*;
use std::convert::TryFrom;

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
    pub typ: IndexSelType,     // is an index expresion not a selection expresion
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
}

///////////// todo: remove trait, remomve code_mut&pos_mut
pub trait FuncGen {
    fn add_params<'e>(&mut self, fl: &FieldList, o: &AstObjects) -> usize;

    fn emit_load(
        &mut self,
        index: EntIndex,
        patch_info: Option<(&mut PkgVarPairs, FunctionKey)>,
        typ: ValueType,
        pos: Option<usize>,
    );

    fn emit_store(
        &mut self,
        lhs: &LeftHandSide,
        rhs_index: OpIndex,
        op: Option<(Opcode, Option<ValueType>)>,
        patch_info: Option<(&mut PkgVarPairs, FunctionKey)>,
        typ: ValueType,
        pos: Option<usize>,
    );

    fn emit_import(&mut self, index: OpIndex, pkg: PackageKey, pos: Option<usize>);

    fn emit_pop(&mut self, count: OpIndex, pos: Option<usize>);

    fn emit_load_struct_field(&mut self, imm: OpIndex, typ: ValueType, pos: Option<usize>);

    fn emit_load_index(
        &mut self,
        typ: ValueType,
        sel_type: ValueType,
        comma_ok: bool,
        pos: Option<usize>,
    );

    fn emit_load_index_imm(
        &mut self,
        imm: OpIndex,
        typ: ValueType,
        comma_ok: bool,
        pos: Option<usize>,
    );

    fn emit_cast_to_interface(
        &mut self,
        typ: ValueType,
        rhs: OpIndex,
        m_index: OpIndex,
        pos: Option<usize>,
    );

    fn emit_return(&mut self, pos: Option<usize>);

    fn emit_return_init_pkg(&mut self, index: OpIndex, pos: Option<usize>);

    fn emit_pre_call(&mut self, pos: Option<usize>);

    fn emit_call(&mut self, has_ellipsis: bool, pos: Option<usize>);

    fn emit_new(&mut self, typ: ValueType, pos: Option<usize>);
}

impl FuncGen for FunctionVal {
    fn add_params<'e>(&mut self, fl: &FieldList, o: &AstObjects) -> usize {
        fl.list
            .iter()
            .map(|f| {
                let names = &o.fields[*f].names;
                if names.len() == 0 {
                    self.add_local(None);
                    1
                } else {
                    names
                        .iter()
                        .map(|n| {
                            let ident = &o.idents[*n];
                            self.add_local(ident.entity.clone().into_key());
                        })
                        .count()
                }
            })
            .sum()
    }

    fn emit_load(
        &mut self,
        index: EntIndex,
        patch_info: Option<(&mut PkgVarPairs, FunctionKey)>,
        typ: ValueType,
        pos: Option<usize>,
    ) {
        match index {
            EntIndex::Const(i) => match self.const_val(i).clone() {
                //GosValue::Nil => self.emit_code(Opcode::PUSH_NIL),
                GosValue::Bool(b) if b => self.emit_code(Opcode::PUSH_TRUE, pos),
                GosValue::Bool(b) if !b => self.emit_code(Opcode::PUSH_FALSE, pos),
                GosValue::Int(i) if OpIndex::try_from(i).ok().is_some() => {
                    let imm: OpIndex = OpIndex::try_from(i).unwrap();
                    self.emit_inst(Opcode::PUSH_IMM, [Some(typ), None, None], Some(imm), pos);
                }
                _ => {
                    self.emit_inst(Opcode::PUSH_CONST, [Some(typ), None, None], Some(i), pos);
                }
            },
            EntIndex::LocalVar(i) => {
                self.emit_inst(Opcode::LOAD_LOCAL, [Some(typ), None, None], Some(i), pos);
            }
            EntIndex::UpValue(i) => {
                self.emit_inst(Opcode::LOAD_UPVALUE, [Some(typ), None, None], Some(i), pos);
            }
            EntIndex::PackageMember(pkg, ident) => {
                self.emit_inst(
                    Opcode::LOAD_PKG_FIELD,
                    [Some(typ), None, None],
                    Some(0),
                    pos,
                );
                self.emit_raw_inst(key_to_u64(pkg), pos);
                let (pairs, func) = patch_info.unwrap();
                pairs.add_pair(pkg, ident, func, self.code().len() - 2, false);
            }
            EntIndex::BuiltInVal(op) => self.emit_code(op, pos),
            EntIndex::BuiltInType(m) => {
                let i = self.add_const(None, GosValue::Metadata(m));
                self.emit_load(i, None, ValueType::Metadata, pos);
            }
            EntIndex::Blank => unreachable!(),
        }
    }

    fn emit_store(
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
                self.push_inst_pos(
                    Instruction::new(Opcode::TO_UINT32, Some(t), None, None, None),
                    pos,
                );
            }
            Instruction::code2index(code)
        });
        inst.set_imm824(imm0, int32);
        self.push_inst_pos(inst, pos);
        if let Some((pkg, ident)) = pkg_info {
            self.emit_raw_inst(key_to_u64(pkg), pos);
            let (pairs, func) = patch_info.unwrap();
            pairs.add_pair(pkg, ident, func, self.code().len() - 2, true);
        }
    }

    fn emit_cast_to_interface(
        &mut self,
        typ: ValueType,
        rhs: OpIndex,
        m_index: OpIndex,
        pos: Option<usize>,
    ) {
        let mut inst = Instruction::new(Opcode::CAST_TO_INTERFACE, Some(typ), None, None, None);
        inst.set_imm824(rhs, m_index);
        self.push_inst_pos(inst, pos);
    }

    fn emit_import(&mut self, index: OpIndex, pkg: PackageKey, pos: Option<usize>) {
        self.emit_inst(Opcode::IMPORT, [None, None, None], Some(index), pos);
        let cd = vec![
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
        ];
        let offset = cd.len() as OpIndex;
        self.emit_inst(Opcode::JUMP_IF_NOT, [None, None, None], Some(offset), pos);
        for i in cd.into_iter() {
            self.push_inst_pos(i, pos);
        }
    }

    fn emit_pop(&mut self, count: OpIndex, pos: Option<usize>) {
        self.emit_inst(Opcode::POP, [None, None, None], Some(count), pos);
    }

    fn emit_load_struct_field(&mut self, imm: OpIndex, typ: ValueType, pos: Option<usize>) {
        self.emit_inst(
            Opcode::LOAD_STRUCT_FIELD,
            [Some(typ), None, None],
            Some(imm),
            pos,
        );
    }

    fn emit_load_index(
        &mut self,
        typ: ValueType,
        index_type: ValueType,
        comma_ok: bool,
        pos: Option<usize>,
    ) {
        let mut inst =
            Instruction::new(Opcode::LOAD_INDEX, Some(typ), Some(index_type), None, None);
        inst.set_t2_with_index(if comma_ok { 1 } else { 0 });
        self.push_inst_pos(inst, pos);
    }

    fn emit_load_index_imm(
        &mut self,
        imm: OpIndex,
        typ: ValueType,
        comma_ok: bool,
        pos: Option<usize>,
    ) {
        let mut inst = Instruction::new(Opcode::LOAD_INDEX_IMM, Some(typ), None, None, Some(imm));
        inst.set_t2_with_index(if comma_ok { 1 } else { 0 });
        self.push_inst_pos(inst, pos);
    }

    fn emit_return(&mut self, pos: Option<usize>) {
        self.emit_inst(Opcode::RETURN, [None, None, None], None, pos);
    }

    fn emit_return_init_pkg(&mut self, index: OpIndex, pos: Option<usize>) {
        self.emit_inst(
            Opcode::RETURN_INIT_PKG,
            [None, None, None],
            Some(index),
            pos,
        );
    }

    fn emit_pre_call(&mut self, pos: Option<usize>) {
        self.emit_inst(Opcode::PRE_CALL, [None, None, None], None, pos);
    }

    fn emit_call(&mut self, has_ellipsis: bool, pos: Option<usize>) {
        let op = if has_ellipsis {
            Opcode::CALL_ELLIPSIS
        } else {
            Opcode::CALL
        };
        self.emit_inst(op, [None, None, None], None, pos);
    }

    fn emit_new(&mut self, typ: ValueType, pos: Option<usize>) {
        self.emit_inst(Opcode::NEW, [Some(typ), None, None], None, pos);
    }
}
