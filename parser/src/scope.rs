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

    pub fn pos(&self, arena: &Objects) -> position::Pos {
        match &self.decl {
            DeclObj::Field(i) => i.pos(arena),
            DeclObj::Spec(i) => arena.specs[*i].pos(arena),
            DeclObj::FuncDecl(i) => arena.fdecls[*i].pos(arena),
            DeclObj::LabeledStmt(i) => arena.l_stmts[*i].pos(arena),
            DeclObj::AssignStmt(i) => arena.a_stmts[*i].pos(arena),
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
mod test {
    //use super::*;
    //use generational_arena as ar;

    /*
    pub struct ScopeDebug<'a> {
        scope: &'a Scope,
        arena: &'a Arena<Entity>,
    }

    impl<'a> ScopeDebug<'a> {
        fn new(scope: &'a Scope, arena: &'a Arena<Entity>) -> ScopeDebug<'a> {
            ScopeDebug{scope: scope, arena: arena}
        }
    }

    impl<'a> fmt::Debug for ScopeDebug<'a> {
        fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
            match write!(f, "scope {:p} {{\n", self.scope) {
                Err(e) => { return Err(e); },
                Ok(_) => {},
            };
            for (k, v) in self.scope.entities.iter() {
                let entity = &self.arena[*v];
                match write!(f, "\t{} {}\n", entity.kind.kind_text(), k) {
                    Err(e) => { return Err(e); },
                    Ok(_) => {},
                }
            };
            write!(f, "}}\n")
        }
    }

    pub fn insert_new<'a>(sel: &mut Scope, e: Entity, arena: &'a mut Arena<Entity>) ->
        Option<&'a mut Entity> {
        match sel.entities.get(&e.name) {
            Some(&i) => arena.get_mut(i),
            None => {
                let name = e.name.to_string();
                let index = arena.insert(e);
                sel.entities.insert(name, index);
                None
            }
        }
    }
    */

    #[test]
    fn test_scope() {
        /*
        let mut arena_s = ar::Arena::new();
        let scope = Scope::new(None);
        let s = arena_s.insert(scope);
        let e = Entity::with_no_data(EntityKind::Fun, "test_entity".to_string(), DeclObj::NoDecl);
        let mut arena = ar::Arena::new();
        insert_new(&mut arena_s[s], e, &mut arena);

        println!("scope: {:?}", ScopeDebug::new(&arena_s[s], &arena));
        */
    }
}
