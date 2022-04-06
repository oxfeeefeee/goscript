use super::ast::Node;
use super::objects::*;
use super::position;
use std::collections::HashMap;
use std::fmt;

#[derive(Debug, Clone)]
pub enum EntityKind {
    Bad,
    Pkg,
    Con,
    Typ,
    Var,
    Fun,
    Lbl,
}

impl EntityKind {
    pub fn kind_text(&self) -> &str {
        match self {
            EntityKind::Bad => "bad",
            EntityKind::Pkg => "package",
            EntityKind::Con => "const",
            EntityKind::Typ => "type",
            EntityKind::Var => "var",
            EntityKind::Fun => "func",
            EntityKind::Lbl => "label",
        }
    }
}

#[derive(Debug, Clone)]
pub enum DeclObj {
    Field(FieldKey),
    Spec(SpecKey),
    FuncDecl(FuncDeclKey),
    LabeledStmt(LabeledStmtKey),
    AssignStmt(AssignStmtKey),
    NoDecl,
}

#[derive(Debug, Clone)]
pub enum EntityData {
    PkgScope(ScopeKey),
    ConIota(isize),
    NoData,
}

// An Entity describes a named language entity such as a package,
// constant, type, variable, function (incl. methods), or label.
#[derive(Debug, Clone)]
pub struct Entity {
    pub kind: EntityKind,
    pub name: String,
    pub decl: DeclObj,
    pub data: EntityData,
}

impl Entity {
    pub fn new(kind: EntityKind, name: String, decl: DeclObj, data: EntityData) -> Entity {
        Entity {
            kind: kind,
            name: name,
            decl: decl,
            data: data,
        }
    }

    pub fn with_no_data(kind: EntityKind, name: String, decl: DeclObj) -> Entity {
        Entity::new(kind, name, decl, EntityData::NoData)
    }

    pub fn pos(&self, objs: &Objects) -> position::Pos {
        match &self.decl {
            DeclObj::Field(i) => i.pos(objs),
            DeclObj::Spec(i) => objs.specs[*i].pos(objs),
            DeclObj::FuncDecl(i) => objs.fdecls[*i].pos(objs),
            DeclObj::LabeledStmt(i) => objs.l_stmts[*i].pos(objs),
            DeclObj::AssignStmt(i) => objs.a_stmts[*i].pos(objs),
            DeclObj::NoDecl => 0,
        }
    }
}

pub struct Scope {
    pub outer: Option<ScopeKey>,
    pub entities: HashMap<String, EntityKey>,
}

impl Scope {
    pub fn new(outer: Option<ScopeKey>) -> Scope {
        Scope {
            outer: outer,
            entities: HashMap::new(),
        }
    }

    pub fn look_up(&self, name: &String) -> Option<&EntityKey> {
        self.entities.get(name)
    }

    pub fn insert(&mut self, name: String, entity: EntityKey) -> Option<EntityKey> {
        self.entities.insert(name, entity)
    }

    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match write!(f, "scope {:p} {{\n", self) {
            Err(e) => {
                return Err(e);
            }
            Ok(_) => {}
        };
        for (k, _) in self.entities.iter() {
            match write!(f, "\t{}\n", k) {
                Err(e) => {
                    return Err(e);
                }
                Ok(_) => {}
            }
        }
        write!(f, "}}\n")
    }
}

#[cfg(test)]
mod test {}
