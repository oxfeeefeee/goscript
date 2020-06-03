#![allow(dead_code)]
use super::super::constant::Value;
use super::super::objects::{DeclKey, ObjKey, PackageKey, ScopeKey, TCObjects, TypeKey};
use super::super::operand::OperandMode;
use super::super::scope::Scope;
use super::super::selection::Selection;
use goscript_parser::ast::{Expr, FuncDecl, NodeId};
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

/// Types info holds the results of Type Checking
pub struct TypeInfo {
    /// 'types' maps expressions to their types, and for constant
    /// expressions, also their values. Invalid expressions are
    /// omitted.
    ///
    /// For (possibly parenthesized) identifiers denoting built-in
    /// functions, the recorded signatures are call-site specific:
    /// if the call result is not a constant, the recorded type is
    /// an argument-specific signature. Otherwise, the recorded type
    /// is invalid.
    ///
    /// 'types' does not record the type of every identifier,
    /// only those that appear where an arbitrary expression is
    /// permitted. For instance, the identifier f in a selector
    /// expression x.f is found only in the Selections map, the
    /// identifier z in a variable declaration 'var z int' is found
    /// only in the Defs map, and identifiers denoting packages in
    /// qualified identifiers are collected in the Uses map.
    types: HashMap<NodeId, TypeAndValue>,
    /// 'defs' maps identifiers to the objects they define (including
    /// package names, dots "." of dot-imports, and blank "_" identifiers).
    /// For identifiers that do not denote objects (e.g., the package name
    /// in package clauses, or symbolic variables t in t := x.(type) of
    /// type switch headers), the corresponding objects are None.
    ///
    /// For an embedded field, Defs returns the field it defines.
    ///
    /// Invariant: defs[id] == None || defs[id].pos() == id.pos()
    defs: HashMap<IdentKey, ObjKey>,
    /// 'uses' maps identifiers to the objects they denote.
    ///
    /// For an embedded field, 'uses' returns the TypeName it denotes.
    ///
    /// Invariant: uses[id].pos() != id.pos()
    uses: HashMap<IdentKey, ObjKey>,
    /// 'implicits' maps nodes to their implicitly declared objects, if any.
    /// The following node and object types may appear:
    ///
    ///     node               declared object
    ///
    ///     ImportSpec    PkgName for imports without renames
    ///     CaseClause    type-specific Object::Var for each type switch case clause (incl. default)
    ///     Field         anonymous parameter Object::Var
    implicites: HashMap<NodeId, ObjKey>,
    /// 'selections' maps selector expressions (excluding qualified identifiers)
    /// to their corresponding selections.
    selections: HashMap<NodeId, Selection>,
    /// 'scopes' maps ast::Nodes to the scopes they define. Package scopes are not
    /// associated with a specific node but with all files belonging to a package.
    /// Thus, the package scope can be found in the type-checked Package object.
    /// Scopes nest, with the Universe scope being the outermost scope, enclosing
    /// the package scope, which contains (one or more) files scopes, which enclose
    /// function scopes which in turn enclose statement and function literal scopes.
    /// Note that even though package-level functions are declared in the package
    /// scope, the function scopes are embedded in the file scope of the file
    /// containing the function declaration.
    ///
    /// The following node types may appear in Scopes:
    ///
    ///     File
    ///     FuncType
    ///     BlockStmt
    ///     IfStmt
    ///     SwitchStmt
    ///     TypeSwitchStmt
    ///     CaseClause
    ///     CommClause
    ///     ForStmt
    ///     RangeStmt
    scopes: HashMap<NodeId, ScopeKey>,
    /// 'init_order' is the list of package-level initializers in the order in which
    /// they must be executed. Initializers referring to variables related by an
    /// initialization dependency appear in topological order, the others appear
    /// in source order. Variables without an initialization expression do not
    /// appear in this list.
    init_order: Vec<Initializer>,
}

pub struct ImportKey {
    pub path: String,
    pub dir: String,
}

pub struct Checker {}
