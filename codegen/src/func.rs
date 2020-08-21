#![allow(dead_code)]

use std::convert::TryFrom;

use goscript_vm::ds::{EntIndex, FunctionVal};
use goscript_vm::opcode::*;
use goscript_vm::value::*;

use goscript_parser::ast::*;
use goscript_parser::objects::Objects as AstObjects;

/// LeftHandSide represents the left hand side of an assign stmt
/// Primitive stores index of lhs variable
/// IndexSelExpr stores the index of lhs on the stack
/// Deref stores the index of lhs on the stack
#[derive(Clone, Debug)]
pub enum LeftHandSide {
    Primitive(EntIndex),
    IndexSelExpr(OpIndex),
    Deref(OpIndex),
}

pub trait FuncGen {
    fn add_params<'e>(&mut self, fl: &FieldList, o: &AstObjects) -> Result<usize, ()>;

    fn emit_load(&mut self, index: EntIndex);

    fn emit_store(&mut self, lhs: &LeftHandSide, rhs_index: OpIndex, op: Option<OpIndex>);

    fn emit_import(&mut self, index: OpIndex);

    fn emit_pop(&mut self);

    fn emit_load_field(&mut self);

    fn emit_load_field_imm(&mut self, imm: OpIndex);

    fn emit_return(&mut self);

    fn emit_return_init_pkg(&mut self, index: OpIndex);

    fn emit_pre_call(&mut self);

    fn emit_call(&mut self, has_ellipsis: bool);

    fn emit_new(&mut self);

    fn emit_range(&mut self);
}

impl FuncGen for FunctionVal {
    fn add_params<'e>(&mut self, fl: &FieldList, o: &AstObjects) -> Result<usize, ()> {
        let re = fl
            .list
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
            .sum();
        Ok(re)
    }

    fn emit_load(&mut self, index: EntIndex) {
        match index {
            EntIndex::Const(i) => {
                // optimizaiton, replace PUSH_CONST with PUSH_NIL/_TRUE/_FALSE/_IMM]
                match self.const_val(i).clone() {
                    GosValue::Nil => self.emit_code(Opcode::PUSH_NIL),
                    GosValue::Bool(b) if b => self.emit_code(Opcode::PUSH_TRUE),
                    GosValue::Bool(b) if !b => self.emit_code(Opcode::PUSH_FALSE),
                    GosValue::Int(i) if i16::try_from(i).ok().is_some() => {
                        self.emit_code(Opcode::PUSH_IMM);
                        self.emit_data(i16::try_from(i).unwrap());
                    }
                    _ => {
                        self.emit_code(Opcode::PUSH_CONST);
                        self.emit_data(i);
                    }
                }
            }
            EntIndex::LocalVar(i) => {
                let code = Opcode::get_load_local(i);
                self.emit_code(code);
                if let Opcode::LOAD_LOCAL = code {
                    self.emit_data(i);
                }
            }
            EntIndex::UpValue(i) => {
                self.emit_code(Opcode::LOAD_UPVALUE);
                self.emit_data(i);
            }
            EntIndex::PackageMember(i) => {
                self.emit_code(Opcode::LOAD_THIS_PKG_FIELD);
                self.emit_data(i);
            }
            EntIndex::BuiltIn(op) => self.emit_code(op),
            EntIndex::Blank => unreachable!(),
        }
    }

    fn emit_store(&mut self, lhs: &LeftHandSide, rhs_index: OpIndex, op: Option<OpIndex>) {
        if let LeftHandSide::Primitive(index) = lhs {
            if EntIndex::Blank == *index {
                return;
            }
        }

        let (code, i) = match lhs {
            LeftHandSide::Primitive(index) => match index {
                EntIndex::Const(_) => unreachable!(),
                EntIndex::LocalVar(i) => (
                    if rhs_index == -1 {
                        match op {
                            Some(_) => Opcode::STORE_LOCAL_OP,
                            None => Opcode::STORE_LOCAL,
                        }
                    } else {
                        Opcode::STORE_LOCAL_NT
                    },
                    i,
                ),
                EntIndex::UpValue(i) => (
                    if rhs_index == -1 {
                        match op {
                            Some(_) => Opcode::STORE_UPVALUE_OP,
                            None => Opcode::STORE_UPVALUE,
                        }
                    } else {
                        Opcode::STORE_UPVALUE_NT
                    },
                    i,
                ),
                EntIndex::PackageMember(_) => unimplemented!(),
                EntIndex::BuiltIn(_) => unreachable!(),
                EntIndex::Blank => unreachable!(),
            },
            LeftHandSide::IndexSelExpr(i) => {
                let code = if rhs_index == -1 {
                    match op {
                        Some(_) => Opcode::STORE_FIELD_OP,
                        None => Opcode::STORE_FIELD,
                    }
                } else {
                    Opcode::STORE_FIELD_NT
                };
                (code, i)
            }
            LeftHandSide::Deref(i) => {
                let code = if rhs_index == -1 {
                    match op {
                        Some(_) => Opcode::STORE_DEREF_OP,
                        None => Opcode::STORE_DEREF,
                    }
                } else {
                    Opcode::STORE_DEREF_NT
                };
                (code, i)
            }
        };
        self.emit_code(code);
        self.emit_data(*i);
        if rhs_index < -1 {
            self.emit_data(rhs_index);
        }
        if let Some(data) = op {
            self.emit_data(data);
        }
    }

    fn emit_import(&mut self, index: OpIndex) {
        self.emit_code(Opcode::IMPORT);
        self.emit_data(index);
        self.emit_code(Opcode::JUMP_IF_NOT);
        let mut cd = vec![
            CodeData::Code(Opcode::PUSH_IMM),
            CodeData::Data(0 as OpIndex),
            CodeData::Code(Opcode::LOAD_FIELD),
            CodeData::Code(Opcode::PRE_CALL),
            CodeData::Code(Opcode::CALL),
        ];
        self.emit_data(cd.len() as OpIndex);
        self.code.append(&mut cd);
    }

    fn emit_pop(&mut self) {
        self.emit_code(Opcode::POP);
    }

    fn emit_load_field(&mut self) {
        self.emit_code(Opcode::LOAD_FIELD);
    }

    fn emit_load_field_imm(&mut self, imm: OpIndex) {
        self.emit_code(Opcode::LOAD_FIELD_IMM);
        self.emit_data(imm);
    }

    fn emit_return(&mut self) {
        self.emit_code(Opcode::RETURN);
    }

    fn emit_return_init_pkg(&mut self, index: OpIndex) {
        self.emit_code(Opcode::RETURN_INIT_PKG);
        self.emit_data(index);
    }

    fn emit_pre_call(&mut self) {
        self.emit_code(Opcode::PRE_CALL);
    }

    fn emit_call(&mut self, has_ellipsis: bool) {
        if has_ellipsis {
            self.emit_code(Opcode::CALL_ELLIPSIS);
        } else {
            self.emit_code(Opcode::CALL);
        }
    }

    fn emit_new(&mut self) {
        self.emit_code(Opcode::NEW);
    }

    fn emit_range(&mut self) {
        self.emit_code(Opcode::RANGE);
    }
}
