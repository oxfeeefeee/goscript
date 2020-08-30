#![allow(dead_code)]

use std::convert::TryFrom;

use goscript_vm::objects::{EntIndex, FunctionVal};
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

    fn emit_store(&mut self, lhs: &LeftHandSide, rhs_index: OpIndex, op: Option<Opcode>);

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
                    GosValue::Int(i) if OpIndex::try_from(i).ok().is_some() => {
                        let imm: OpIndex = OpIndex::try_from(i).unwrap();
                        self.emit_inst(Opcode::PUSH_IMM, None, None, None, Some(imm));
                    }
                    _ => {
                        self.emit_inst(Opcode::PUSH_CONST, None, None, None, Some(i));
                    }
                }
            }
            EntIndex::LocalVar(i) => self.emit_inst(Opcode::LOAD_LOCAL, None, None, None, Some(i)),
            EntIndex::UpValue(i) => self.emit_inst(Opcode::LOAD_UPVALUE, None, None, None, Some(i)),
            EntIndex::PackageMember(i) => {
                self.emit_inst(Opcode::LOAD_THIS_PKG_FIELD, None, None, None, Some(i))
            }
            EntIndex::BuiltIn(op) => self.emit_code(op),
            EntIndex::Blank => unreachable!(),
        }
    }

    fn emit_store(&mut self, lhs: &LeftHandSide, rhs_index: OpIndex, op: Option<Opcode>) {
        if let LeftHandSide::Primitive(index) = lhs {
            if EntIndex::Blank == *index {
                return;
            }
        }

        let (code, i) = match lhs {
            LeftHandSide::Primitive(index) => match index {
                EntIndex::Const(_) => unreachable!(),
                EntIndex::LocalVar(i) => (Opcode::STORE_LOCAL, i),
                EntIndex::UpValue(i) => (Opcode::STORE_UPVALUE, i),
                EntIndex::PackageMember(i) => (Opcode::STORE_THIS_PKG_FIELD, i),
                EntIndex::BuiltIn(_) => unreachable!(),
                EntIndex::Blank => unreachable!(),
            },
            LeftHandSide::IndexSelExpr(i) => (Opcode::STORE_FIELD, i),
            LeftHandSide::Deref(i) => (Opcode::STORE_DEREF, i),
        };

        let mut inst = Instruction::new(code, op, None, None, None);
        inst.set_imm2(rhs_index, *i);
        self.code.push(CodeData::Inst(inst));
    }

    fn emit_import(&mut self, index: OpIndex) {
        self.emit_inst(Opcode::IMPORT, None, None, None, Some(index));
        let mut cd = vec![
            CodeData::Inst(Instruction::new(
                Opcode::PUSH_IMM,
                None,
                None,
                None,
                Some(0),
            )),
            CodeData::Code(Opcode::LOAD_FIELD),
            CodeData::Code(Opcode::PRE_CALL),
            CodeData::Code(Opcode::CALL),
        ];
        let offset = cd.len() as OpIndex;
        self.emit_inst(Opcode::JUMP_IF_NOT, None, None, None, Some(offset));
        self.code.append(&mut cd);
    }

    fn emit_pop(&mut self) {
        self.emit_inst(Opcode::POP, None, None, None, None);
    }

    fn emit_load_field(&mut self) {
        self.emit_inst(Opcode::LOAD_FIELD, None, None, None, None);
    }

    fn emit_load_field_imm(&mut self, imm: OpIndex) {
        self.emit_inst(Opcode::LOAD_FIELD_IMM, None, None, None, Some(imm));
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

    fn emit_new(&mut self) {
        self.emit_inst(Opcode::NEW, None, None, None, None);
    }

    fn emit_range(&mut self) {
        self.emit_inst(Opcode::RANGE, None, None, None, None);
    }
}
