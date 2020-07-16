#![allow(dead_code)]
use super::super::constant;
use super::super::obj::LangObj;
use super::super::objects::{DeclInfoKey, ObjKey, PackageKey, ScopeKey, TCObjects, TypeKey};
use super::super::operand::Operand;
use super::check::{Checker, ExprInfo, FilesContext};
use goscript_parser::ast::{BlockStmt, BranchStmt, Decl, Expr, Stmt};
use goscript_parser::objects::{FuncDeclKey, LabeledStmtKey, Objects as AstObjects};
use goscript_parser::scope::Scope;
use goscript_parser::{Pos, Token};
use std::cell::RefCell;
use std::collections::HashMap;
use std::ops::Deref;
use std::ptr::eq;
use std::rc::Rc;

struct Block {
    parent: Option<Rc<RefCell<Block>>>, // enclosing block
    lstmt: Option<LabeledStmtKey>,      // labeled statement to which this block belongs
    labels: HashMap<String, LabeledStmtKey>,
}

impl Block {
    fn new(parent: Option<Rc<RefCell<Block>>>, stmt: Option<LabeledStmtKey>) -> Block {
        Block {
            parent: parent,
            lstmt: stmt,
            labels: HashMap::new(),
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

struct StmtBranchesContext<'a> {
    var_decl_pos: Option<Pos>,
    fwd_jumps: Vec<&'a BranchStmt>,
    bad_jumps: Vec<&'a BranchStmt>,
    lstmt: Option<LabeledStmtKey>,
}

impl<'a> StmtBranchesContext<'a> {
    fn new(lstmt: Option<LabeledStmtKey>) -> StmtBranchesContext<'a> {
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

    fn jumps_over_var_decl(&self, bs: &BranchStmt) -> bool {
        if let Some(_) = self.var_decl_pos {
            self.bad_jumps.iter().find(|&&x| eq(x, bs)).is_some()
        } else {
            false
        }
    }
}

impl<'a> Checker<'a> {
    /// block_branches processes a block's statement list and returns the set of outgoing forward jumps.
    /// all is the scope of all declared labels, parent the set of labels declared in the immediately
    /// enclosing block, and lstmt is the labeled statement this block is associated with.
    fn block_branches<'c>(
        &'a self,
        all: &Scope,
        parent: Option<Rc<RefCell<Block>>>,
        lstmt: Option<LabeledStmtKey>,
        list: &'c Vec<Stmt>,
    ) -> Vec<&'c BranchStmt> {
        let b = Rc::new(RefCell::new(Block::new(parent, lstmt)));
        let mut ctx = StmtBranchesContext::new(lstmt);

        for s in list.iter() {
            self.stmt_branches(all, b.clone(), s, &mut ctx);
        }

        ctx.fwd_jumps
    }

    fn stmt_branches<'c>(
        &'a self,
        all: &Scope,
        block: Rc<RefCell<Block>>,
        stmt: &'c Stmt,
        ctx: &mut StmtBranchesContext<'c>,
    ) {
        match stmt {
            Stmt::Decl(d) => match &**d {
                Decl::Gen(gd) => {
                    if gd.token == Token::VAR {
                        ctx.record_var_decl(gd.token_pos)
                    }
                }
                _ => {}
            },
            Stmt::Block(bs) => {
                // Unresolved forward jumps inside the nested block
                // become forward jumps in the current block.
                ctx.fwd_jumps.append(&mut self.block_branches(
                    all,
                    Some(block),
                    ctx.lstmt,
                    &bs.list,
                ));
            }
            _ => {}
        }
    }
}
