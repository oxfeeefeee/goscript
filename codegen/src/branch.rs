// Copyright 2022 The Goscript Authors. All rights reserved.
// Use of this source code is governed by a BSD-style
// license that can be found in the LICENSE file.

/// Helper for patching branch points
///
///
use goscript_parser::ast::*;
use goscript_parser::token::Token;
use goscript_vm::instruction::*;
use goscript_vm::value::*;
use slotmap::KeyData;
use std::collections::HashMap;

/// branch points of break and continue
pub struct BranchBlock {
    points: Vec<(usize, Token, Option<KeyData>)>,
    label: Option<KeyData>,
    is_loop: bool,
}

impl BranchBlock {
    pub fn new(label: Option<KeyData>, is_loop: bool) -> BranchBlock {
        BranchBlock {
            points: vec![],
            label: label,
            is_loop: is_loop,
        }
    }
}

/// helper for break, continue and goto
pub struct BranchHelper {
    block_stack: Vec<BranchBlock>,
    next_block_label: Option<KeyData>,
    labels: HashMap<KeyData, usize>,
    go_tos: HashMap<(FunctionKey, usize), KeyData>,
}

impl BranchHelper {
    pub fn new() -> BranchHelper {
        BranchHelper {
            block_stack: vec![],
            next_block_label: None,
            labels: HashMap::new(),
            go_tos: HashMap::new(),
        }
    }

    pub fn add_point(
        &mut self,
        func: &mut FunctionVal,
        token: Token,
        label: Option<KeyData>,
        pos: usize,
    ) {
        let index = func.code().len();
        let inst = Instruction::with_op_index(Opcode::JUMP, 0, 0, 0);
        func.add_inst_pos(inst, Some(pos));
        self.block_stack
            .last_mut()
            .unwrap()
            .points
            .push((index, token, label));
    }

    pub fn add_label(&mut self, label: KeyData, offset: usize, is_breakable: bool) {
        self.labels.insert(label, offset);
        if is_breakable {
            self.next_block_label = Some(label);
        }
    }

    pub fn go_to(
        &mut self,
        funcs: &mut FunctionObjs,
        fkey: FunctionKey,
        label: KeyData,
        pos: usize,
    ) {
        let func = &mut funcs[fkey];
        let current_offset = func.code().len();
        let jump_to = match self.labels.get(&label) {
            Some(l_offset) => (*l_offset as OpIndex) - (current_offset as OpIndex) - 1,
            None => {
                self.go_tos.insert((fkey, current_offset), label);
                0
            }
        };
        let inst = Instruction::with_op_index(Opcode::JUMP, jump_to, 0, 0);
        func.add_inst_pos(inst, Some(pos));
    }

    pub fn patch_go_tos(&self, funcs: &mut FunctionObjs) {
        for ((fkey, patch_offset), label) in self.go_tos.iter() {
            let func = &mut funcs[*fkey];
            let l_offset = self.labels[label];
            let offset = (l_offset as OpIndex) - (*patch_offset as OpIndex) - 1;
            func.instruction_mut(*patch_offset).d = offset;
        }
    }

    pub fn enter_block(&mut self, is_loop: bool) {
        self.block_stack
            .push(BranchBlock::new(self.next_block_label.take(), is_loop))
    }

    pub fn leave_block(&mut self, func: &mut FunctionVal, begin: Option<usize>) {
        let end = func.next_code_index();
        let block = self.block_stack.pop().unwrap();
        for (index, token, label) in block.points.into_iter() {
            let label_match = label.is_none() || label == block.label;
            // for select&switch, 'break' breaks current block
            // for 'continue' breaks outer looping block
            let (break_this, target) = match token {
                Token::BREAK => (true, Some(end)),
                Token::CONTINUE => (block.is_loop, begin),
                _ => unreachable!(),
            };
            if label_match && break_this {
                let current_pc = index as OpIndex + 1;
                func.instruction_mut(index).d = target.unwrap() as OpIndex - current_pc;
            } else {
                // this break/continue tries to jump out of an outer block
                // so we add it to outer block's jump out points
                self.block_stack
                    .last_mut()
                    .unwrap()
                    .points
                    .push((index, token, label));
            }
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
            func.instruction_mut(*i).d = imm;
        }
    }

    pub fn patch_default(&mut self, func: &mut FunctionVal, loc: usize) {
        if let Some(de) = self.default {
            let imm = (loc - de) as OpIndex - 1;
            func.instruction_mut(de).d = imm;
        }
    }
}

pub struct SwitchHelper {
    // the beginnings of the cases
    pub tags: SwitchJumpPoints,
    // the ends of the cases
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

pub enum CommType<'a> {
    Send(ValueType),
    RecvNoLhs,
    Recv(&'a AssignStmt),
    RecvCommaOk(&'a AssignStmt),
    Default,
}

pub struct SelectComm<'a> {
    typ: CommType<'a>,
    pos: usize,
    begin: usize,
    end: usize,
}

impl<'a> SelectComm<'a> {
    pub fn new(typ: CommType, pos: usize) -> SelectComm {
        SelectComm {
            typ: typ,
            pos: pos,
            begin: 0,
            end: 0,
        }
    }
}

pub struct SelectHelper<'a> {
    comms: Vec<SelectComm<'a>>,
    select_offset: usize,
}

impl<'a> SelectHelper<'a> {
    pub fn new() -> SelectHelper<'a> {
        SelectHelper {
            comms: vec![],
            select_offset: 0,
        }
    }

    pub fn comm_type(&self, i: usize) -> &CommType {
        &self.comms[i].typ
    }

    pub fn add_comm(&mut self, typ: CommType<'a>, pos: usize) {
        self.comms.push(SelectComm::new(typ, pos));
    }

    pub fn set_block_begin_end(&mut self, i: usize, begin: usize, end: usize) {
        let comm = &mut self.comms[i];
        comm.begin = begin;
        comm.end = end;
    }

    pub fn emit_select(&mut self, func: &mut FunctionVal) {
        self.select_offset = func.next_code_index();
        for comm in self.comms.iter() {
            let (flag, t) = match &comm.typ {
                CommType::Send(t) => (ValueType::FlagA, Some(*t)),
                CommType::RecvNoLhs => (ValueType::FlagB, None),
                CommType::Recv(_) => (ValueType::FlagC, None),
                CommType::RecvCommaOk(_) => (ValueType::FlagD, None),
                CommType::Default => (ValueType::FlagE, None),
            };
            func.add_inst_pos(
                Instruction::with_op_t_index(Opcode::SELECT, Some(flag), t, 0, 0, 0),
                Some(comm.pos),
            );
        }
    }

    pub fn patch_select(&self, func: &mut FunctionVal) {
        let count = self.comms.len();
        let offset = self.select_offset;
        let blocks_begin = self.comms.first().unwrap().begin as OpIndex;
        let blocks_end = self.comms.last().unwrap().end as OpIndex;
        // set the block offset for each block.
        // the first block's offset is known(0) so the space is used for
        // the count of blocks.
        func.instruction_mut(offset).d = count as OpIndex;
        for i in 1..count {
            let diff = self.comms[i].begin as OpIndex - blocks_begin;
            func.instruction_mut(offset + i).d = diff;
        }

        // set where to jump when the block ends
        for i in 0..count - 1 {
            let block_end = self.comms[i].end;
            let diff = blocks_end - block_end as OpIndex;
            func.instruction_mut(block_end).d = diff;
        }
    }

    pub fn to_comm_clause(s: &Stmt) -> &CommClause {
        match s {
            Stmt::Comm(c) => c,
            _ => unreachable!(),
        }
    }

    pub fn unwrap_recv(e: &Expr) -> (&Expr, usize) {
        match e {
            Expr::Unary(ue) => {
                assert_eq!(ue.op, Token::ARROW);
                (&ue.expr, ue.op_pos)
            }
            _ => unreachable!(),
        }
    }
}
