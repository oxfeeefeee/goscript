#![allow(dead_code)]
use super::objects::{PackageKey, ScopeKey};
use std::borrow::Cow;
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
    pub fn new(path: String, name: Option<String>, scope: ScopeKey) -> Package {
        Package {
            path: path,
            name: name,
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

    pub fn fake(&self) -> &bool {
        &self.fake
    }

    pub fn mark_fake_with_name(&mut self, name: String) {
        self.fake = true;
        self.name = Some(name);
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

    pub fn imports_mut(&mut self) -> &mut Vec<PackageKey> {
        &mut self.imports
    }

    pub fn add_import(&mut self, pkey: PackageKey) {
        self.imports.push(pkey);
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
