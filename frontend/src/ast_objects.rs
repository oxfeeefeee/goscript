#![macro_use]
#![allow(unused_macros)]
use super::ast;
use super::scope;
use slotmap::{new_key_type, DenseSlotMap};

const DEFAULT_CAPACITY: usize = 16;

new_key_type! { pub struct LabeledStmtIndex; }
new_key_type! { pub struct AssignStmtIndex; }
new_key_type! { pub struct SpecIndex; }
new_key_type! { pub struct FuncDeclIndex; }
new_key_type! { pub struct IdentIndex; }
new_key_type! { pub struct FieldIndex; }
new_key_type! { pub struct EntityIndex; }
new_key_type! { pub struct ScopeIndex; }

pub type LabeledStmtArena = DenseSlotMap<LabeledStmtIndex, ast::LabeledStmt>;
pub type AssignStmtArena = DenseSlotMap<AssignStmtIndex, ast::AssignStmt>;
pub type SpecArena = DenseSlotMap<SpecIndex, ast::Spec>;
pub type FuncDeclArena = DenseSlotMap<FuncDeclIndex, ast::FuncDecl>;
pub type IdentArena = DenseSlotMap<IdentIndex, ast::Ident>;
pub type FieldArena = DenseSlotMap<FieldIndex, ast::Field>;
pub type EntityArena = DenseSlotMap<EntityIndex, scope::Entity>;
pub type ScopeArena = DenseSlotMap<ScopeIndex, scope::Scope>;

pub struct Objects {
    pub l_stmts: LabeledStmtArena,
    pub a_stmts: AssignStmtArena,
    pub specs: SpecArena,
    pub decls: FuncDeclArena,
    pub idents: IdentArena,
    pub fields: FieldArena,
    pub entities: EntityArena,
    pub scopes: ScopeArena,
}

impl Objects {
    pub fn new() -> Objects {
        Objects {
            l_stmts: DenseSlotMap::with_capacity_and_key(DEFAULT_CAPACITY),
            a_stmts: DenseSlotMap::with_capacity_and_key(DEFAULT_CAPACITY),
            specs: DenseSlotMap::with_capacity_and_key(DEFAULT_CAPACITY),
            decls: DenseSlotMap::with_capacity_and_key(DEFAULT_CAPACITY),
            idents: DenseSlotMap::with_capacity_and_key(DEFAULT_CAPACITY),
            fields: DenseSlotMap::with_capacity_and_key(DEFAULT_CAPACITY),
            entities: DenseSlotMap::with_capacity_and_key(DEFAULT_CAPACITY),
            scopes: DenseSlotMap::with_capacity_and_key(DEFAULT_CAPACITY),
        }
    }
}

macro_rules! lab_stmts {
    ($owner:ident) => {
        &$owner.objects.l_stmts
    };
}
macro_rules! lab_stmts_mut {
    ($owner:ident) => {
        &mut $owner.objects.l_stmts
    };
}
macro_rules! lab_stmt {
    ($owner:ident, $idx:expr) => {
        &$owner.objects.l_stmts[$idx]
    };
}
macro_rules! lab_stmt_mut {
    ($owner:ident, $idx:expr) => {
        &mut $owner.objects.l_stmts[$idx]
    };
}

macro_rules! ass_stmts {
    ($owner:ident) => {
        &$owner.objects.a_stmts
    };
}
macro_rules! ass_stmts_mut {
    ($owner:ident) => {
        &mut $owner.objects.a_stmts
    };
}
macro_rules! ass_stmt {
    ($owner:ident, $idx:expr) => {
        &$owner.objects.a_stmts[$idx]
    };
}
macro_rules! ass_stmt_mut {
    ($owner:ident, $idx:expr) => {
        &mut $owner.objects.a_stmts[$idx]
    };
}

macro_rules! specs {
    ($owner:ident) => {
        &$owner.objects.specs
    };
}
macro_rules! specs_mut {
    ($owner:ident) => {
        &mut $owner.objects.specs
    };
}
macro_rules! spec {
    ($owner:ident, $idx:expr) => {
        &$owner.objects.specs[$idx]
    };
}
macro_rules! spec_mut {
    ($owner:ident, $idx:expr) => {
        &mut $owner.objects.specs[$idx]
    };
}

macro_rules! fn_decls {
    ($owner:ident) => {
        &$owner.objects.decls
    };
}
macro_rules! fn_decls_mut {
    ($owner:ident) => {
        &mut $owner.objects.decls
    };
}
macro_rules! fn_decl {
    ($owner:ident, $idx:expr) => {
        &$owner.objects.decls[$idx]
    };
}
macro_rules! fn_decl_mut {
    ($owner:ident, $idx:expr) => {
        &mut $owner.objects.decls[$idx]
    };
}

macro_rules! entities {
    ($owner:ident) => {
        &$owner.objects.entities
    };
}
macro_rules! entities_mut {
    ($owner:ident) => {
        &mut $owner.objects.entities
    };
}
macro_rules! entity {
    ($owner:ident, $idx:expr) => {
        &$owner.objects.entities[$idx]
    };
}
macro_rules! entity_mut {
    ($owner:ident, $idx:expr) => {
        &mut $owner.objects.entities[$idx]
    };
}
macro_rules! new_entity {
    ($owner:ident, $kind:expr, $name:expr, $decl:expr, $data:expr) => {
        $owner
            .objects
            .entities
            .insert(Entity::new($kind, $name, $decl, $data))
    };
}

macro_rules! scopes {
    ($owner:ident) => {
        &$owner.objects.scopes
    };
}
macro_rules! scopes_mut {
    ($owner:ident) => {
        &mut $owner.objects.scopes
    };
}
macro_rules! scope {
    ($owner:ident, $idx:expr) => {
        &$owner.objects.scopes[$idx]
    };
}
macro_rules! scope_mut {
    ($owner:ident, $idx:expr) => {
        &mut $owner.objects.scopes[$idx]
    };
}
macro_rules! new_scope {
    ($owner:ident, $outer:expr) => {
        $owner.objects.scopes.insert(Scope {
            outer: $outer,
            entities: HashMap::new(),
        })
    };
}

macro_rules! idents {
    ($owner:ident) => {
        &$owner.objects.idents
    };
}
macro_rules! idents_mut {
    ($owner:ident) => {
        &mut $owner.objects.idents
    };
}
macro_rules! ident {
    ($owner:ident, $idx:expr) => {
        &$owner.objects.idents[$idx]
    };
}
macro_rules! ident_mut {
    ($owner:ident, $idx:expr) => {
        &mut $owner.objects.idents[$idx]
    };
}
macro_rules! new_ident {
    ($owner:ident, $pos:expr, $name:expr, $entity:expr) => {
        $owner.objects.idents.insert(Ident {
            pos: $pos,
            name: $name,
            entity: $entity,
        })
    };
}

macro_rules! field {
    ($owner:ident, $idx:expr) => {
        &$owner.objects.fields[$idx]
    };
}
macro_rules! new_field {
    ($owner:ident, $names:expr, $typ:expr, $tag:expr) => {
        $owner.objects.fields.insert(Field {
            names: $names,
            typ: $typ,
            tag: $tag,
        })
    };
}
