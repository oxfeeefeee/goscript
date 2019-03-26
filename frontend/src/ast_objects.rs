#![macro_use]
#![allow(unused_macros)]
use generational_arena as arena;
use super::ast;
use super::scope;

const DEFAULT_CAPACITY: usize = 16;

pub type LabeledStmtIndex = arena::Index;
pub type AssignStmtIndex = arena::Index;
pub type SpecIndex = arena::Index;
pub type FuncDeclIndex = arena::Index;
pub type IdentIndex = arena::Index;
pub type FieldIndex = arena::Index;
pub type EntityIndex = arena::Index;
pub type ScopeIndex = arena::Index;

pub type LabeledStmtArena = arena::Arena<ast::LabeledStmt>;
pub type AssignStmtArena = arena::Arena<ast::AssignStmt>;
pub type SpecArena = arena::Arena<ast::Spec>;
pub type FuncDeclArena = arena::Arena<ast::FuncDecl>;
pub type IdentArena = arena::Arena<ast::Ident>;
pub type FieldArena = arena::Arena<ast::Field>;
pub type EntityArena = arena::Arena<scope::Entity>;
pub type ScopeArena = arena::Arena<scope::Scope>;

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
        Objects{
            l_stmts: arena::Arena::with_capacity(DEFAULT_CAPACITY),
            a_stmts: arena::Arena::with_capacity(DEFAULT_CAPACITY),
            specs: arena::Arena::with_capacity(DEFAULT_CAPACITY),
            decls: arena::Arena::with_capacity(DEFAULT_CAPACITY),
            idents: arena::Arena::with_capacity(DEFAULT_CAPACITY),
            fields: arena::Arena::with_capacity(DEFAULT_CAPACITY),
            entities: arena::Arena::with_capacity(DEFAULT_CAPACITY),
            scopes: arena::Arena::with_capacity(DEFAULT_CAPACITY),
        }
    }
}

macro_rules! lab_stmts {($self:ident) => {&$self.objects.l_stmts};}
macro_rules! lab_stmts_mut {($self:ident) => {&mut $self.objects.l_stmts};}
macro_rules! lab_stmt {($self:ident, $idx:expr) => {&$self.objects.l_stmts[$idx]};}
macro_rules! lab_stmt_mut {($self:ident, $idx:expr) => {&mut $self.objects.l_stmts[$idx]};}

macro_rules! ass_stmts {($self:ident) => {&$self.objects.a_stmts};}
macro_rules! ass_stmts_mut {($self:ident) => {&mut $self.objects.a_stmts};}
macro_rules! ass_stmt {($self:ident, $idx:expr) => {&$self.objects.a_stmts[$idx]};}
macro_rules! ass_stmt_mut {($self:ident, $idx:expr) => {&mut $self.objects.a_stmts[$idx]};}

macro_rules! specs {($self:ident) => {&$self.objects.specs};}
macro_rules! specs_mut {($self:ident) => {&mut $self.objects.specs};}
macro_rules! spec {($self:ident, $idx:expr) => {&$self.objects.specs[$idx]};}
macro_rules! spec_mut {($self:ident, $idx:expr) => {&mut $self.objects.specs[$idx]};}

macro_rules! fn_decls {($self:ident) => {&$self.objects.decls};}
macro_rules! fn_decls_mut {($self:ident) => {&mut $self.objects.decls};}
macro_rules! fn_decl {($self:ident, $idx:expr) => {&$self.objects.decls[$idx]};}
macro_rules! fn_decl_mut {($self:ident, $idx:expr) => {&mut $self.objects.decls[$idx]};}

macro_rules! entities {($self:ident) => {&$self.objects.entities};}
macro_rules! entities_mut {($self:ident) => {&mut $self.objects.entities};}
macro_rules! entity {($self:ident, $idx:expr) => {&$self.objects.entities[$idx]};}
macro_rules! entity_mut {($self:ident, $idx:expr) => {&mut $self.objects.entities[$idx]};}
macro_rules! new_entity {
    ($self:ident, $kind:expr, $name:expr, $decl:expr, $data:expr) => {
        $self.objects.entities.insert(
            Entity::new($kind, $name, $decl, $data)
        )
    };
}

macro_rules! scopes {($self:ident) => {&$self.objects.scopes};}
macro_rules! scopes_mut {($self:ident) => {&mut $self.objects.scopes};}
macro_rules! scope {($self:ident, $idx:expr) => {&$self.objects.scopes[$idx]};}
macro_rules! scope_mut {($self:ident, $idx:expr) => {&mut $self.objects.scopes[$idx]};}
macro_rules! new_scope {
    ($self:ident, $outer:expr) => {
        $self.objects.scopes.insert(Scope{outer: $outer, entities: HashMap::new()})
    };
}   

macro_rules! idents {($self:ident) => {&$self.objects.idents};}
macro_rules! idents_mut {($self:ident) => {&mut $self.objects.idents};}
macro_rules! ident {($self:ident, $idx:expr) => {&$self.objects.idents[$idx]};}
macro_rules! ident_mut {($self:ident, $idx:expr) => {&mut $self.objects.idents[$idx]};}
macro_rules! new_ident {
    ($self:ident, $pos:expr, $name:expr, $entity:expr) => {
        $self.objects.idents.insert(Ident{ pos: $pos, name: $name,
            entity: $entity})
    };
}

macro_rules! field {($self:ident, $idx:expr) => {&$self.objects.fields[$idx]};}
macro_rules! new_field {
    ($self:ident, $names:expr, $typ:expr, $tag:expr) => {
        $self.objects.fields.insert(Field{ names: $names,
            typ: $typ,
            tag: $tag}) 
    };
}
