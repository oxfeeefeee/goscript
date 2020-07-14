use super::super::check::MethodInfo;
use super::super::lookup::MethodSet;
use super::super::obj::fmt_obj;
use super::super::objects::{ObjKey, TypeKey};
use super::super::operand::{fmt_expr, Operand};
use super::super::selection::Selection;
use super::super::typ::fmt_type;
use super::Checker;
use goscript_parser::ast::Node;
use goscript_parser::{ast::Expr, Pos};
use std::fmt;

pub trait Display {
    fn format(&self, f: &mut fmt::Formatter, c: &Checker) -> fmt::Result;

    fn position(&self, _: &Checker) -> Pos {
        0
    }
}

pub struct Displayer<'a> {
    obj: &'a dyn Display,
    c: &'a Checker<'a>,
}

impl<'a> Displayer<'a> {
    pub fn new(obj: &'a dyn Display, c: &'a Checker<'a>) -> Displayer<'a> {
        Displayer { obj: obj, c: c }
    }

    pub fn pos(&self) -> Pos {
        self.obj.position(self.c)
    }
}

impl<'a> fmt::Display for Displayer<'a> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.obj.format(f, self.c)
    }
}

impl<'a> Display for Expr {
    fn format(&self, f: &mut fmt::Formatter, c: &Checker) -> fmt::Result {
        fmt_expr(self, f, c.ast_objs)
    }

    fn position(&self, c: &Checker) -> Pos {
        self.pos(c.ast_objs)
    }
}

impl<'a> Display for Operand {
    fn format(&self, f: &mut fmt::Formatter, c: &Checker) -> fmt::Result {
        self.fmt(f, c.tc_objs, c.ast_objs)
    }

    fn position(&self, c: &Checker) -> Pos {
        self.pos(c.ast_objs)
    }
}

impl<'a> Display for ObjKey {
    fn format(&self, f: &mut fmt::Formatter, c: &Checker) -> fmt::Result {
        fmt_obj(self, f, c.tc_objs)
    }

    fn position(&self, c: &Checker) -> Pos {
        c.lobj(*self).pos()
    }
}

impl<'a> Display for TypeKey {
    fn format(&self, f: &mut fmt::Formatter, c: &Checker) -> fmt::Result {
        fmt_type(Some(*self), f, c.tc_objs)
    }
}

impl<'a> Display for MethodInfo {
    fn format(&self, f: &mut fmt::Formatter, c: &Checker) -> fmt::Result {
        self.fmt(f, c.tc_objs, c.ast_objs)
    }
}

impl<'a> Display for Selection {
    fn format(&self, f: &mut fmt::Formatter, c: &Checker) -> fmt::Result {
        self.fmt(f, c.tc_objs)
    }
}

impl<'a> Display for MethodSet {
    fn format(&self, f: &mut fmt::Formatter, c: &Checker) -> fmt::Result {
        self.fmt(f, c.tc_objs)
    }
}
