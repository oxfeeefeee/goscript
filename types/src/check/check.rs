#![allow(dead_code)]
use super::super::constant::Value;
use super::super::obj;
use super::super::objects::{DeclKey, ObjKey, PackageKey, ScopeKey, TCObjects, TypeKey};
use super::super::operand::OperandMode;
use super::super::scope::Scope;
use super::super::selection::Selection;
use super::super::typ;
use super::interface::IfaceInfo;
use super::resolver::DeclInfo;
use goscript_parser::ast::Node;
use goscript_parser::ast::{Expr, FuncDecl, NodeId};
use goscript_parser::errors::{ErrorList, FilePosErrors};
use goscript_parser::objects::{IdentKey, Objects as AstObjects};
use goscript_parser::position::{File, Pos};
use goscript_parser::FileSet;
use std::collections::{HashMap, HashSet};

/// TypeAndValue reports the type and value (for constants)
/// of the corresponding expression.
pub struct TypeAndValue {
    mode: OperandMode,
    typ: TypeKey,
    val: Option<Value>,
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

/// ExprInfo stores information about an untyped expression.
struct ExprInfo {
    is_lhs: bool,
    mode: OperandMode,
    typ: TypeKey,
    val: Value,
}

// ObjContext is context within which the current object is type-checked
// (valid only for the duration of type-checking a specific object)
pub struct ObjContext {
    // package-level declaration whose init expression/function body is checked
    decl: DeclKey,
    // top-most scope for lookups
    scope: ScopeKey,
    // if valid, identifiers are looked up as if at position pos (used by Eval)
    pos: Pos,
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
}

impl ObjContext {
    pub fn lookup<'a>(&self, name: &str, tc_objs: &'a TCObjects) -> Option<&'a ObjKey> {
        tc_objs.scopes[self.scope].lookup(name)
    }
}

type DelayedAction = fn(&Checker);

/// FilesContext contains information collected during type-checking
/// of a set of package files
pub struct FilesContext<'a> {
    // package files
    files: Vec<&'a File>,
    // positions of unused dot-imported packages for each file scope
    unused_dot_imports: HashMap<ScopeKey, HashMap<PackageKey, Pos>>,
    // maps package scope type names(LangObj::TypeName) to associated
    // non-blank, non-interface methods(LangObj::Func)
    methods: HashMap<ObjKey, Vec<ObjKey>>,
    // maps interface(LangObj::TypeName) type names to corresponding
    // interface infos
    ifaces: HashMap<ObjKey, IfaceInfo>,
    // map of expressions(ast::Expr) without final type
    untyped: HashMap<NodeId, ExprInfo>,
    // stack of delayed actions
    delayed: Vec<DelayedAction>,
    // path of object dependencies during type inference (for cycle reporting)
    obj_path: Vec<ObjKey>,
}

#[derive(PartialEq, Eq, Hash)]
pub struct ImportKey {
    pub path: String,
    pub dir: String,
}

pub struct Checker<'a> {
    // object container for type checker
    tc_objs: &'a mut TCObjects,
    // object container for AST
    ast_objs: &'a AstObjects,
    // errors
    errors: &'a FilePosErrors<'a>,
    // files in this package
    fset: &'a FileSet,
    // this package
    pkg: PackageKey,
    // maps package-level object to declaration info
    obj_map: HashMap<ObjKey, DeclInfo>,
    // maps (import path, source directory) to (complete or fake) package
    imp_map: HashMap<ImportKey, PackageKey>,
    // for debug
    indent: isize,
}

impl TypeAndValue {
    fn new(mode: OperandMode, typ: TypeKey, val: Option<Value>) -> TypeAndValue {
        TypeAndValue {
            mode: mode,
            typ: typ,
            val: val,
        }
    }
}

impl TypeInfo {
    pub fn record_type_and_value(
        &mut self,
        e: &Expr,
        mode: OperandMode,
        typ: TypeKey,
        val: Option<Value>,
    ) {
        assert!(val.is_some());
        if mode == OperandMode::Invalid {
            return;
        }
        self.types.insert(e.id(), TypeAndValue::new(mode, typ, val));
    }

    pub fn record_builtin_type(&mut self, e: &Expr, sig: TypeKey) {
        let mut expr = e;
        // expr must be a (possibly parenthesized) identifier denoting a built-in
        // (built-ins in package unsafe always produce a constant result and
        // we don't record their signatures, so we don't see qualified idents
        // here): record the signature for f and possible children.
        loop {
            self.record_type_and_value(expr, OperandMode::Builtin, sig, None);
            match expr {
                Expr::Ident(_) => break,
                Expr::Paren(p) => expr = &(*p).expr,
                _ => unreachable!(),
            }
        }
    }

    pub fn record_comma_ok_types(&mut self, e: &Expr, t: &[TypeKey; 2], checker: &mut Checker) {
        let mut expr = e;
        loop {
            let tv = self.types.get_mut(&expr.id()).unwrap();
            assert!(tv.val.is_some());
            tv.typ = checker.comma_ok_type(expr, t);
            match expr {
                Expr::Paren(p) => expr = &(*p).expr,
                _ => break,
            }
        }
    }
}

impl<'a> Checker<'a> {
    pub fn tc_objs(&self) -> &TCObjects {
        self.tc_objs
    }

    pub fn tc_objs_mut(&mut self) -> &mut TCObjects {
        self.tc_objs
    }

    pub fn ast_objs(&self) -> &AstObjects {
        self.ast_objs
    }

    pub fn errors(&self) -> &FilePosErrors<'a> {
        self.errors
    }

    pub fn fset(&self) -> &FileSet {
        self.fset
    }

    pub fn pkg(&self) -> &PackageKey {
        &self.pkg
    }

    pub fn obj_map(&self) -> &HashMap<ObjKey, DeclInfo> {
        &self.obj_map
    }

    pub fn obj_map_mut(&mut self) -> &mut HashMap<ObjKey, DeclInfo> {
        &mut self.obj_map
    }

    pub fn imp_map(&self) -> &HashMap<ImportKey, PackageKey> {
        &self.imp_map
    }

    pub fn imp_map_mut(&mut self) -> &mut HashMap<ImportKey, PackageKey> {
        &mut self.imp_map
    }

    pub fn comma_ok_type(&mut self, e: &Expr, t: &[TypeKey; 2]) -> TypeKey {
        let pos = e.pos(self.ast_objs);
        let vars = vec![
            self.tc_objs.lobjs.insert(obj::LangObj::new_var(
                pos,
                Some(self.pkg),
                String::new(),
                Some(t[0]),
            )),
            self.tc_objs.lobjs.insert(obj::LangObj::new_var(
                pos,
                Some(self.pkg),
                String::new(),
                Some(t[1]),
            )),
        ];
        self.tc_objs
            .types
            .insert(typ::Type::Tuple(typ::TupleDetail::new(vars)))
    }
}
