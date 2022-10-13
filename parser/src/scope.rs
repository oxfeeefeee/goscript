// Copyright 2022 The Goscript Authors. All rights reserved.
// Use of this source code is governed by a BSD-style
// license that can be found in the LICENSE file.
//
//
// This code is adapted from the offical Go code written in Go
// with license as follows:
// Copyright 2013 The Go Authors. All rights reserved.
// Use of this source code is governed by a BSD-style
// license that can be found in the LICENSE file.

use super::ast::Node;
use super::map::Map;
use super::objects::*;
use super::position;
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
            kind,
            name,
            decl,
            data,
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
    pub entities: Map<String, EntityKey>,
}

impl Scope {
    pub fn new(outer: Option<ScopeKey>) -> Scope {
        Scope {
            outer: outer,
            entities: Map::new(),
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
