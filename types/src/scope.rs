use super::objects::{ObjKey, ScopeKey};
use goscript_parser::position;
use std::collections::HashMap;

/// A Scope maintains a set of objects and links to its containing
/// (parent) and contained (children) scopes. Objects may be inserted
/// and looked up by name.
pub struct Scope {
    parent: ScopeKey,
    children: Vec<ScopeKey>,
    elems: HashMap<String, ObjKey>, // lazily allocated
    pos: position::Pos,             // scope pos; may be invalid
    end: position::Pos,
    comment: String, // for debugging only
    is_func: bool,   // set if this is a function scope (internal use only)
}
