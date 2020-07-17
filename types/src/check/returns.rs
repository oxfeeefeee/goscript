#![allow(dead_code)]
use super::super::objects::ScopeKey;
use super::super::scope::Scope;
use super::check::Checker;
use goscript_parser::ast::Node;
use goscript_parser::ast::{BlockStmt, BranchStmt, Decl, Stmt};
use goscript_parser::objects::{LabeledStmtKey, Objects as AstObjects};
use goscript_parser::{Pos, Token};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

impl<'a> Checker<'a> {
    /// is_terminating returns if s is a terminating statement.
    /// If s is labeled, label is the label name
    pub fn is_terminating(&mut self, s: &Stmt, label: Option<String>) -> bool {
        unimplemented!()
    }

    /// has_break reports if s is or contains a break statement
    /// referring to the label-ed statement or implicit-ly the
    /// closest outer breakable statement.
    fn has_break(&self, s: &Stmt, label: Option<&String>, implicit: bool) -> bool {
        match s {
            Stmt::Bad(_)
            | Stmt::Decl(_)
            | Stmt::Empty(_)
            | Stmt::Expr(_)
            | Stmt::Send(_)
            | Stmt::IncDec(_)
            | Stmt::Assign(_)
            | Stmt::Go(_)
            | Stmt::Defer(_)
            | Stmt::Return(_) => false,
            Stmt::Labeled(ls) => {
                let l = &self.ast_ident(self.ast_objs.l_stmts[*ls].label).name;
                self.has_break(&Stmt::Labeled(*ls), Some(l), implicit)
            }
            Stmt::Branch(bs) => {
                bs.token == Token::BREAK
                    && (bs
                        .label
                        .map_or(implicit, |ikey| Some(&self.ast_ident(ikey).name) == label))
            }
            Stmt::Block(bs) => self.has_break_list(&bs.list, label, implicit),

            _ => unimplemented!(),
        }
    }

    fn has_break_list(&self, list: &Vec<Stmt>, label: Option<&String>, implicit: bool) -> bool {
        list.iter()
            .find(|x| self.has_break(x, label, implicit))
            .is_some()
    }
}
