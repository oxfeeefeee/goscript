#![allow(dead_code)]
use super::check::MethodInfo;
use super::lookup::MethodSet;
use super::obj::fmt_obj;
use super::objects::TCObjects;
use super::objects::{ObjKey, ScopeKey, TypeKey};
use super::operand::{fmt_expr, Operand};
use super::scope::fmt_scope_full;
use super::selection::Selection;
use super::typ::fmt_type;
use goscript_parser::ast::Node;
use goscript_parser::objects::Objects as AstObjects;
use goscript_parser::{ast::Expr, Pos};
use std::fmt;

pub fn type_str(t: &TypeKey, tco: &TCObjects) -> String {
    format!("{}", Displayer::new(t, None, Some(tco)))
}

pub trait Display {
    fn format(
        &self,
        f: &mut fmt::Formatter,
        asto: Option<&AstObjects>,
        tco: Option<&TCObjects>,
    ) -> fmt::Result;

    fn position(&self, _: Option<&AstObjects>, _: Option<&TCObjects>) -> Pos {
        unreachable!()
    }
}

pub struct Displayer<'a> {
    obj: &'a dyn Display,
    asto: Option<&'a AstObjects>,
    tco: Option<&'a TCObjects>,
}

impl<'a> Displayer<'a> {
    pub fn new(
        obj: &'a dyn Display,
        asto: Option<&'a AstObjects>,
        tco: Option<&'a TCObjects>,
    ) -> Displayer<'a> {
        Displayer {
            obj,
            asto,
            tco,
        }
    }

    pub fn pos(&self) -> Pos {
        self.obj.position(self.asto, self.tco)
    }
}

impl<'a> fmt::Display for Displayer<'a> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.obj.format(f, self.asto, self.tco)
    }
}

impl<'a> Display for Expr {
    fn format(
        &self,
        f: &mut fmt::Formatter,
        asto: Option<&AstObjects>,
        _: Option<&TCObjects>,
    ) -> fmt::Result {
        fmt_expr(self, f, asto.as_ref().unwrap())
    }

    fn position(&self, asto: Option<&AstObjects>, _: Option<&TCObjects>) -> Pos {
        self.pos(asto.as_ref().unwrap())
    }
}

impl<'a> Display for Operand {
    fn format(
        &self,
        f: &mut fmt::Formatter,
        asto: Option<&AstObjects>,
        tco: Option<&TCObjects>,
    ) -> fmt::Result {
        self.fmt(f, tco.as_ref().unwrap(), asto.as_ref().unwrap())
    }

    fn position(&self, asto: Option<&AstObjects>, _: Option<&TCObjects>) -> Pos {
        self.pos(asto.as_ref().unwrap())
    }
}

impl<'a> Display for ObjKey {
    fn format(
        &self,
        f: &mut fmt::Formatter,
        _: Option<&AstObjects>,
        tco: Option<&TCObjects>,
    ) -> fmt::Result {
        fmt_obj(*self, f, tco.as_ref().unwrap())
    }

    fn position(&self, _: Option<&AstObjects>, tco: Option<&TCObjects>) -> Pos {
        tco.as_ref().unwrap().lobjs[*self].pos()
    }
}

impl<'a> Display for TypeKey {
    fn format(
        &self,
        f: &mut fmt::Formatter,
        _: Option<&AstObjects>,
        tco: Option<&TCObjects>,
    ) -> fmt::Result {
        fmt_type(Some(*self), f, tco.as_ref().unwrap())
    }
}

impl<'a> Display for MethodInfo {
    fn format(
        &self,
        f: &mut fmt::Formatter,
        asto: Option<&AstObjects>,
        tco: Option<&TCObjects>,
    ) -> fmt::Result {
        self.fmt(f, tco.as_ref().unwrap(), asto.as_ref().unwrap())
    }
}

impl<'a> Display for Selection {
    fn format(
        &self,
        f: &mut fmt::Formatter,
        _: Option<&AstObjects>,
        tco: Option<&TCObjects>,
    ) -> fmt::Result {
        self.fmt(f, tco.as_ref().unwrap())
    }
}

impl<'a> Display for MethodSet {
    fn format(
        &self,
        f: &mut fmt::Formatter,
        _: Option<&AstObjects>,
        tco: Option<&TCObjects>,
    ) -> fmt::Result {
        self.fmt(f, tco.as_ref().unwrap())
    }
}

impl<'a> Display for ScopeKey {
    fn format(
        &self,
        f: &mut fmt::Formatter,
        _: Option<&AstObjects>,
        tco: Option<&TCObjects>,
    ) -> fmt::Result {
        fmt_scope_full(self, f, 0, tco.as_ref().unwrap())
    }
}
