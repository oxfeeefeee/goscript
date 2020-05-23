use super::objects::{PackageKey, ScopeKey, TypeKey};
use goscript_parser::position;

/// A LangObj describes a named language entity such as a package,
/// constant, type, variable, function (incl. methods), or label.
///
pub struct LangObj {
    typ: ObjectType,
    data: ObjectData,
}

/// ObjectType defines the types of Object
pub enum ObjectType {
    /// A PkgName represents an imported Go package.
    PkgName(PackageKey, bool),
    /// A Const represents a declared constant.
    Const(bool),
    /// A TypeName represents a name for a (defined or alias) type.
    TypeName,
    /// A Variable represents a declared variable (including function
    /// parameters and results, and struct fields).
    Var(bool, bool, bool),
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

/// ObjectData defines common data for all kinds of Objects
pub struct ObjectData {
    parent: Option<ScopeKey>,
    pos: position::Pos,
    pkg: Option<PackageKey>,
    name: String,
    typ: TypeKey,
    //order
    //color
    scope_pos: position::Pos,
}

// ----------------------------------------------------------------------------
// utilities
