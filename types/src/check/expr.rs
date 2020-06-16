use super::super::objects::{DeclInfoKey, ObjKey, PackageKey, ScopeKey, TCObjects, TypeKey};
use super::super::operand::Operand;
use super::check::Checker;
use goscript_parser::ast::Expr;

impl<'a> Checker<'a> {
    pub fn expr(&mut self, x: &mut Operand, e: &Expr) {
        unimplemented!()
    }
}
