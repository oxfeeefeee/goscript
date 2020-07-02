#![allow(dead_code)]
use super::super::objects::{ObjKey, PackageKey, ScopeKey, TCObjects, TypeKey};
use super::check::Checker;
use goscript_parser::ast::{self};
use goscript_parser::objects::FieldKey;

/// MethodInfo represents an interface method.
/// At least one of src or fun must be non-None.
/// (Methods declared in the current package have a non-None scope
/// and src, and eventually a non-None fun field; imported and pre-
/// declared methods have a None scope and src, and only a non-None
/// fun field.)
pub struct MethodInfo {
    // scope of interface method; or None
    pub scope: Option<ScopeKey>,
    // syntax tree representation of interface method; or None
    pub src: Option<FieldKey>,
    // corresponding fully type-checked method type(LangObj::Func); or None
    pub fun: Option<ObjKey>,
}

/// IfaceInfo describes the method set for an interface.
pub struct IfaceInfo {
    pub explicits: usize,
    pub methods: Vec<MethodInfo>,
}

impl IfaceInfo {
    pub fn new_empty() -> IfaceInfo {
        IfaceInfo {
            explicits: 0,
            methods: vec![],
        }
    }

    pub fn is_empty(&self) -> bool {
        self.methods.is_empty()
    }
}

impl<'a> Checker<'a> {
    /// info_from_type_lit computes the method set for the given interface iface
    /// declared in scope.
    /// If a corresponding type name exists (tname is_some), it is used for
    /// cycle detection and to cache the method set.
    /// The result is the method set, or None if there is a cycle via embedded
    /// interfaces. A is_some result doesn't mean that there were no errors,
    /// but they were either reported (e.g., blank methods), or will be found
    /// (again) when computing the interface's type.
    /// If tname is not None it must be the last element in path.
    pub fn info_from_type_lit(
        &mut self,
        skey: ScopeKey,
        iface: &ast::InterfaceType,
        tname: Option<ObjKey>,
        path: Vec<ObjKey>,
    ) -> Option<IfaceInfo> {
        unimplemented!()
    }
}
