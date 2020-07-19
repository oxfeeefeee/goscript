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
    pub fn is_terminating(&self, s: &Stmt, label: Option<&String>) -> bool {
        unimplemented!()
    }

    fn is_terminating_list(&self, list: &Vec<Stmt>, label: Option<&String>) -> bool {
        // trailing empty statements are permitted - skip them
        let non_empty = list.iter().rev().find(|x| match x {
            Stmt::Empty(_) => false,
            _ => true,
        });
        if let Some(s) = non_empty {
            self.is_terminating(s, label)
        } else {
            false
        }
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
                let ls_val = &self.ast_objs.l_stmts[*ls];
                let l = &self.ast_ident(ls_val.label).name;
                self.has_break(&ls_val.stmt, Some(l), implicit)
            }
            Stmt::Branch(bs) => {
                bs.token == Token::BREAK
                    && (bs
                        .label
                        .map_or(implicit, |ikey| Some(&self.ast_ident(ikey).name) == label))
            }
            Stmt::Block(bs) => self.has_break_list(&bs.list, label, implicit),
            Stmt::If(ifs) => {
                self.has_break_list(&ifs.body.list, label, implicit)
                    || ifs.els.is_some()
                        && self.has_break(ifs.els.as_ref().unwrap(), label, implicit)
            }
            Stmt::Case(cc) => self.has_break_list(&cc.body, label, implicit),
            Stmt::Switch(ss) => {
                label.is_some() && self.has_break_list(&ss.body.list, label, implicit)
            }
            Stmt::TypeSwitch(tss) => {
                label.is_some() && self.has_break_list(&tss.body.list, label, implicit)
            }
            Stmt::Comm(cc) => self.has_break_list(&cc.body, label, implicit),
            Stmt::Select(ss) => {
                label.is_some() && self.has_break_list(&ss.body.list, label, implicit)
            }
            Stmt::For(fs) => label.is_some() && self.has_break_list(&fs.body.list, label, implicit),
            Stmt::Range(rs) => {
                label.is_some() && self.has_break_list(&rs.body.list, label, implicit)
            }
        }
    }

    fn has_break_list(&self, list: &Vec<Stmt>, label: Option<&String>, implicit: bool) -> bool {
        list.iter()
            .find(|x| self.has_break(x, label, implicit))
            .is_some()
    }
}
