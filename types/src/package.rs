#![allow(dead_code)]
use super::objects::{ObjKey, PackageKey, ScopeKey};
use goscript_parser::ast::{Expr, FuncDecl};
use std::borrow::Cow;
use std::collections::HashSet;
use std::fmt;

/// A Package describes a Go package.
pub struct Package {
    path: String,
    name: Option<String>,
    scope: ScopeKey,
    complete: bool,
    imports: Vec<PackageKey>,
    // scope lookup errors are silently dropped if package is fake (internal use only)
    fake: bool,
}

impl Package {
    pub fn new(path: String, scope: ScopeKey) -> Package {
        Package {
            path: path,
            name: None,
            scope: scope,
            complete: false,
            imports: Vec::new(),
            fake: false,
        }
    }

    pub fn path(&self) -> &String {
        &self.path
    }

    pub fn name(&self) -> &Option<String> {
        &self.name
    }

    pub fn set_name(&mut self, name: String) {
        self.name = Some(name)
    }

    /// Scope returns the (complete or incomplete) package scope
    /// holding the objects declared at package level (TypeNames,
    /// Consts, Vars, and Funcs).
    pub fn scope(&self) -> &ScopeKey {
        &self.scope
    }

    /// A package is complete if its scope contains (at least) all
    /// exported objects; otherwise it is incomplete.    
    pub fn complete(&self) -> &bool {
        &self.complete
    }

    pub fn mark_complete(&mut self) {
        self.complete = true
    }

    /// Imports returns the list of packages directly imported by
    /// pkg; the list is in source order.
    ///
    /// If pkg was loaded from export data, Imports includes packages that
    /// provide package-level objects referenced by pkg. This may be more or
    /// less than the set of packages directly imported by pkg's source code.
    pub fn imports(&self) -> &Vec<PackageKey> {
        &self.imports
    }

    /// SetImports sets the list of explicitly imported packages to list.
    /// It is the caller's responsibility to make sure list elements are unique.
    pub fn set_imports(&mut self, pkgs: Vec<PackageKey>) {
        self.imports = pkgs
    }

    pub fn fmt_with_qualifier(
        &self,
        f: &mut fmt::Formatter<'_>,
        qf: &dyn Fn(&Package) -> Cow<str>,
    ) -> fmt::Result {
        write!(f, "{}.", qf(self))
    }
}

impl fmt::Display for Package {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.name.is_none() {
            write!(f, "uninitialized package, path: {}", &self.path)
        } else {
            write!(
                f,
                "package {} ({})",
                &self.name.as_ref().unwrap(),
                &self.path
            )
        }
    }
}

/// DeclInfo describes a package-level const, type, var, or func declaration.
pub struct DeclInfo {
    file_scope: ScopeKey,     // scope of file containing this declaration
    lhs: Option<Vec<ObjKey>>, // lhs of n:1 variable declarations, or None
    typ: Option<Expr>,        // type, or None
    init: Option<Expr>,       // init/orig expression, or None
    fdecl: Option<FuncDecl>,  // func declaration, or None
    deps: HashSet<ObjKey>,    // deps tracks initialization expression dependencies.
}

impl DeclInfo {
    pub fn has_initializer(&self) -> bool {
        self.init.is_some() || self.fdecl.is_some() && self.fdecl.as_ref().unwrap().body.is_some()
    }

    pub fn add_dep(&mut self, okey: ObjKey) {
        self.deps.insert(okey);
    }
}
