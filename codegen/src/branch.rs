/// Helper for patching branch points
///
///
use goscript_parser::ast::*;
use goscript_parser::objects::*;
use goscript_parser::token::Token;
use goscript_vm::instruction::*;
use goscript_vm::value::*;
use std::collections::HashMap;

/// branch points of break and continue
pub struct BranchBlock {
    points: Vec<(usize, Token, Option<EntityKey>)>,
    label: Option<EntityKey>,
}

impl BranchBlock {
    pub fn new(label: Option<EntityKey>) -> BranchBlock {
        BranchBlock {
            points: vec![],
            label: label,
        }
    }
}

/// helper for break, continue and goto
pub struct BranchHelper {
    block_stack: Vec<BranchBlock>,
    next_block_label: Option<EntityKey>,
    labels: HashMap<EntityKey, usize>,
}

impl BranchHelper {
    pub fn new() -> BranchHelper {
        BranchHelper {
            block_stack: vec![],
            next_block_label: None,
            labels: HashMap::new(),
        }
    }

    pub fn add_point(
        &mut self,
        func: &mut FunctionVal,
        token: Token,
        label: Option<EntityKey>,
        pos: usize,
    ) {
        let index = func.code().len();
        func.emit_code_with_imm(Opcode::JUMP, 0, Some(pos));
        self.block_stack
            .last_mut()
            .unwrap()
            .points
            .push((index, token, label));
    }

    pub fn add_label(&mut self, label: EntityKey, offset: usize, is_breakable: bool) {
        self.labels.insert(label, offset);
        if is_breakable {
            self.next_block_label = Some(label);
        }
    }

    pub fn go_to(&self, func: &mut FunctionVal, label: &EntityKey, pos: usize) {
        let current_offset = func.code().len();
        let l_offset = self.labels.get(label).unwrap();
        let offset = (*l_offset as OpIndex) - (current_offset as OpIndex) - 1;
        func.emit_code_with_imm(Opcode::JUMP, offset, Some(pos));
    }

    pub fn enter_block(&mut self) {
        self.block_stack
            .push(BranchBlock::new(self.next_block_label.take()))
    }

    pub fn leave_block(&mut self, func: &mut FunctionVal, begin: Option<usize>) {
        let end = func.next_code_index();
        let block = self.block_stack.pop().unwrap();
        for (index, token, label) in block.points.into_iter() {
            let current_pc = index as OpIndex + 1;
            let target = if token == Token::BREAK {
                end
            } else {
                begin.unwrap()
            };
            if label == block.label {
                func.instruction_mut(index)
                    .set_imm(target as OpIndex - current_pc);
            } else {
                // this break/continue tries to jump out of an outer loop
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
            func.emit_code_with_type2(Opcode::SELECT, flag, t, Some(comm.pos));
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
        func.instruction_mut(offset).set_imm(count as OpIndex);
        for i in 1..count {
            let diff = self.comms[i].begin as OpIndex - blocks_begin;
            func.instruction_mut(offset + i).set_imm(diff);
        }

        // set where to jump when the block ends
        for i in 0..count - 1 {
            let block_end = self.comms[i].end;
            let diff = blocks_end - block_end as OpIndex;
            func.instruction_mut(block_end).set_imm(diff);
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
