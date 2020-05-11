#![macro_use]
#![allow(unused_macros)]
use super::ast;
use super::scope;
use slotmap::{new_key_type, DenseSlotMap};

const DEFAULT_CAPACITY: usize = 16;

macro_rules! new_objects {
    () => {
        DenseSlotMap::with_capacity_and_key(DEFAULT_CAPACITY)
    };
}

new_key_type! { pub struct LabeledStmtKey; }
new_key_type! { pub struct AssignStmtKey; }
new_key_type! { pub struct SpecKey; }
new_key_type! { pub struct FuncDeclKey; }
new_key_type! { pub struct IdentKey; }
new_key_type! { pub struct FieldKey; }
new_key_type! { pub struct EntityKey; }
new_key_type! { pub struct ScopeKey; }

pub type LabeledStmts = DenseSlotMap<LabeledStmtKey, ast::LabeledStmt>;
pub type AssignStmts = DenseSlotMap<AssignStmtKey, ast::AssignStmt>;
pub type Specs = DenseSlotMap<SpecKey, ast::Spec>;
pub type FuncDecls = DenseSlotMap<FuncDeclKey, ast::FuncDecl>;
pub type Idents = DenseSlotMap<IdentKey, ast::Ident>;
pub type Fields = DenseSlotMap<FieldKey, ast::Field>;
pub type Entitys = DenseSlotMap<EntityKey, scope::Entity>;
pub type Scopes = DenseSlotMap<ScopeKey, scope::Scope>;

pub struct Objects {
    pub l_stmts: LabeledStmts,
    pub a_stmts: AssignStmts,
    pub specs: Specs,
    pub decls: FuncDecls,
    pub idents: Idents,
    pub fields: Fields,
    pub entities: Entitys,
    pub scopes: Scopes,
}

impl Objects {
    pub fn new() -> Objects {
        Objects {
            l_stmts: new_objects!(),
            a_stmts: new_objects!(),
            specs: new_objects!(),
            decls: new_objects!(),
            idents: new_objects!(),
            fields: new_objects!(),
            entities: new_objects!(),
            scopes: new_objects!(),
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
