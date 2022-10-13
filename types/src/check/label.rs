// Copyright 2022 The Goscript Authors. All rights reserved.
// Use of this source code is governed by a BSD-style
// license that can be found in the LICENSE file.
//
//
// This code is adapted from the offical Go code written in Go
// with license as follows:
// Copyright 2013 The Go Authors. All rights reserved.
// Use of this source code is governed by a BSD-style
// license that can be found in the LICENSE file.

#![allow(dead_code)]
use crate::SourceRead;

use super::super::objects::ScopeKey;
use super::super::scope::Scope;
use super::check::Checker;
use goscript_parser::ast::Node;
use goscript_parser::ast::{BlockStmt, BranchStmt, Decl, Stmt};
use goscript_parser::objects::{LabeledStmtKey, Objects as AstObjects};
use goscript_parser::{Map, Pos, Token};
use std::cell::RefCell;
use std::rc::Rc;

#[derive(Debug)]
struct Block {
    parent: Option<Rc<RefCell<Block>>>, // enclosing block
    lstmt: Option<LabeledStmtKey>,      // labeled statement to which this block belongs
    labels: Map<String, LabeledStmtKey>,
}

impl Block {
    fn new(parent: Option<Rc<RefCell<Block>>>, stmt: Option<LabeledStmtKey>) -> Block {
        Block {
            parent: parent,
            lstmt: stmt,
            labels: Map::new(),
        }
    }

    /// insert records a new label declaration for the current block.
    /// The label must not have been declared before in any block.
    fn insert(&mut self, s: LabeledStmtKey, objs: &AstObjects) {
        let name = objs.idents[objs.l_stmts[s].label].name.clone();
        debug_assert!(self.goto_target(&name).is_none());
        self.labels.insert(name, s);
    }

    fn search(
        &self,
        name: &str,
        objs: Option<&AstObjects>,
        f: Box<dyn Fn(&Block, &str, Option<&AstObjects>) -> Option<LabeledStmtKey>>,
    ) -> Option<LabeledStmtKey> {
        if let Some(l) = f(self, name, objs) {
            return Some(l);
        }
        let mut block_op = self.parent.clone();
        loop {
            if let Some(b) = block_op {
                if let Some(l) = f(&b.borrow(), name, objs) {
                    return Some(l);
                }
                block_op = b.borrow().parent.clone();
            } else {
                return None;
            }
        }
    }

    /// goto_target returns the labeled statement in the current
    /// or an enclosing block with the given label name, or None.
    fn goto_target(&self, name: &str) -> Option<LabeledStmtKey> {
        let f = |b: &Block, n: &str, _: Option<&AstObjects>| b.labels.get(n).map(|x| *x);
        self.search(name, None, Box::new(f))
    }

    /// enclosing_target returns the innermost enclosing labeled
    /// statement with the given label name, or None.
    fn enclosing_target(&self, name: &str, objs: &AstObjects) -> Option<LabeledStmtKey> {
        let f = |b: &Block, n: &str, o: Option<&AstObjects>| {
            let objs = o.unwrap();
            if let Some(s) = b.lstmt {
                if &objs.idents[objs.l_stmts[s].label].name == n {
                    return Some(s);
                }
            }
            None
        };
        self.search(name, Some(objs), Box::new(f))
    }
}

struct StmtBranchesContext {
    var_decl_pos: Option<Pos>,
    fwd_jumps: Vec<Rc<BranchStmt>>,
    bad_jumps: Vec<Rc<BranchStmt>>,
    lstmt: Option<LabeledStmtKey>,
}

impl StmtBranchesContext {
    fn new(lstmt: Option<LabeledStmtKey>) -> StmtBranchesContext {
        StmtBranchesContext {
            var_decl_pos: None,
            fwd_jumps: vec![],
            bad_jumps: vec![],
            lstmt: lstmt,
        }
    }

    /// All forward jumps jumping over a variable declaration are possibly
    /// invalid (they may still jump out of the block and be ok).
    /// record_var_decl records them for the given position.
    fn record_var_decl(&mut self, p: Pos) {
        self.var_decl_pos = Some(p);
        self.bad_jumps = self.fwd_jumps.clone();
    }

    fn jumps_over_var_decl(&self, bs: &Rc<BranchStmt>) -> bool {
        if let Some(_) = self.var_decl_pos {
            self.bad_jumps.iter().find(|&x| Rc::ptr_eq(x, bs)).is_some()
        } else {
            false
        }
    }
}

impl<'a, S: SourceRead> Checker<'a, S> {
    pub fn labels(&mut self, body: &Rc<BlockStmt>) {
        let (pos, end) = (body.pos(), body.end());
        let comment = "label".to_owned();
        let all = self.tc_objs.new_scope(None, pos, end, comment, false);
        let fwd_jumps = self.block_branches(all, None, None, &body.list);

        // If there are any forward jumps left, no label was found for
        // the corresponding goto statements. Either those labels were
        // never defined, or they are inside blocks and not reachable
        // for the respective gotos.
        let scope = &self.tc_objs.scopes[all];
        for jump in fwd_jumps.iter() {
            let ident = self.ast_ident(jump.label.unwrap());
            let (pos, name) = (ident.pos, ident.name.clone());
            let msg = if let Some(alt) = scope.lookup(&name) {
                self.tc_objs.lobjs[*alt]
                    .entity_type_mut()
                    .label_set_used(true); // avoid another error
                format!("goto {} jumps into block", name)
            } else {
                format!("label {} not declared", name)
            };
            self.error(pos, msg);
        }

        // spec: "It is illegal to define a label that is never used."
        for (_, okey) in scope.elems().iter() {
            let lobj = self.lobj(*okey);
            if !lobj.entity_type().label_used() {
                self.soft_error(
                    lobj.pos(),
                    format!("label {} declared but not used", lobj.name()),
                );
            }
        }
    }

    /// block_branches processes a block's statement list and returns the set of outgoing forward jumps.
    /// all is the scope of all declared labels, parent the set of labels declared in the immediately
    /// enclosing block, and lstmt is the labeled statement this block is associated with.
    fn block_branches(
        &mut self,
        all: ScopeKey,
        parent: Option<Rc<RefCell<Block>>>,
        lstmt: Option<LabeledStmtKey>,
        list: &Vec<Stmt>,
    ) -> Vec<Rc<BranchStmt>> {
        let b = Rc::new(RefCell::new(Block::new(parent, lstmt)));
        let mut ctx = StmtBranchesContext::new(lstmt);

        for s in list.iter() {
            self.stmt_branches(all, b.clone(), s, &mut ctx);
        }

        ctx.fwd_jumps
    }

    fn stmt_branches(
        &mut self,
        all: ScopeKey,
        block: Rc<RefCell<Block>>,
        stmt: &Stmt,
        ctx: &mut StmtBranchesContext,
    ) {
        let mut block_branches = |lstmt: Option<LabeledStmtKey>,
                                  list: &Vec<Stmt>,
                                  ctx: &mut StmtBranchesContext| {
            // Unresolved forward jumps inside the nested block
            // become forward jumps in the current block.
            ctx.fwd_jumps
                .append(&mut self.block_branches(all, Some(block.clone()), lstmt, list));
        };

        match stmt {
            Stmt::Decl(d) => match &**d {
                Decl::Gen(gd) => {
                    if gd.token == Token::VAR {
                        ctx.record_var_decl(gd.token_pos)
                    }
                }
                _ => {}
            },
            Stmt::Labeled(lkey) => {
                let ls = &self.ast_objs.l_stmts[*lkey];
                let lable_stmt = ls.stmt.clone();
                let label = &self.ast_objs.idents[ls.label];
                let name = label.name.clone();
                if name != "_" {
                    let lb = self
                        .tc_objs
                        .new_label(label.pos, Some(self.pkg), name.to_string());
                    if let Some(alt) = Scope::insert(all, lb, self.tc_objs) {
                        // ok to continue
                        self.soft_error(label.pos, format!("label {} already declared", name));
                        self.report_alt_decl(alt);
                    } else {
                        block.borrow_mut().insert(*lkey, self.ast_objs);
                        self.result.record_def(ls.label, Some(lb));
                    }
                    // resolve matching forward jumps and remove them from fwd_jumps
                    ctx.fwd_jumps = ctx
                        .fwd_jumps
                        .iter()
                        .filter(|&x| {
                            let ikey = x.label.unwrap();
                            let ident = &self.ast_objs.idents[ikey];
                            let found = ident.name == name;
                            if found {
                                self.tc_objs.lobjs[lb]
                                    .entity_type_mut()
                                    .label_set_used(true);
                                self.result.record_use(ikey, lb);
                                if ctx.jumps_over_var_decl(x) {
                                    self.soft_error(
                                        ident.pos,
                                        format!(
                                            "goto {} jumps over variable declaration at line {}",
                                            name,
                                            self.position(ctx.var_decl_pos.unwrap()).line
                                        ),
                                    )
                                }
                            }
                            !found
                        })
                        .map(|x| x.clone())
                        .collect();

                    ctx.lstmt = Some(*lkey);
                }
                self.stmt_branches(all, block, &lable_stmt, ctx);
            }
            Stmt::Branch(bs) => {
                if let Some(label) = bs.label {
                    let ident = &self.ast_ident(label);
                    let name = &ident.name;
                    // determine and validate target
                    match &bs.token {
                        Token::BREAK => {
                            // spec: "If there is a label, it must be that of an enclosing
                            // "for", "switch", or "select" statement, and that is the one
                            // whose execution terminates."
                            let valid = match block.borrow().enclosing_target(name, self.ast_objs) {
                                Some(t) => match &self.ast_objs.l_stmts[t].stmt {
                                    Stmt::Switch(_)
                                    | Stmt::TypeSwitch(_)
                                    | Stmt::Select(_)
                                    | Stmt::For(_)
                                    | Stmt::Range(_) => true,
                                    _ => false,
                                },
                                None => false,
                            };
                            if !valid {
                                self.error(ident.pos, format!("invalid break label {}", name));
                                return;
                            }
                        }
                        Token::CONTINUE => {
                            let valid = match block.borrow().enclosing_target(name, self.ast_objs) {
                                Some(t) => match &self.ast_objs.l_stmts[t].stmt {
                                    Stmt::For(_) | Stmt::Range(_) => true,
                                    _ => false,
                                },
                                None => false,
                            };
                            if !valid {
                                self.error(ident.pos, format!("invalid continue label {}", name));
                                return;
                            }
                        }
                        Token::GOTO => {
                            if block.borrow().goto_target(name).is_none() {
                                ctx.fwd_jumps.push(bs.clone());
                                return;
                            }
                        }
                        _ => {
                            self.invalid_ast(
                                stmt.pos(self.ast_objs),
                                &format!("branch statement: {} {}", &bs.token, name),
                            );
                            return;
                        }
                    }

                    // record label use
                    let scope = &self.tc_objs.scopes[all];
                    let lobj = *scope.lookup(name).unwrap();
                    self.tc_objs.lobjs[lobj]
                        .entity_type_mut()
                        .label_set_used(true);
                    self.result.record_use(label, lobj);
                }
            }
            Stmt::Assign(akey) => {
                let astmt = &self.ast_objs.a_stmts[*akey];
                if astmt.token == Token::DEFINE {
                    ctx.record_var_decl(astmt.pos(self.ast_objs));
                }
            }
            Stmt::Block(bs) => {
                block_branches(ctx.lstmt, &bs.list, ctx);
            }
            Stmt::If(ifstmt) => {
                let body = Stmt::Block(ifstmt.body.clone());
                self.stmt_branches(all, block.clone(), &body, ctx);
                if let Some(els) = &ifstmt.els {
                    self.stmt_branches(all, block, els, ctx);
                }
            }
            Stmt::Case(cc) => {
                block_branches(None, &cc.body, ctx);
            }
            Stmt::Switch(ss) => {
                let body = Stmt::Block(ss.body.clone());
                self.stmt_branches(all, block, &body, ctx);
            }
            Stmt::TypeSwitch(tss) => {
                let body = Stmt::Block(tss.body.clone());
                self.stmt_branches(all, block, &body, ctx);
            }
            Stmt::Comm(cc) => {
                block_branches(None, &cc.body, ctx);
            }
            Stmt::Select(ss) => {
                let body = Stmt::Block(ss.body.clone());
                self.stmt_branches(all, block, &body, ctx);
            }
            Stmt::For(fs) => {
                let body = Stmt::Block(fs.body.clone());
                self.stmt_branches(all, block, &body, ctx);
            }
            Stmt::Range(rs) => {
                let body = Stmt::Block(rs.body.clone());
                self.stmt_branches(all, block, &body, ctx);
            }
            _ => {}
        }
    }
}
