#![allow(dead_code)]
use super::super::constant::Value;
use super::super::objects::{DeclKey, ObjKey, PackageKey, ScopeKey, TCObjects, TypeKey};
use super::super::operand::OperandMode;
use super::super::scope::Scope;
use goscript_parser::ast::{Expr, ExprId, FuncDecl};
use goscript_parser::objects::{IdentKey, Objects as AstObjects};
use goscript_parser::position;
use std::collections::{HashMap, HashSet};

/// ExprInfo stores information about an untyped expression.
struct ExprInfo {
    is_lhs: bool,
    mode: OperandMode,
    typ: TypeKey,
    val: Value,
}

struct Context<'a> {
    // package-level declaration whose init expression/function body is checked
    decl: DeclKey,
    // top-most scope for lookups
    scope: ScopeKey,
    // if valid, identifiers are looked up as if at position pos (used by Eval)
    pos: position::Pos,
    // value of iota in a constant declaration; None otherwise
    iota: Option<Value>,
    // function signature if inside a function; None otherwise
    sig: Option<ObjKey>,
    // set of panic call ids (used for termination check)
    panics: Option<Vec<Expr>>,
    // set if a function makes use of labels (only ~1% of functions); unused outside functions
    has_label: bool,
    // set if an expression contains a function call or channel receive operation
    has_call_or_recv: bool,
    // object container for type checker
    tc_objs: &'a TCObjects,
    // object container for AST
    ast_objs: &'a AstObjects,
}

impl<'a> Context<'a> {
    pub fn lookup(&self, name: &str) -> Option<&ObjKey> {
        self.tc_objs.scopes[self.scope].lookup(name)
    }
}

/// TypeAndValue reports the type and value (for constants)
/// of the corresponding expression.
pub struct TypeAndValue {
    mode: OperandMode,
    typ: TypeKey,
    val: Value,
}

/// An Initializer describes a package-level variable, or a list of variables in case
/// of a multi-valued initialization expression, and the corresponding initialization
/// expression.
pub struct Initializer {
    lhs: Vec<ObjKey>,
    rhs: Expr,
}

// Types info holds the result of Type Checking
pub struct TypeInfo {
    types: HashMap<ExprId, TypeAndValue>,
}

pub struct ImportKey {
    pub path: String,
    pub dir: String,
}

pub struct Checker {}
