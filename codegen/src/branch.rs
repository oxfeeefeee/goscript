#![allow(dead_code)]
use goscript_parser::ast::*;
use goscript_parser::token::Token;
use goscript_vm::instruction::*;
use goscript_vm::value::*;

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

    pub fn add_point(&mut self, func: &mut FunctionVal, token: Token, pos: usize) {
        let index = func.code().len();
        func.emit_code_with_imm(Opcode::JUMP, 0, Some(pos));
        self.points_vec
            .last_mut()
            .unwrap()
            .data
            .push((index, token));
    }

    pub fn enter_block(&mut self) {
        self.points_vec.push(BranchPoints::new())
    }

    pub fn leave_block(&mut self, func: &mut FunctionVal, begin: Option<usize>, end: usize) {
        let points = self.points_vec.pop().unwrap();
        for (index, token) in points.data.iter() {
            let current_pc = *index as OpIndex + 1;
            let target = if *token == Token::BREAK {
                end
            } else {
                begin.unwrap()
            };
            func.instruction_mut(*index)
                .set_imm(target as OpIndex - current_pc);
        }
    }
}

pub struct SwitchJumpPoints {
    cases: Vec<Vec<usize>>,
    default: Option<usize>,
}

impl SwitchJumpPoints {
    fn new() -> SwitchJumpPoints {
        SwitchJumpPoints {
            cases: vec![],
            default: None,
        }
    }

    fn add_case_clause(&mut self) {
        self.cases.push(vec![]);
    }

    pub fn add_case(&mut self, case: usize, index: usize) {
        self.cases[case].push(index);
    }

    pub fn add_default(&mut self, index: usize) {
        self.default = Some(index)
    }

    pub fn patch_case(&mut self, func: &mut FunctionVal, case: usize, loc: usize) {
        for i in self.cases[case].iter() {
            let imm = (loc - i) as OpIndex - 1;
            func.instruction_mut(*i).set_imm(imm);
        }
    }

    pub fn patch_default(&mut self, func: &mut FunctionVal, loc: usize) {
        if let Some(de) = self.default {
            let imm = (loc - de) as OpIndex - 1;
            func.instruction_mut(de).set_imm(imm);
        }
    }
}

pub struct SwitchHelper {
    pub tags: SwitchJumpPoints,
    pub ends: SwitchJumpPoints,
}

impl SwitchHelper {
    pub fn new() -> SwitchHelper {
        SwitchHelper {
            tags: SwitchJumpPoints::new(),
            ends: SwitchJumpPoints::new(),
        }
    }

    pub fn add_case_clause(&mut self) {
        self.tags.add_case_clause();
        self.ends.add_case_clause();
    }

    pub fn patch_ends(&mut self, func: &mut FunctionVal, loc: usize) {
        for i in 0..self.ends.cases.len() {
            self.ends.patch_case(func, i, loc);
        }
        self.ends.patch_default(func, loc);
    }

    pub fn to_case_clause(s: &Stmt) -> &CaseClause {
        match s {
            Stmt::Case(c) => c,
            _ => unreachable!(),
        }
    }

    pub fn has_fall_through(s: &Stmt) -> bool {
        let case = Self::to_case_clause(s);
        case.body.last().map_or(false, |x| match x {
            Stmt::Branch(b) => b.token == Token::FALLTHROUGH,
            _ => false,
        })
    }
}
