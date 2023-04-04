// Copyright 2022 The Goscript Authors. All rights reserved.
// Use of this source code is governed by a BSD-style
// license that can be found in the LICENSE file.
//
//
// This code is adapted from the offical Go code written in Go
// with license as follows:
// Copyright 2013 The Go Authors. All rights reserved.
// Use of this source code is governed by a BSD-style
// license that can be found in the LICENSE file.

#![allow(dead_code)]
use super::super::constant::Value;
use super::super::importer::{ImportKey, Importer, SourceRead, TraceConfig};
use super::super::objects::{DeclInfoKey, ObjKey, PackageKey, ScopeKey, TCObjects, TypeKey};
use super::super::operand::OperandMode;
use super::super::selection::Selection;
use super::interface::IfaceInfo;
use go_parser::ast;
use go_parser::ast::{Expr, Node, NodeId};
use go_parser::{AstObjects, ErrorList, FilePosErrors, FileSet, IdentKey, Map, Pos};
use std::cell::RefCell;
use std::collections::HashSet;
use std::rc::Rc;

/// TypeAndValue reports the type and value (for constants, stored in 'mode')
/// of the corresponding expression.
#[derive(Debug, Clone)]
pub struct TypeAndValue {
    pub mode: OperandMode,
    pub typ: TypeKey,
}

impl TypeAndValue {
    pub fn get_const_val(&self) -> Option<&Value> {
        match &self.mode {
            OperandMode::Constant(v) => Some(v),
            _ => None,
        }
    }
}

/// An Initializer describes a package-level variable, or a list of variables in case
/// of a multi-valued initialization expression, and the corresponding initialization
/// expression.
#[derive(Debug)]
pub struct Initializer {
    pub lhs: Vec<ObjKey>,
    pub rhs: Expr,
}

/// Types info holds the results of Type Checking
#[derive(Debug)]
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
    pub types: Map<NodeId, TypeAndValue>,
    /// 'defs' maps identifiers to the objects they define (including
    /// package names, dots "." of dot-imports, and blank "_" identifiers).
    /// For identifiers that do not denote objects (e.g., the package name
    /// in package clauses, or symbolic variables t in t := x.(type) of
    /// type switch headers), the corresponding objects are None.
    ///
    /// For an embedded field, Defs returns the field it defines.
    ///
    /// Invariant: defs\[id\] == None || defs\[id\].pos() == id.pos()
    pub defs: Map<IdentKey, Option<ObjKey>>,
    /// 'uses' maps identifiers to the objects they denote.
    ///
    /// For an embedded field, 'uses' returns the TypeName it denotes.
    ///
    /// Invariant: uses\[id\].pos() != id.pos()
    pub uses: Map<IdentKey, ObjKey>,
    /// 'implicits' maps nodes to their implicitly declared objects, if any.
    /// The following node and object types may appear:
    ///     node               declared object
    ///     ImportSpec    PkgName for imports without renames
    ///     CaseClause    type-specific Object::Var for each type switch case clause (incl. default)
    ///     Field         anonymous parameter Object::Var
    pub implicits: Map<NodeId, ObjKey>,
    /// 'selections' maps selector expressions (excluding qualified identifiers)
    /// to their corresponding selections.
    pub selections: Map<NodeId, Selection>,
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
    pub scopes: Map<NodeId, ScopeKey>,
    /// 'init_order' is the list of package-level initializers in the order in which
    /// they must be executed. Initializers referring to variables related by an
    /// initialization dependency appear in topological order, the others appear
    /// in source order. Variables without an initialization expression do not
    /// appear in this list.
    pub init_order: Vec<Initializer>,
    /// oxfeeefeee: parse result of the package, to be used by code gen
    pub ast_files: Vec<ast::File>,
}

impl TypeInfo {
    pub fn new() -> TypeInfo {
        TypeInfo {
            types: Map::new(),
            defs: Map::new(),
            uses: Map::new(),
            implicits: Map::new(),
            selections: Map::new(),
            scopes: Map::new(),
            init_order: Vec::new(),
            ast_files: Vec::new(),
        }
    }
}

/// ExprInfo stores information about an untyped expression.
#[derive(Debug)]
pub struct ExprInfo {
    pub is_lhs: bool,
    pub mode: OperandMode,
    pub typ: Option<TypeKey>,
}

// ObjContext is context within which the current object is type-checked
// (valid only for the duration of type-checking a specific object)
#[derive(Clone)]
pub struct ObjContext {
    // package-level declaration whose init expression/function body is checked
    pub decl: Option<DeclInfoKey>,
    // top-most scope for lookups
    pub scope: Option<ScopeKey>,
    // if valid, identifiers are looked up as if at position pos (used by Eval)
    pub pos: Option<Pos>,
    // value of iota in a constant declaration; None otherwise
    pub iota: Option<Value>,
    // function signature if inside a function; None otherwise
    pub sig: Option<TypeKey>,
    // set of panic call ids (used for termination check)
    pub panics: Option<HashSet<NodeId>>,
    // set if a function makes use of labels (only ~1% of functions); unused outside functions
    pub has_label: bool,
    // set if an expression contains a function call or channel receive operation
    pub has_call_or_recv: bool,
}

type DelayedAction<S> = Box<dyn FnOnce(&mut Checker<S>, &mut FilesContext<S>)>;

pub type RcIfaceInfo = Rc<IfaceInfo>;

/// FilesContext contains information collected during type-checking
/// of a set of package files
pub struct FilesContext<'a, S: SourceRead> {
    // package files
    pub files: &'a Vec<ast::File>,
    // positions of unused dot-imported packages for each file scope
    pub unused_dot_imports: Map<ScopeKey, Map<PackageKey, Pos>>,
    // maps package scope type names(LangObj::TypeName) to associated
    // non-blank, non-interface methods(LangObj::Func)
    pub methods: Map<ObjKey, Vec<ObjKey>>,
    // maps interface(LangObj::TypeName) type names to corresponding
    // interface infos
    pub ifaces: Map<ObjKey, Option<RcIfaceInfo>>,
    // map of expressions(ast::Expr) without final type
    pub untyped: Map<NodeId, ExprInfo>,
    // stack of delayed actions
    pub delayed: Vec<DelayedAction<S>>,
    // path of object dependencies during type inference (for cycle reporting)
    pub obj_path: Vec<ObjKey>,
}

pub struct Checker<'a, S: SourceRead> {
    // object container for type checker
    pub tc_objs: &'a mut TCObjects,
    // object container for AST
    pub ast_objs: &'a mut AstObjects,
    // errors
    errors: &'a ErrorList,
    // files in this package
    pub fset: &'a mut FileSet,
    // all packages checked so far
    pub all_pkgs: &'a mut Map<String, PackageKey>,
    // all results, i.e. including results collected from
    // previously created Checker instances
    all_results: &'a mut Map<PackageKey, TypeInfo>,
    // this package
    pub pkg: PackageKey,
    // maps package-level objects and (non-interface) methods to declaration info
    pub obj_map: Map<ObjKey, DeclInfoKey>,
    // maps (import path, source directory) to (complete or fake) package
    pub imp_map: Map<ImportKey, PackageKey>,
    // object context
    pub octx: ObjContext,

    trace_config: &'a TraceConfig,

    reader: &'a S,
    // result of type checking
    pub result: TypeInfo,
    // for debug
    pub indent: Rc<RefCell<usize>>,
}

impl ObjContext {
    pub fn new() -> ObjContext {
        ObjContext {
            decl: None,
            scope: None,
            pos: None,
            iota: None,
            sig: None,
            panics: None,
            has_label: false,
            has_call_or_recv: false,
        }
    }
}

impl<S: SourceRead> FilesContext<'_, S> {
    pub fn new(files: &Vec<ast::File>) -> FilesContext<'_, S> {
        FilesContext {
            files: files,
            unused_dot_imports: Map::new(),
            methods: Map::new(),
            ifaces: Map::new(),
            untyped: Map::new(),
            delayed: Vec::new(),
            obj_path: Vec::new(),
        }
    }

    /// file_name returns a filename suitable for debugging output.
    pub fn file_name(&self, index: usize, checker: &Checker<S>) -> String {
        let file = &self.files[index];
        let pos = file.pos(checker.ast_objs);
        if pos > 0 {
            checker.fset.file(pos).unwrap().name().to_owned()
        } else {
            format!("file[{}]", index)
        }
    }

    pub fn add_unused_dot_import(&mut self, scope: &ScopeKey, pkg: &PackageKey, pos: Pos) {
        if !self.unused_dot_imports.contains_key(scope) {
            self.unused_dot_imports.insert(*scope, Map::new());
        }
        self.unused_dot_imports
            .get_mut(scope)
            .unwrap()
            .insert(*pkg, pos);
    }

    pub fn remember_untyped(&mut self, e: &Expr, ex_info: ExprInfo) {
        self.untyped.insert(e.id(), ex_info);
    }

    /// later pushes f on to the stack of actions that will be processed later;
    /// either at the end of the current statement, or in case of a local constant
    /// or variable declaration, before the constant or variable is in scope
    /// (so that f still sees the scope before any new declarations).
    pub fn later(&mut self, action: DelayedAction<S>) {
        self.delayed.push(action);
    }

    pub fn delayed_count(&self) -> usize {
        self.delayed.len()
    }

    pub fn process_delayed(&mut self, top: usize, checker: &mut Checker<S>) {
        let fs: Vec<DelayedAction<S>> = self.delayed.drain(top..).into_iter().collect();
        for f in fs {
            f(checker, self);
        }
    }

    /// push pushes obj to obj_path and returns it's index
    pub fn push(&mut self, obj: ObjKey) -> usize {
        self.obj_path.push(obj);
        self.obj_path.len() - 1
    }

    pub fn pop(&mut self) -> ObjKey {
        self.obj_path.pop().unwrap()
    }
}

impl TypeAndValue {
    fn new(mode: OperandMode, typ: TypeKey) -> TypeAndValue {
        TypeAndValue {
            mode: mode,
            typ: typ,
        }
    }
}

impl TypeInfo {
    pub fn record_type_and_value(&mut self, e: &Expr, mode: OperandMode, typ: TypeKey) {
        self.record_type_and_value_with_id(e.id(), mode, typ);
    }

    pub fn record_type_and_value_with_id(&mut self, id: NodeId, mode: OperandMode, typ: TypeKey) {
        if let OperandMode::Invalid = mode {
            return;
        }
        self.types.insert(id, TypeAndValue::new(mode, typ));
    }

    pub fn record_builtin_type(&mut self, mode: &OperandMode, e: &Expr, sig: TypeKey) {
        let mut expr = e;
        // expr must be a (possibly parenthesized) identifier denoting a built-in
        // (built-ins in package unsafe always produce a constant result and
        // we don't record their signatures, so we don't see qualified idents
        // here): record the signature for f and possible children.
        loop {
            self.record_type_and_value(expr, mode.clone(), sig);
            match expr {
                Expr::Ident(_) => break,
                Expr::Paren(p) => expr = &(*p).expr,
                _ => unreachable!(),
            }
        }
    }

    pub fn record_comma_ok_types<S: SourceRead>(
        &mut self,
        e: &Expr,
        t: &[TypeKey; 2],
        tc_objs: &mut TCObjects,
        ast_objs: &AstObjects,
        pkg: PackageKey,
    ) {
        let pos = e.pos(ast_objs);
        let mut expr = e;
        loop {
            let tv = self.types.get_mut(&expr.id()).unwrap();
            tv.typ = Checker::<S>::comma_ok_type(tc_objs, pos, pkg, t);
            match expr {
                Expr::Paren(p) => expr = &(*p).expr,
                _ => break,
            }
        }
    }

    pub fn record_def(&mut self, id: IdentKey, obj: Option<ObjKey>) {
        self.defs.insert(id, obj);
    }

    pub fn record_use(&mut self, id: IdentKey, obj: ObjKey) {
        self.uses.insert(id, obj);
    }

    pub fn record_implicit(&mut self, node: &impl Node, obj: ObjKey) {
        self.implicits.insert(node.id(), obj);
    }

    pub fn record_selection(&mut self, expr: &ast::SelectorExpr, sel: Selection) {
        self.record_use(expr.sel, sel.obj());
        self.selections.insert(expr.id(), sel);
    }

    pub fn record_scope(&mut self, node: &impl Node, scope: ScopeKey) {
        self.scopes.insert(node.id(), scope);
    }

    pub fn record_init_order(&mut self, init_order: Vec<Initializer>) {
        self.init_order = init_order;
    }
}

impl<'a, S: SourceRead> Checker<'a, S> {
    pub fn new(
        tc_objs: &'a mut TCObjects,
        ast_objs: &'a mut AstObjects,
        fset: &'a mut FileSet,
        errors: &'a ErrorList,
        pkgs: &'a mut Map<String, PackageKey>,
        all_results: &'a mut Map<PackageKey, TypeInfo>,
        pkg: PackageKey,
        cfg: &'a TraceConfig,
        reader: &'a S,
    ) -> Checker<'a, S> {
        Checker {
            tc_objs: tc_objs,
            ast_objs: ast_objs,
            fset: fset,
            errors: errors,
            all_pkgs: pkgs,
            all_results: all_results,
            pkg: pkg,
            obj_map: Map::new(),
            imp_map: Map::new(),
            octx: ObjContext::new(),
            trace_config: cfg,
            reader: reader,
            result: TypeInfo::new(),
            indent: Rc::new(RefCell::new(0)),
        }
    }

    pub fn check(mut self, mut files: Vec<ast::File>) -> Result<PackageKey, ()> {
        self.check_files_pkg_name(&files)?;
        let fctx = &mut FilesContext::new(&files);
        self.collect_objects(fctx);
        self.package_objects(fctx);
        fctx.process_delayed(0, &mut self);
        self.init_order();
        self.unused_imports(fctx);
        self.record_untyped(fctx);

        std::mem::swap(&mut self.result.ast_files, &mut files);
        self.all_results.insert(self.pkg, self.result);
        Ok(self.pkg)
    }

    fn record_untyped(&mut self, fctx: &mut FilesContext<S>) {
        for (id, info) in fctx.untyped.iter() {
            if info.mode != OperandMode::Invalid {
                self.result.record_type_and_value_with_id(
                    id.clone(),
                    info.mode.clone(),
                    info.typ.unwrap(),
                );
            }
        }
    }

    #[inline]
    pub fn errors(&self) -> &ErrorList {
        self.errors
    }

    #[inline]
    pub fn trace(&self) -> bool {
        self.trace_config.trace_checker
    }

    pub fn new_importer(&mut self, pos: Pos) -> Importer<S> {
        Importer::new(
            self.trace_config,
            self.reader,
            self.fset,
            self.all_pkgs,
            self.all_results,
            self.ast_objs,
            self.tc_objs,
            self.errors,
            pos,
        )
    }

    /// check files' package name
    fn check_files_pkg_name(&mut self, files: &Vec<ast::File>) -> Result<(), ()> {
        let mut pkg_name: Option<String> = None;
        for f in files.iter() {
            let ident = &self.ast_objs.idents[f.name];
            if pkg_name.is_none() {
                if ident.name == "_" {
                    self.error(ident.pos, "invalid package name _".to_owned());
                    return Err(());
                } else {
                    pkg_name = Some(ident.name.clone());
                }
            } else if &ident.name != pkg_name.as_ref().unwrap() {
                self.error(
                    f.package,
                    format!(
                        "package {}; expected {}",
                        ident.name,
                        pkg_name.as_ref().unwrap()
                    ),
                );
                return Err(());
            }
        }
        self.tc_objs.pkgs[self.pkg].set_name(pkg_name.unwrap());
        Ok(())
    }

    pub fn error(&self, pos: Pos, err: String) {
        self.error_impl(pos, err, false);
    }

    pub fn error_str(&self, pos: Pos, err: &str) {
        self.error_impl(pos, err.to_string(), false);
    }

    pub fn soft_error(&self, pos: Pos, err: String) {
        self.error_impl(pos, err, true);
    }

    pub fn soft_error_str(&self, pos: Pos, err: &str) {
        self.error_impl(pos, err.to_string(), true);
    }

    fn error_impl(&self, pos: Pos, err: String, soft: bool) {
        let file = self.fset.file(pos).unwrap();
        FilePosErrors::new(file, self.errors).add(pos, err, soft);
    }
}
