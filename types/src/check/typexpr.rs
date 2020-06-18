use super::super::objects::{DeclInfoKey, ObjKey, PackageKey, ScopeKey, TCObjects, TypeKey};
use super::check::Checker;
use goscript_parser::ast::{Expr, FieldList};
use goscript_parser::objects::FuncTypeKey;

impl<'a> Checker<'a> {
    pub fn type_expr(&mut self, e: &Expr) -> TypeKey {
        unimplemented!()
    }

    pub fn defined_type(&mut self, e: &Expr, def: TypeKey) -> TypeKey {
        unimplemented!()
    }

    pub fn func_type(&mut self, sig: TypeKey, recv: Option<FieldList>, ftype: FuncTypeKey) {
        unimplemented!()
    }
}
