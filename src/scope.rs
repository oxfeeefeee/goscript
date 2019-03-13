use std::fmt;
use std::rc::Rc;
use std::rc::Weak;
use std::cell::RefCell;
use std::collections::HashMap;
use super::ast;

// An Object describes a named language entity such as a package,
// constant, type, variable, function (incl. methods), or label.
pub struct Object {
    kind: ObjKind,
    name: String,
    decl: ObjDecl,
    data: ObjData,
}

pub enum ObjKind {
    Bad,
    Pkg,
    Con,
    Typ,
    Var,
    Fun,
    Lbl,
}

impl ObjKind {
	pub fn kind_text(&self) -> &str {
        match self {
            Bad => "bad",
            Pkg => "package",
            Con => "const",
            Typ => "type",
            Var => "var",
            Fun => "func",
            Lbl => "label", 
        }
    }
}

pub enum ObjDecl {
    Field(Box<ast::Field>),
}

pub enum ObjData {
    PkgScope(Box<Scope>),
    ConIota(usize),
}

pub struct Scope {
    outer: Rc<RefCell<Scope>>,
    objects: HashMap<String, Box<Object>>,
}

