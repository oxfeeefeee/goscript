#![allow(dead_code)]
use super::super::objects::{DeclInfoKey, ObjKey, PackageKey, ScopeKey, TCObjects, TypeKey};
use super::super::operand::Operand;
use super::check::Checker;
use goscript_parser::ast::Expr;

impl<'a> Checker<'a> {
    /// expr typechecks expression e and initializes x with the expression value.
    /// The result must be a single value.
    /// If an error occurred, x.mode is set to invalid.
    pub fn expr(&mut self, x: &mut Operand, e: &Expr) {
        unimplemented!()
    }

    /// multi_expr is like expr but the result may be a multi-value.
    pub fn multi_expr(&mut self, x: &mut Operand, e: &Expr) {
        unimplemented!()
    }

    pub fn single_value(&mut self, x: &mut Operand) {
        unimplemented!()
    }

    pub fn convert_untyped(&mut self, x: &mut Operand, target: TypeKey) {
        unimplemented!()
    }
}
