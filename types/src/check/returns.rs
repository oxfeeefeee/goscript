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

use super::check::Checker;
use goscript_parser::ast::{BlockStmt, Expr, Stmt};
use goscript_parser::Token;
use std::rc::Rc;

impl<'a, S: SourceRead> Checker<'a, S> {
    /// is_terminating returns if s is a terminating statement.
    /// If s is labeled, label is the label name
    pub fn is_terminating(&self, s: &Stmt, label: Option<&String>) -> bool {
        match s {
            Stmt::Labeled(ls) => {
                let ls_val = &self.ast_objs.l_stmts[*ls];
                let l = &self.ast_ident(ls_val.label).name;
                self.is_terminating(&ls_val.stmt, Some(l))
            }
            Stmt::Expr(e) => match Checker::<S>::unparen(e) {
                Expr::Call(ce) => match &self.octx.panics {
                    Some(pa) => pa.contains(&ce.id()),
                    _ => false,
                },
                _ => false,
            },
            Stmt::Return(_) => true,
            Stmt::Branch(bs) => match bs.token {
                Token::GOTO | Token::FALLTHROUGH => true,
                _ => false,
            },
            Stmt::Block(bs) => self.is_terminating_list(&bs.list, None),
            Stmt::If(ifs) => {
                ifs.els.is_some()
                    && self.is_terminating_list(&ifs.body.list, None)
                    && self.is_terminating(ifs.els.as_ref().unwrap(), None)
            }
            Stmt::Switch(ss) => self.is_terminating_switch(&ss.body, label),
            Stmt::TypeSwitch(tss) => self.is_terminating_switch(&tss.body, label),
            Stmt::Select(ss) => !ss.body.list.iter().any(|x| match x {
                Stmt::Comm(cc) => {
                    !self.is_terminating_list(&cc.body, None)
                        || self.has_break_list(&cc.body, label, true)
                }
                _ => unreachable!(),
            }),
            Stmt::For(fs) => fs.cond.is_none() && !self.has_break_list(&fs.body.list, label, true),
            Stmt::Bad(_)
            | Stmt::Decl(_)
            | Stmt::Empty(_)
            | Stmt::Send(_)
            | Stmt::IncDec(_)
            | Stmt::Assign(_)
            | Stmt::Go(_)
            | Stmt::Defer(_)
            | Stmt::Range(_) => false,
            _ => unreachable!(),
        }
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

    fn is_terminating_switch(&self, body: &Rc<BlockStmt>, label: Option<&String>) -> bool {
        let mut has_default = false;
        for s in body.list.iter() {
            match s {
                Stmt::Case(cc) => {
                    if cc.list.is_none() {
                        has_default = true
                    }
                    if !self.is_terminating_list(&cc.body, None)
                        || self.has_break_list(&cc.body, label, true)
                    {
                        return false;
                    }
                }
                _ => unreachable!(),
            }
        }
        has_default
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
