#![macro_use]
#![allow(unused_macros)]
#![allow(dead_code)]
use super::check::DeclInfo;
use super::constant;
use super::obj::LangObj;
use super::package::Package;
use super::scope::Scope;
use super::typ::Type;
use super::universe::Universe;
use goscript_parser::position;
use std::borrow::Cow;

use slotmap::{new_key_type, DenseSlotMap};

const DEFAULT_CAPACITY: usize = 16;

macro_rules! null_key {
    () => {
        slotmap::Key::null()
    };
}

macro_rules! new_objects {
    () => {
        DenseSlotMap::with_capacity_and_key(DEFAULT_CAPACITY)
    };
}

new_key_type! { pub struct ObjKey; }
new_key_type! { pub struct TypeKey; }
new_key_type! { pub struct PackageKey; }
new_key_type! { pub struct DeclInfoKey; }
new_key_type! { pub struct ScopeKey; }

pub type LangObjs = DenseSlotMap<ObjKey, LangObj>;
pub type Types = DenseSlotMap<TypeKey, Type>;
pub type Packages = DenseSlotMap<PackageKey, Package>;
pub type Decls = DenseSlotMap<DeclInfoKey, DeclInfo>;
pub type Scopes = DenseSlotMap<ScopeKey, Scope>;

/// The container of all "managed" objects
/// also works as a "global" variable holder
pub struct TCObjects {
    pub lobjs: LangObjs,
    pub types: Types,
    pub pkgs: Packages,
    pub decls: Decls,
    pub scopes: Scopes,
    pub universe: Option<Universe>,
    // "global" variable
    pub debug: bool,
    // "global" variable
    pub fmt_qualifier: Box<dyn Fn(&Package) -> Cow<str>>,
}

fn default_fmt_qualifier(p: &Package) -> Cow<str> {
    p.path().into()
}

impl TCObjects {
    pub fn new() -> TCObjects {
        let fmtq = Box::new(default_fmt_qualifier);
        let mut objs = TCObjects {
            lobjs: new_objects!(),
            types: new_objects!(),
            pkgs: new_objects!(),
            decls: new_objects!(),
            scopes: new_objects!(),
            universe: None,
            debug: true,
            fmt_qualifier: fmtq,
        };
        objs.universe = Some(Universe::new(&mut objs));
        objs
    }

    pub fn new_scope(
        &mut self,
        parent: Option<ScopeKey>,
        pos: position::Pos,
        end: position::Pos,
        comment: String,
        is_func: bool,
    ) -> ScopeKey {
        let scope = Scope::new(parent, pos, end, comment, is_func);
        let skey = self.scopes.insert(scope);
        if let Some(skey) = parent {
            // don't add children to Universe scope
            if skey != *self.universe().scope() {
                self.scopes[skey].add_child(skey);
            }
        }
        skey
    }

    pub fn new_package(&mut self, path: String) -> PackageKey {
        let skey = self.new_scope(
            Some(*self.universe().scope()),
            0,
            0,
            format!("package {}", path),
            false,
        );
        let pkg = Package::new(path, skey);
        self.pkgs.insert(pkg)
    }

    pub fn new_pkg_name(
        &mut self,
        pos: position::Pos,
        pkg: Option<PackageKey>,
        name: String,
        imported: PackageKey,
    ) -> ObjKey {
        let lobj = LangObj::new_pkg_name(pos, pkg, name, imported, self.universe());
        self.lobjs.insert(lobj)
    }

    pub fn new_const(
        &mut self,
        pos: position::Pos,
        pkg: Option<PackageKey>,
        name: String,
        typ: Option<TypeKey>,
        val: constant::Value,
    ) -> ObjKey {
        let lobj = LangObj::new_const(pos, pkg, name, typ, val);
        self.lobjs.insert(lobj)
    }

    pub fn new_type_name(
        &mut self,
        pos: position::Pos,
        pkg: Option<PackageKey>,
        name: String,
        typ: Option<TypeKey>,
    ) -> ObjKey {
        let lobj = LangObj::new_type_name(pos, pkg, name, typ);
        self.lobjs.insert(lobj)
    }

    pub fn new_var(
        &mut self,
        pos: position::Pos,
        pkg: Option<PackageKey>,
        name: String,
        typ: Option<TypeKey>,
    ) -> ObjKey {
        let lobj = LangObj::new_var(pos, pkg, name, typ);
        self.lobjs.insert(lobj)
    }

    pub fn new_param_var(
        &mut self,
        pos: position::Pos,
        pkg: Option<PackageKey>,
        name: String,
        typ: Option<TypeKey>,
    ) -> ObjKey {
        let lobj = LangObj::new_param_var(pos, pkg, name, typ);
        self.lobjs.insert(lobj)
    }

    pub fn new_field(
        &mut self,
        pos: position::Pos,
        pkg: Option<PackageKey>,
        name: String,
        typ: Option<TypeKey>,
        embedded: bool,
    ) -> ObjKey {
        let lobj = LangObj::new_field(pos, pkg, name, typ, embedded);
        self.lobjs.insert(lobj)
    }

    pub fn new_func(
        &mut self,
        pos: position::Pos,
        pkg: Option<PackageKey>,
        name: String,
        typ: Option<TypeKey>,
    ) -> ObjKey {
        let lobj = LangObj::new_func(pos, pkg, name, typ);
        self.lobjs.insert(lobj)
    }

    pub fn universe(&self) -> &Universe {
        self.universe.as_ref().unwrap()
    }
}
