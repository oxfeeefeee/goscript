use std::fmt;
use std::collections::HashMap;
use generational_arena::Arena;
use super::ast_objects::*;
use super::ast::Node;
use super::position;

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

#[derive(Clone)]
pub enum DeclObj {
    Field(FieldIndex),
    Spec(SpecIndex),
    Decl(DeclIndex),
    Stmt(StmtIndex),
    Scope(ScopeIndex),
    NoDecl,
}

#[derive(Clone)]
pub enum EntityData {
    PkgScope(ScopeIndex),
    ConIota(usize),
    NoData,
}

// An Entity describes a named language entity such as a package,
// constant, type, variable, function (incl. methods), or label.
pub struct Entity {
    pub kind: EntityKind,
    pub name: String,
    pub decl: DeclObj,
    pub data: EntityData,
}

impl Entity{
    pub fn new(kind: EntityKind, name: String, decl: DeclObj, data: EntityData) -> Entity {
        Entity{kind: kind, name: name, decl: decl, data: data }
    }

    pub fn with_no_data(kind: EntityKind, name: String, decl: DeclObj) -> Entity {
        Entity::new(kind, name, decl, EntityData::NoData)
    }

    pub fn arena_new(kind: EntityKind, name: String, decl: DeclObj, data: EntityData,
        arena: &mut Arena<Entity>) -> EntityIndex {
            arena.insert(Entity::new(kind, name, decl, data))
    }

    pub fn pos(&self, arena: &Objects) -> position::Pos {
        match &self.decl {
            DeclObj::Field(i) => arena.fields[*i].pos(arena),
            DeclObj::Spec(i) => arena.specs[*i].pos(arena),
            DeclObj::Decl(i) => arena.decls[*i].pos(arena),
            DeclObj::Stmt(i) => arena.stmts[*i].pos(arena),
            DeclObj::Scope(_) => 0,
            NoDecl => 0,
        }
    }
}

pub struct Scope {
    pub outer: Option<ScopeIndex>,
    pub entities: HashMap<String, EntityIndex>,
}

impl Scope {
    pub fn arena_new(outer: Option<ScopeIndex>, arena: &mut Arena<Scope>) -> ScopeIndex {
        arena.insert(Scope{outer: outer, entities: HashMap::new()})
    }

    pub fn look_up<'a>(&self, name: &String, arena: &'a mut Arena<Entity>) ->
        Option<&'a mut Entity> {
        match self.entities.get(name) {
            Some(&i) => arena.get_mut(i),
            None => None,
        }
    }

    pub fn insert(&mut self, name: String, entity: EntityIndex) -> Option<EntityIndex> {
        match self.entities.get(&name) {
            Some(i) => Some(*i),
            None => {
                self.entities.insert(name, entity);
                None
            },
        }
    }

    pub fn insert_new<'a>(&mut self, e: Entity, arena: &'a mut Arena<Entity>) -> 
        Option<&'a mut Entity> {
        match self.entities.get(&e.name) {
            Some(&i) => arena.get_mut(i),
            None => {
                let name = e.name.to_string();
                let index = arena.insert(e);
                self.entities.insert(name, index);
                None
            }
        }
    }

    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match write!(f, "scope {:p} {{\n", self) {
            Err(e) => { return Err(e); },
            Ok(_) => {}, 
        };
        for (k, _) in self.entities.iter() {
            match write!(f, "\t{}\n", k) {
                Err(e) => { return Err(e); },
                Ok(_) => {}, 
            } 
        };
        write!(f, "}}\n")
    }
}

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


#[cfg(test)]
mod test {
	use super::*;
    use generational_arena as ar;

	#[test]
	fn test_scope () {
        let mut arena_s = ar::Arena::new();
        let mut s = Scope::arena_new(None, &mut arena_s);
        let e = Entity::with_no_data(EntityKind::Fun, "test_entity".to_string(), DeclObj::NoDecl);
        let mut arena = ar::Arena::new();
        arena_s[s].insert_new(e, &mut arena);

        println!("scope: {:?}", ScopeDebug::new(&arena_s[s], &arena));

    }
}