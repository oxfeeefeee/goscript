#![allow(dead_code)]
use super::constant;
use super::objects::{PackageKey, ScopeKey, TCObjects, TypeKey};
use super::package::Package;
use goscript_parser::ast;
use goscript_parser::position;
use std::borrow::Cow;
use std::fmt;

/// A LangObj describes a named language entity such as a package,
/// constant, type, variable, function (incl. methods), or label.
///
pub struct LangObj {
    entity_typ: EntityType,
    parent: Option<ScopeKey>,
    pos: position::Pos,
    pkg: Option<PackageKey>,
    name: String,
    typ: Option<TypeKey>,
    order: u32,
    color: ObjColor,
    scope_pos: position::Pos,
}

/// EntityType defines the types of Object entities
pub enum EntityType {
    /// A PkgName represents an imported Go package.
    PkgName(PackageKey, bool),
    /// A Const represents a declared constant.
    Const(constant::Value),
    /// A TypeName represents a name for a (defined or alias) type.
    TypeName,
    /// A Variable represents a declared variable (including function
    /// parameters and results, and struct fields).
    Var(bool, bool, bool), // embedded, field, used
    /// A Func represents a declared function, concrete method, or abstract
    /// (interface) method. Its Type() is always a *Signature.
    /// An abstract method may belong to many interfaces due to embedding.
    Func,
    /// A Label represents a declared label.
    /// Labels don't have a type.
    Label(bool),
    /// A Builtin represents a built-in function.
    /// Builtins don't have a valid type.
    Builtin(/*todo: builtin id*/),
    /// Nil represents the predeclared value nil.
    Nil,
}

pub enum ObjColor {
    White,
    Black,
    Gray,
}

impl LangObj {
    pub fn new_pkg_name(
        pos: position::Pos,
        pkg: Option<PackageKey>,
        name: String,
        imported: PackageKey,
    ) -> LangObj {
        LangObj::new(EntityType::PkgName(imported, false), pos, pkg, name, None)
    }

    pub fn new_const(
        pos: position::Pos,
        pkg: Option<PackageKey>,
        name: String,
        typ: Option<TypeKey>,
        val: constant::Value,
    ) -> LangObj {
        LangObj::new(EntityType::Const(val), pos, pkg, name, typ)
    }

    pub fn new_type_name(
        pos: position::Pos,
        pkg: Option<PackageKey>,
        name: String,
        typ: Option<TypeKey>,
    ) -> LangObj {
        LangObj::new(EntityType::TypeName, pos, pkg, name, typ)
    }

    fn new_var(
        pos: position::Pos,
        pkg: Option<PackageKey>,
        name: String,
        typ: Option<TypeKey>,
    ) -> LangObj {
        LangObj::new(EntityType::Var(false, false, false), pos, pkg, name, typ)
    }

    fn new_param(
        pos: position::Pos,
        pkg: Option<PackageKey>,
        name: String,
        typ: Option<TypeKey>,
    ) -> LangObj {
        LangObj::new(EntityType::Var(false, false, true), pos, pkg, name, typ)
    }

    fn new_field(
        pos: position::Pos,
        pkg: Option<PackageKey>,
        name: String,
        typ: Option<TypeKey>,
        embedded: bool,
    ) -> LangObj {
        LangObj::new(EntityType::Var(embedded, true, false), pos, pkg, name, typ)
    }

    fn new_func(
        pos: position::Pos,
        pkg: Option<PackageKey>,
        name: String,
        typ: Option<TypeKey>,
    ) -> LangObj {
        LangObj::new(EntityType::Func, pos, pkg, name, typ)
    }

    fn new_label(pos: position::Pos, pkg: Option<PackageKey>, name: String) -> LangObj {
        LangObj::new(EntityType::Label(false), pos, pkg, name, None)
    }

    fn new_builtin() -> LangObj {
        unimplemented!()
    }

    fn new_nil() -> LangObj {
        LangObj::new(EntityType::Nil, 0, None, "nil".to_owned(), None)
    }

    pub fn entity_typ(&self) -> &EntityType {
        &self.entity_typ
    }

    pub fn parent(&self) -> &Option<ScopeKey> {
        &self.parent
    }

    pub fn pos(&self) -> &position::Pos {
        &self.pos
    }

    pub fn name(&self) -> &String {
        &self.name
    }

    pub fn typ(&self) -> &Option<TypeKey> {
        &self.typ
    }

    pub fn exported(&self) -> bool {
        ast::is_exported(&self.name)
    }

    pub fn id(&self, objs: &TCObjects) -> Cow<str> {
        let pkg = self.pkg.map(|x| &objs.pkgs[x]);
        get_id(pkg, &self.name)
    }

    pub fn order(&self) -> &u32 {
        &self.order
    }

    pub fn color(&self) -> &ObjColor {
        &self.color
    }

    pub fn set_parent(&mut self, parent: ScopeKey) {
        self.parent = Some(parent)
    }

    pub fn scope_pos(&self) -> &position::Pos {
        &self.scope_pos
    }

    pub fn set_order(&mut self, order: u32) {
        assert!(order > 0);
        self.order = order;
    }

    pub fn set_color(&mut self, color: ObjColor) {
        self.color = color
    }

    pub fn set_scope_pos(&mut self, pos: position::Pos) {
        self.scope_pos = pos
    }

    pub fn same_id(&self, pkg: Option<PackageKey>, name: &str, objs: &TCObjects) -> bool {
        // spec:
        // "Two identifiers are different if they are spelled differently,
        // or if they appear in different packages and are not exported.
        // Otherwise, they are the same."
        if name != self.name {
            false
        } else if self.exported() {
            true
        } else if pkg.is_none() || self.pkg.is_none() {
            pkg == self.pkg
        } else {
            let a = &objs.pkgs[pkg.unwrap()];
            let b = &objs.pkgs[self.pkg.unwrap()];
            a.path() == b.path()
        }
    }

    pub fn pkg_name_imported(&self) -> &PackageKey {
        match &self.entity_typ {
            EntityType::PkgName(imported, _) => imported,
            _ => unreachable!(),
        }
    }

    pub fn const_val(&self) -> &constant::Value {
        match &self.entity_typ {
            EntityType::Const(val) => val,
            _ => unreachable!(),
        }
    }

    pub fn type_name_is_alias(&self) -> bool {
        unimplemented!()
    }

    pub fn var_embedded(&self) -> &bool {
        match &self.entity_typ {
            EntityType::Var(embedded, _, _) => embedded,
            _ => unreachable!(),
        }
    }

    pub fn var_is_field(&self) -> &bool {
        match &self.entity_typ {
            EntityType::Var(_, field, _) => field,
            _ => unreachable!(),
        }
    }

    pub fn func_full_name(&self) -> &String {
        unimplemented!()
    }

    pub fn func_scope(&self) -> &ScopeKey {
        unimplemented!()
    }

    fn new(
        entity_typ: EntityType,
        pos: position::Pos,
        pkg: Option<PackageKey>,
        name: String,
        typ: Option<TypeKey>,
    ) -> LangObj {
        LangObj {
            entity_typ: entity_typ,
            parent: None,
            pos: pos,
            pkg: pkg,
            name: name,
            typ: typ,
            order: 0,
            color: color_for_typ(typ),
            scope_pos: 0,
        }
    }
}

impl fmt::Display for LangObj {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        unimplemented!()
    }
}

// ----------------------------------------------------------------------------
// utilities

pub fn get_id<'a>(pkg: Option<&Package>, name: &'a str) -> Cow<'a, str> {
    if ast::is_exported(name) {
        return Cow::Borrowed(name);
    }
    let path = if let Some(p) = pkg {
        if !p.path().is_empty() {
            p.path()
        } else {
            "_"
        }
    } else {
        "_"
    };
    Cow::Owned(format!("{}.{}", path, name))
}

fn color_for_typ(typ: Option<TypeKey>) -> ObjColor {
    match typ {
        Some(_) => ObjColor::Black,
        None => ObjColor::White,
    }
}
