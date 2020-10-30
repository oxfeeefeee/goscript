#![allow(dead_code)]

use std::convert::TryFrom;

use goscript_vm::instruction::*;
use goscript_vm::objects::{key_to_u64, EntIndex, FunctionVal};
use goscript_vm::value::*;

use goscript_parser::ast::*;
use goscript_parser::objects::IdentKey;
use goscript_parser::objects::Objects as AstObjects;

#[derive(Clone, Copy, Debug)]
pub enum IndexSelType {
    Indexing,
    StructField,
    PkgMember(PackageKey, IdentKey),
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

pub trait FuncGen {
    fn add_params<'e>(&mut self, fl: &FieldList, o: &AstObjects) -> usize;

    fn emit_load(&mut self, index: EntIndex, pkg: Option<PackageKey>, typ: ValueType);

    fn emit_store(
        &mut self,
        lhs: &LeftHandSide,
        rhs_index: OpIndex,
        op: Option<Opcode>,
        pkg: Option<PackageKey>,
        typ: ValueType,
    );

    fn emit_import(&mut self, index: OpIndex, pkg: PackageKey);

    fn emit_pop(&mut self, count: OpIndex);

    fn emit_load_field(&mut self, typ: ValueType, sel_type: ValueType);

    fn emit_load_struct_field(&mut self, imm: OpIndex, typ: ValueType);

    fn emit_load_index(&mut self, typ: ValueType, sel_type: ValueType);

    fn emit_load_index_imm(&mut self, imm: OpIndex, typ: ValueType);

    fn emit_cast_to_interface(&mut self, typ: ValueType, rhs: OpIndex, m_index: OpIndex);

    fn emit_return(&mut self);

    fn emit_return_init_pkg(&mut self, index: OpIndex);

    fn emit_pre_call(&mut self);

    fn emit_call(&mut self, has_ellipsis: bool);

    fn emit_new(&mut self, typ: ValueType);

    fn emit_range(&mut self);
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

    fn emit_load(&mut self, index: EntIndex, pkg: Option<PackageKey>, typ: ValueType) {
        match index {
            EntIndex::Const(i) => match self.const_val(i).clone() {
                //GosValue::Nil => self.emit_code(Opcode::PUSH_NIL),
                GosValue::Bool(b) if b => self.emit_code(Opcode::PUSH_TRUE),
                GosValue::Bool(b) if !b => self.emit_code(Opcode::PUSH_FALSE),
                GosValue::Int(i) if OpIndex::try_from(i).ok().is_some() => {
                    let imm: OpIndex = OpIndex::try_from(i).unwrap();
                    self.emit_inst(Opcode::PUSH_IMM, Some(typ), None, None, Some(imm));
                }
                _ => {
                    self.emit_inst(Opcode::PUSH_CONST, Some(typ), None, None, Some(i));
                }
            },
            EntIndex::LocalVar(i) => {
                self.emit_inst(Opcode::LOAD_LOCAL, Some(typ), None, None, Some(i));
            }
            EntIndex::UpValue(i) => {
                self.emit_inst(Opcode::LOAD_UPVALUE, Some(typ), None, None, Some(i));
            }
            EntIndex::PackageMember(i) => {
                let pkg = pkg.unwrap_or(self.package);
                self.emit_inst(Opcode::LOAD_PKG_FIELD, Some(typ), None, None, Some(i));
                self.emit_raw_inst(key_to_u64(pkg));
            }
            EntIndex::BuiltInVal(op) => self.emit_code(op),
            EntIndex::BuiltInType(m) => {
                let i = self.add_const(None, GosValue::Metadata(m));
                self.emit_load(i, None, ValueType::Metadata);
            }
            EntIndex::Blank => unreachable!(),
        }
    }

    fn emit_store(
        &mut self,
        lhs: &LeftHandSide,
        rhs_index: OpIndex,
        op: Option<Opcode>,
        pkg: Option<PackageKey>,
        typ: ValueType,
    ) {
        if let LeftHandSide::Primitive(index) = lhs {
            if EntIndex::Blank == *index {
                return;
            }
        }

        let mut pkg_key = None;
        let (code, int32, t1, t2, int8) = match lhs {
            LeftHandSide::Primitive(index) => match index {
                EntIndex::Const(_) => unreachable!(),
                EntIndex::LocalVar(i) => (Opcode::STORE_LOCAL, *i, None, None, None),
                EntIndex::UpValue(i) => (Opcode::STORE_UPVALUE, *i, None, None, None),
                EntIndex::PackageMember(i) => {
                    pkg_key = Some(pkg.unwrap_or(self.package));
                    (
                        Opcode::STORE_PKG_FIELD,
                        *i,
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
                IndexSelType::PkgMember(pkg, _) => {
                    pkg_key = Some(pkg);
                    (
                        Opcode::STORE_PKG_FIELD,
                        info.imm_index.unwrap(),
                        None,
                        None,
                        None,
                    )
                }
            },
            LeftHandSide::Deref(i) => (Opcode::STORE_DEREF, *i, None, None, None),
        };
        let mut inst = Instruction::new(code, Some(typ), t1, t2, None);
        if let Some(i) = int8 {
            inst.set_t2_with_index(i);
        }
        assert!(rhs_index == -1 || op.is_none());
        let imm0 = op.map_or(rhs_index, |x| Instruction::code2index(x));
        inst.set_imm824(imm0, int32);
        self.code.push(inst);
        if let Some(key) = pkg_key {
            self.emit_raw_inst(key_to_u64(key));
        }
    }

    fn emit_cast_to_interface(&mut self, typ: ValueType, rhs: OpIndex, m_index: OpIndex) {
        let mut inst = Instruction::new(Opcode::CAST_TO_INTERFACE, Some(typ), None, None, None);
        inst.set_imm824(rhs, m_index);
        self.code.push(inst);
    }

    fn emit_import(&mut self, index: OpIndex, pkg: PackageKey) {
        self.emit_inst(Opcode::IMPORT, None, None, None, Some(index));
        let mut cd = vec![
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
        self.emit_inst(Opcode::JUMP_IF_NOT, None, None, None, Some(offset));
        self.code.append(&mut cd);
    }

    fn emit_pop(&mut self, count: OpIndex) {
        self.emit_inst(Opcode::POP, None, None, None, Some(count));
    }

    fn emit_load_field(&mut self, typ: ValueType, sel_type: ValueType) {
        self.emit_inst(Opcode::LOAD_FIELD, Some(typ), Some(sel_type), None, None);
    }

    fn emit_load_struct_field(&mut self, imm: OpIndex, typ: ValueType) {
        self.emit_inst(Opcode::LOAD_STRUCT_FIELD, Some(typ), None, None, Some(imm));
    }

    fn emit_load_index(&mut self, typ: ValueType, index_type: ValueType) {
        self.emit_inst(Opcode::LOAD_INDEX, Some(typ), Some(index_type), None, None);
    }

    fn emit_load_index_imm(&mut self, imm: OpIndex, typ: ValueType) {
        self.emit_inst(Opcode::LOAD_INDEX_IMM, Some(typ), None, None, Some(imm));
    }

    fn emit_return(&mut self) {
        self.emit_inst(Opcode::RETURN, None, None, None, None);
    }

    fn emit_return_init_pkg(&mut self, index: OpIndex) {
        self.emit_inst(Opcode::RETURN_INIT_PKG, None, None, None, Some(index));
    }

    fn emit_pre_call(&mut self) {
        self.emit_inst(Opcode::PRE_CALL, None, None, None, None);
    }

    fn emit_call(&mut self, has_ellipsis: bool) {
        let op = if has_ellipsis {
            Opcode::CALL_ELLIPSIS
        } else {
            Opcode::CALL
        };
        self.emit_inst(op, None, None, None, None);
    }

    fn emit_new(&mut self, typ: ValueType) {
        self.emit_inst(Opcode::NEW, Some(typ), None, None, None);
    }

    fn emit_range(&mut self) {
        self.emit_inst(Opcode::RANGE, None, None, None, None);
    }
}
