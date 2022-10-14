// Copyright 2022 The Goscript Authors. All rights reserved.
// Use of this source code is governed by a BSD-style
// license that can be found in the LICENSE file.

/// Helper for patching branch points
///
///
use crate::context::*;
use goscript_parser::ast::*;
use goscript_parser::objects::AssignStmtKey;
use goscript_parser::{Map, Token};
use goscript_types::ObjKey as TCObjKey;
use goscript_vm::value::*;

/// branch points of break and continue
pub(crate) struct BranchBlock {
    points: Vec<(usize, Token, Option<TCObjKey>)>,
    label: Option<TCObjKey>,
    is_loop: bool,
}

impl BranchBlock {
    pub fn new(label: Option<TCObjKey>, is_loop: bool) -> BranchBlock {
        BranchBlock {
            points: vec![],
            label,
            is_loop,
        }
    }
}

/// helper for break, continue and goto
pub(crate) struct BranchHelper {
    block_stack: Vec<BranchBlock>,
    next_block_label: Option<TCObjKey>,
    labels: Map<TCObjKey, usize>,
}

impl BranchHelper {
    pub fn new() -> BranchHelper {
        BranchHelper {
            block_stack: vec![],
            next_block_label: None,
            labels: Map::new(),
        }
    }

    pub fn labels(&self) -> &Map<TCObjKey, usize> {
        &self.labels
    }

    pub fn add_label(&mut self, label: TCObjKey, offset: usize, is_breakable: bool) {
        self.labels.insert(label, offset);
        if is_breakable {
            self.next_block_label = Some(label);
        }
    }

    pub fn add_jump_point(
        &mut self,
        fctx: &mut FuncCtx,
        token: Token,
        label: Option<TCObjKey>,
        pos: usize,
    ) {
        let index = fctx.next_code_index();
        fctx.emit_jump(0, Some(pos));
        self.block_stack
            .last_mut()
            .unwrap()
            .points
            .push((index, token, label));
    }

    pub fn enter_block(&mut self, is_loop: bool) {
        self.block_stack
            .push(BranchBlock::new(self.next_block_label.take(), is_loop))
    }

    pub fn leave_block(&mut self, fctx: &mut FuncCtx, begin: Option<usize>) {
        let end = fctx.next_code_index();
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
                fctx.inst_mut(index).d = Addr::Imm(target.unwrap() as OpIndex - current_pc);
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

pub(crate) struct SwitchJumpPoints {
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

    pub fn patch_case(&mut self, func: &mut FuncCtx, case: usize, loc: usize) {
        for i in self.cases[case].iter() {
            func.inst_mut(*i).d = Addr::Imm((loc - i) as OpIndex - 1);
        }
    }

    pub fn patch_default(&mut self, func: &mut FuncCtx, loc: usize) {
        if let Some(de) = self.default {
            func.inst_mut(de).d = Addr::Imm((loc - de) as OpIndex - 1);
        }
    }
}

pub(crate) struct SwitchHelper {
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

    pub fn patch_ends(&mut self, func: &mut FuncCtx, loc: usize) {
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

pub(crate) enum CommType {
    Send(Addr),
    RecvNoLhs,
    Recv(AssignStmtKey, Addr, bool),
    Default,
}

impl CommType {
    pub fn runtime_flag(&self) -> ValueType {
        match self {
            Self::Send(_) => ValueType::FlagA,
            Self::RecvNoLhs => ValueType::FlagB,
            Self::Recv(_, _, ok) => {
                if !ok {
                    ValueType::FlagC
                } else {
                    ValueType::FlagD
                }
            }
            Self::Default => ValueType::FlagE,
        }
    }
}

pub(crate) struct SelectComm {
    typ: CommType,
    chan_addr: Option<Addr>,
    pos: usize,
    begin: usize,
    end: usize,
    offset: usize,
}

impl SelectComm {
    pub fn new(typ: CommType, chan_addr: Option<Addr>, pos: usize) -> SelectComm {
        SelectComm {
            typ,
            chan_addr,
            pos,
            begin: 0,
            end: 0,
            offset: 0,
        }
    }
}

pub(crate) struct SelectHelper {
    comms: Vec<SelectComm>,
}

impl SelectHelper {
    pub fn new() -> SelectHelper {
        SelectHelper { comms: vec![] }
    }

    pub fn comm_type(&self, i: usize) -> &CommType {
        &self.comms[i].typ
    }

    pub fn add_comm(&mut self, typ: CommType, chan_addr: Option<Addr>, pos: usize) {
        self.comms.push(SelectComm::new(typ, chan_addr, pos));
    }

    pub fn set_block_begin_end(&mut self, i: usize, begin: usize, end: usize) {
        let comm = &mut self.comms[i];
        comm.begin = begin;
        comm.end = end;
    }

    pub fn emit_select(&mut self, fctx: &mut FuncCtx, pos: Option<usize>) {
        let default_flag = self.comms.last().unwrap().typ.runtime_flag();
        let count = self.comms.len()
            - if default_flag == ValueType::FlagE {
                1
            } else {
                0
            };
        let select_offset = fctx.next_code_index();
        fctx.emit_inst(
            InterInst::with_op_t_index(
                Opcode::SELECT,
                Some(default_flag),
                None,
                Addr::Void,
                Addr::Imm(count as OpIndex),
                Addr::Void,
            ),
            pos,
        );

        for comm in self.comms.iter_mut() {
            comm.offset = fctx.next_code_index();
            let flag = comm.typ.runtime_flag();
            match &comm.typ {
                CommType::Send(val) | CommType::Recv(_, val, _) => fctx.emit_inst(
                    InterInst::with_op_t_index(
                        Opcode::VOID,
                        Some(flag),
                        None,
                        Addr::Void,
                        comm.chan_addr.unwrap(),
                        *val,
                    ),
                    Some(comm.pos),
                ),
                CommType::RecvNoLhs => fctx.emit_inst(
                    InterInst::with_op_t_index(
                        Opcode::VOID,
                        Some(flag),
                        None,
                        Addr::Void,
                        comm.chan_addr.unwrap(),
                        Addr::Void,
                    ),
                    Some(comm.pos),
                ),
                CommType::Default => comm.offset = select_offset,
            };
        }
    }

    pub fn patch_select(&self, fctx: &mut FuncCtx) {
        let blocks_begin = self.comms.first().unwrap().begin as OpIndex;
        let blocks_end = self.comms.last().unwrap().end as OpIndex;
        // set the block offset for each block.
        for comm in self.comms.iter() {
            let diff = comm.begin as OpIndex - blocks_begin;
            fctx.inst_mut(comm.offset).d = Addr::Imm(diff);
        }
        // set where to jump when the block ends
        for comm in self.comms[..self.comms.len() - 1].iter() {
            let block_end = comm.end;
            let diff = blocks_end - block_end as OpIndex;
            fctx.inst_mut(block_end).d = Addr::Imm(diff);
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
