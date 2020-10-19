#![allow(dead_code)]
use super::func::FuncGen;
use goscript_parser::ast::*;
use goscript_parser::objects::Objects as AstObjects;
use goscript_parser::objects::*;
use goscript_parser::token::Token;
use goscript_types::{PackageKey as TCPackageKey, TCObjects, TypeInfo};
use goscript_vm::instruction::*;
use goscript_vm::value::*;
use std::collections::HashMap;
use std::rc::Rc;

/// branch points of break and continue
pub struct BranchPoints {
    data: Vec<(usize, Token)>,
}

impl BranchPoints {
    pub fn new() -> BranchPoints {
        BranchPoints { data: vec![] }
    }
}

/// helper for break & continue
pub struct BreakContinue {
    points_vec: Vec<BranchPoints>,
}

impl BreakContinue {
    pub fn new() -> BreakContinue {
        BreakContinue { points_vec: vec![] }
    }

    pub fn add_point(&mut self, func: &mut FunctionVal, token: Token) {
        let index = func.code.len();
        func.emit_code_with_imm(Opcode::JUMP, 0);
        self.points_vec
            .last_mut()
            .unwrap()
            .data
            .push((index, token));
    }

    pub fn enter_block(&mut self) {
        self.points_vec.push(BranchPoints::new())
    }

    pub fn leave_block(&mut self, func: &mut FunctionVal, begin: usize, end: usize) {
        let points = self.points_vec.pop().unwrap();
        for (index, token) in points.data.iter() {
            let offset = if *token == Token::BREAK {
                (end - index) as OpIndex - 1
            } else {
                if begin >= *index {
                    (begin - index) as OpIndex - 1
                } else {
                    (index - begin) as OpIndex - 1
                }
            };
            func.code[*index as usize].set_imm(offset);
        }
    }
}

pub struct SwitchHelper {
    cases: Vec<Vec<usize>>,
    default: usize,
}

impl SwitchHelper {
    pub fn new() -> SwitchHelper {
        SwitchHelper {
            cases: vec![],
            default: 0,
        }
    }

    pub fn add_case_clause(&mut self) {
        self.cases.push(vec![]);
    }

    pub fn add_case(&mut self, index: usize) {
        self.cases.last_mut().unwrap().push(index);
    }

    pub fn add_default(&mut self, index: usize) {
        self.default = index
    }

    pub fn patch_case_clause(&mut self, func: &mut FunctionVal, case: usize, loc: usize) {
        for i in self.cases[case].iter() {
            let imm = (loc - i) as OpIndex - 1;
            func.code[*i].set_imm(imm);
        }
    }

    pub fn patch_default(&mut self, func: &mut FunctionVal, loc: usize) {
        let imm = (loc - self.default) as OpIndex - 1;
        func.code[self.default].set_imm(imm);
    }
}
