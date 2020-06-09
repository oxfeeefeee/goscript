#![allow(dead_code)]
use super::operand::fmt_expr;
use goscript_parser::ast::Expr;
use goscript_parser::objects::Objects as AstObjects;
use std::fmt::{self};

pub struct ExprDisplay<'a> {
    expr: &'a Expr,
    objs: &'a AstObjects,
}

impl<'a> ExprDisplay<'a> {
    pub fn new(expr: &'a Expr, objs: &'a AstObjects) -> ExprDisplay<'a> {
        ExprDisplay {
            expr: expr,
            objs: objs,
        }
    }
}

impl<'a> fmt::Display for ExprDisplay<'a> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fmt_expr(self.expr, f, self.objs)
    }
}
