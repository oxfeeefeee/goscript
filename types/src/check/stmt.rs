use super::super::constant;
use super::super::objects::{DeclInfoKey, ObjKey, PackageKey, ScopeKey, TCObjects, TypeKey};
use super::super::operand::Operand;
use super::check::Checker;
use goscript_parser::ast::Expr;
use goscript_parser::objects::FuncDeclKey;

impl<'a> Checker<'a> {
    pub fn func_body(
        &mut self,
        di: DeclInfoKey,
        name: &str,
        sig: TypeKey,
        fd: FuncDeclKey,
        iota: Option<constant::Value>,
    ) {
        unimplemented!()
    }
}
