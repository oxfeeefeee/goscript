#![allow(dead_code)]
#![allow(dead_code)]
use super::super::objects::{ObjKey, PackageKey, ScopeKey, TCObjects, TypeKey};
use goscript_parser::objects::FieldKey;

/// MethodInfo represents an interface method.
/// At least one of src or fun must be non-nil.
/// (Methods declared in the current package have a non-nil scope
/// and src, and eventually a non-nil fun field; imported and pre-
/// declared methods have a nil scope and src, and only a non-nil
/// fun field.)
pub struct MethodInfo {
    // scope of interface method; or None
    scope: Option<ScopeKey>,
    // syntax tree representation of interface method; or None
    src: Option<FieldKey>,
    // corresponding fully type-checked method type(LangObj::Func); or None
    fun: Option<ObjKey>,
}

/// IfaceInfo describes the method set for an interface.
/// The zero value for an ifaceInfo is a ready-to-use ifaceInfo representing
/// the empty interface.
pub struct IfaceInfo {
    explicits: usize,
    methods: Vec<MethodInfo>,
}
