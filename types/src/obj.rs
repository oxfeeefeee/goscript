#![allow(dead_code)]
use super::constant;
use super::objects::{ObjKey, PackageKey, ScopeKey, TCObjects, TypeKey};
use super::package::Package;
use super::typ;
use super::universe;
use super::universe::Universe;
use goscript_parser::ast;
use goscript_parser::position;
use std::borrow::Cow;
use std::collections::HashMap;
use std::fmt;
use std::fmt::Write;

/// EntityType defines the types of LangObj entities
///
#[derive(Clone, Debug)]
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
    Builtin(universe::Builtin),
    /// Nil represents the predeclared value nil.
    Nil,
}

impl EntityType {
    pub fn is_pkg_name(&self) -> bool {
        match self {
            EntityType::PkgName(_, _) => true,
            _ => false,
        }
    }

    pub fn is_const(&self) -> bool {
        match self {
            EntityType::Const(_) => true,
            _ => false,
        }
    }

    pub fn is_type_name(&self) -> bool {
        match self {
            EntityType::TypeName => true,
            _ => false,
        }
    }

    pub fn is_var(&self) -> bool {
        match self {
            EntityType::Var(_, _, _) => true,
            _ => false,
        }
    }

    pub fn is_func(&self) -> bool {
        match self {
            EntityType::Func => true,
            _ => false,
        }
    }

    pub fn is_label(&self) -> bool {
        match self {
            EntityType::Label(_) => true,
            _ => false,
        }
    }

    pub fn is_builtin(&self) -> bool {
        match self {
            EntityType::Builtin(_) => true,
            _ => false,
        }
    }

    pub fn is_nil(&self) -> bool {
        match self {
            EntityType::Nil => true,
            _ => false,
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum ObjColor {
    White,
    Black,
    Gray,
}

// ----------------------------------------------------------------------------
// LangObj
//
/// A LangObj describes a named language entity such as a package,
/// constant, type, variable, function (incl. methods), or label.
///
#[derive(Clone, Debug)]
pub struct LangObj {
    entity_type: EntityType,
    parent: Option<ScopeKey>,
    pos: position::Pos,
    pkg: Option<PackageKey>,
    name: String,
    typ: Option<TypeKey>,
    order: u32,
    color: ObjColor,
    scope_pos: position::Pos,
}

impl LangObj {
    pub fn new_pkg_name(
        pos: position::Pos,
        pkg: Option<PackageKey>,
        name: String,
        imported: PackageKey,
        univ: &Universe,
    ) -> LangObj {
        let t = univ.types()[&typ::BasicType::Invalid];
        LangObj::new(
            EntityType::PkgName(imported, false),
            pos,
            pkg,
            name,
            Some(t),
        )
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

    pub fn new_var(
        pos: position::Pos,
        pkg: Option<PackageKey>,
        name: String,
        typ: Option<TypeKey>,
    ) -> LangObj {
        LangObj::new(EntityType::Var(false, false, false), pos, pkg, name, typ)
    }

    pub fn new_param(
        pos: position::Pos,
        pkg: Option<PackageKey>,
        name: String,
        typ: Option<TypeKey>,
    ) -> LangObj {
        LangObj::new(EntityType::Var(false, false, true), pos, pkg, name, typ)
    }

    pub fn new_field(
        pos: position::Pos,
        pkg: Option<PackageKey>,
        name: String,
        typ: Option<TypeKey>,
        embedded: bool,
    ) -> LangObj {
        LangObj::new(EntityType::Var(embedded, true, false), pos, pkg, name, typ)
    }

    pub fn new_func(
        pos: position::Pos,
        pkg: Option<PackageKey>,
        name: String,
        typ: Option<TypeKey>,
    ) -> LangObj {
        LangObj::new(EntityType::Func, pos, pkg, name, typ)
    }

    fn new_label(
        pos: position::Pos,
        pkg: Option<PackageKey>,
        name: String,
        univ: &Universe,
    ) -> LangObj {
        let t = univ.types()[&typ::BasicType::Invalid];
        LangObj::new(EntityType::Label(false), pos, pkg, name, Some(t))
    }

    pub fn new_builtin(f: universe::Builtin, name: String, typ: TypeKey) -> LangObj {
        LangObj::new(EntityType::Builtin(f), 0, None, name, Some(typ))
    }

    pub fn new_nil(typ: TypeKey) -> LangObj {
        LangObj::new(EntityType::Nil, 0, None, "nil".to_owned(), Some(typ))
    }

    pub fn entity_type(&self) -> &EntityType {
        &self.entity_type
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

    pub fn pkg(&self) -> &Option<PackageKey> {
        &self.pkg
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

    pub fn set_type(&mut self, typ: Option<TypeKey>) {
        self.typ = typ
    }

    pub fn set_pkg(&mut self, pkg: Option<PackageKey>) {
        self.pkg = pkg;
    }

    pub fn set_parent(&mut self, parent: Option<ScopeKey>) {
        self.parent = parent
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

    pub fn same_id(&self, pkg: &Option<PackageKey>, name: &str, objs: &TCObjects) -> bool {
        // spec:
        // "Two identifiers are different if they are spelled differently,
        // or if they appear in different packages and are not exported.
        // Otherwise, they are the same."
        if name != self.name {
            false
        } else if self.exported() {
            true
        } else if pkg.is_none() || self.pkg.is_none() {
            pkg == &self.pkg
        } else {
            let a = &objs.pkgs[pkg.unwrap()];
            let b = &objs.pkgs[self.pkg.unwrap()];
            a.path() == b.path()
        }
    }

    pub fn pkg_name_imported(&self) -> &PackageKey {
        match &self.entity_type {
            EntityType::PkgName(imported, _) => imported,
            _ => unreachable!(),
        }
    }

    pub fn const_val(&self) -> &constant::Value {
        match &self.entity_type {
            EntityType::Const(val) => val,
            _ => unreachable!(),
        }
    }

    pub fn type_name_is_alias(&self) -> bool {
        unimplemented!()
    }

    pub fn var_embedded(&self) -> &bool {
        match &self.entity_type {
            EntityType::Var(embedded, _, _) => embedded,
            _ => unreachable!(),
        }
    }

    pub fn var_is_field(&self) -> &bool {
        match &self.entity_type {
            EntityType::Var(_, field, _) => field,
            _ => unreachable!(),
        }
    }

    pub fn func_fmt_name(&self, f: &mut fmt::Formatter<'_>, objs: &TCObjects) -> fmt::Result {
        match &self.entity_type {
            EntityType::Func => fmt_func_name(self, f, objs),
            _ => unreachable!(),
        }
    }

    pub fn func_scope(&self) -> &ScopeKey {
        unimplemented!()
    }

    fn new(
        entity_type: EntityType,
        pos: position::Pos,
        pkg: Option<PackageKey>,
        name: String,
        typ: Option<TypeKey>,
    ) -> LangObj {
        LangObj {
            entity_type: entity_type,
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
    fn fmt(&self, _f: &mut fmt::Formatter<'_>) -> fmt::Result {
        unimplemented!()
    }
}

// ----------------------------------------------------------------------------
// ObjSet
//
/// An ObjSet is a set of objects identified by their unique id.
pub struct ObjSet(HashMap<String, ObjKey>);

impl ObjSet {
    pub fn new() -> ObjSet {
        ObjSet(HashMap::new())
    }

    pub fn insert(&self, okey: ObjKey, objs: &TCObjects) -> Option<&ObjKey> {
        let obj = &objs.lobjs[okey];
        let id = obj.id(objs);
        self.0.get(id.as_ref())
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

pub fn fmt_obj(okey: &ObjKey, f: &mut fmt::Formatter<'_>, objs: &TCObjects) -> fmt::Result {
    let obj = &objs.lobjs[*okey];
    match obj.entity_type() {
        EntityType::PkgName(imported, _) => {
            write!(f, "package {}", obj.name())?;
            let path = objs.pkgs[*imported].path();
            if path != obj.name() {
                write!(f, " ('{}')", path)?;
            }
        }
        EntityType::Const(_) => {
            f.write_str("const")?;
            fmt_obj_name(okey, f, objs)?;
            fmt_obj_type(obj, f, objs)?;
        }
        EntityType::TypeName => {
            f.write_str("const")?;
            fmt_obj_name(okey, f, objs)?;
            fmt_obj_type(obj, f, objs)?;
        }
        EntityType::Var(_, field, _) => {
            f.write_str(if *field { "field" } else { "var" })?;
            fmt_obj_name(okey, f, objs)?;
            fmt_obj_type(obj, f, objs)?;
        }
        EntityType::Func => {
            f.write_str("func ")?;
            fmt_func_name(obj, f, objs)?;
            if let Some(t) = obj.typ() {
                typ::fmt_signature(t, f, objs)?;
            }
        }
        EntityType::Label(_) => {
            f.write_str("label")?;
            fmt_obj_name(okey, f, objs)?;
        }
        EntityType::Builtin(_) => {
            f.write_str("builtin")?;
            fmt_obj_name(okey, f, objs)?;
        }
        EntityType::Nil => f.write_str("nil")?,
    }
    Ok(())
}

fn fmt_obj_name(okey: &ObjKey, f: &mut fmt::Formatter<'_>, objs: &TCObjects) -> fmt::Result {
    f.write_char(' ')?;
    let obj = &objs.lobjs[*okey];
    if let Some(p) = obj.pkg {
        let pkg_val = &objs.pkgs[p];
        if let Some(k) = objs.scopes[*pkg_val.scope()].lookup(obj.name()) {
            if k == okey {
                pkg_val.fmt_with_qualifier(f, objs.fmt_qualifier.as_ref())?;
            }
        }
    }
    f.write_str(obj.name())
}

fn fmt_obj_type(obj: &LangObj, f: &mut fmt::Formatter<'_>, objs: &TCObjects) -> fmt::Result {
    if obj.typ().is_none() {
        return Ok(());
    }
    let mut obj_typ = obj.typ().unwrap();
    if obj.entity_type().is_type_name() {
        let typ_val = &objs.types[obj.typ().unwrap()];
        if typ_val.try_as_basic().is_some() {
            return Ok(());
        }
        if obj.type_name_is_alias() {
            f.write_str(" =")?;
        } else {
            obj_typ = *typ::underlying_type(&obj_typ, objs);
        }
    }
    f.write_char(' ')?;
    typ::fmt_type(&Some(obj_typ), f, objs)
}

fn fmt_func_name(func: &LangObj, f: &mut fmt::Formatter<'_>, objs: &TCObjects) -> fmt::Result {
    if let Some(t) = func.typ() {
        let sig = objs.types[*t].try_as_signature().unwrap();
        if let Some(r) = sig.recv() {
            f.write_char('(')?;
            typ::fmt_type(objs.lobjs[*r].typ(), f, objs)?;
            f.write_str(").")?;
        } else {
            if let Some(p) = func.pkg() {
                objs.pkgs[*p].fmt_with_qualifier(f, objs.fmt_qualifier.as_ref())?;
            }
        }
    }
    f.write_str(func.name())
}

fn color_for_typ(typ: Option<TypeKey>) -> ObjColor {
    match typ {
        Some(_) => ObjColor::Black,
        None => ObjColor::White,
    }
}
