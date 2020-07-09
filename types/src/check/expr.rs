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

    /// raw_expr typechecks expression e and initializes x with the expression
    /// value or type. If an error occurred, x.mode is set to invalid.
    /// If hint is_some(), it is the type of a composite literal element.
    pub fn raw_expr(&mut self, x: &mut Operand, e: &Expr, hint: Option<TypeKey>) {
        unimplemented!()
    }

    /// multi_expr is like expr but the result may be a multi-value.
    pub fn multi_expr(&mut self, x: &mut Operand, e: &Expr) {
        unimplemented!()
    }

    /// expr_or_type typechecks expression or type e and initializes x with the expression
    /// value or type. If an error occurred, x.mode is set to invalid.
    pub fn expr_or_type(&mut self, x: &mut Operand, e: &Expr) {
        unimplemented!()
    }

    pub fn single_value(&mut self, x: &mut Operand) {
        unimplemented!()
    }

    pub fn convert_untyped(&mut self, x: &mut Operand, target: TypeKey) {
        unimplemented!()
    }

    pub fn update_expr_type(&mut self, e: &Expr, tkey: TypeKey, final_: bool) {
        unimplemented!()
    }

    /// index checks an index expression for validity.
    /// If max >= 0, it is the upper bound for index.
    /// If the result >= 0, then it is the constant value of index.
    pub fn index(&mut self, index: &Expr, max: Option<isize>) -> Option<isize> {
        unimplemented!()
    }
}
