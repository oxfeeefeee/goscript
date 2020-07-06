#![allow(dead_code)]
use super::super::display::{ExprDisplay, OperandDisplay, TypeDisplay};
use super::super::lookup;
use super::super::obj::EntityType;
use super::super::objects::{DeclInfoKey, ObjKey, PackageKey, ScopeKey, TCObjects, TypeKey};
use super::super::operand::{Operand, OperandMode};
use super::super::scope::Scope;
use super::super::typ;
use super::check::{Checker, FilesContext};
use super::util::{UnpackResult, UnpackedResultLeftovers};
use goscript_parser::ast::{CallExpr, Expr, FieldList, Node, SelectorExpr};
use goscript_parser::objects::{FuncTypeKey, IdentKey};

impl<'a> Checker<'a> {
    pub fn selector(&mut self, x: &mut Operand, e: &SelectorExpr) {
        unimplemented!()
    }

    pub fn arguments(
        &mut self,
        x: &mut Operand,
        call: &CallExpr,
        sig: TypeKey,
        re: &UnpackedResultLeftovers,
        n: usize,
    ) {
        unimplemented!()
    }
}
