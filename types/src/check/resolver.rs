#![allow(dead_code)]
use super::super::objects::{DeclKey, ObjKey, PackageKey, ScopeKey, TCObjects, TypeKey};
use goscript_parser::ast::Expr;
use goscript_parser::objects::FuncDeclKey;
use std::collections::HashSet;

/// DeclInfo describes a package-level const, type, var, or func declaration.
pub struct DeclInfo {
    file_scope: ScopeKey,
    lhs: Vec<ObjKey>,
    typ: Option<Expr>,
    init: Option<Expr>,
    fdecl: FuncDeclKey,
    alias: bool,
    deps: HashSet<ObjKey>,
}
