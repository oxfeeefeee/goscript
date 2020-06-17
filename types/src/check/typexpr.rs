use super::super::objects::{DeclInfoKey, ObjKey, PackageKey, ScopeKey, TCObjects, TypeKey};
use super::check::Checker;
use goscript_parser::ast::Expr;

impl<'a> Checker<'a> {
    pub fn type_expr(&mut self, e: &Expr) -> TypeKey {
        unimplemented!()
    }

    pub fn defined_type(&mut self, e: &Expr, def: TypeKey) -> TypeKey {
        unimplemented!()
    }
}
