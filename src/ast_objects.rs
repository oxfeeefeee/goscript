#![macro_use]
#![allow(unused_macros)]
use generational_arena as arena;
use super::ast;
use super::scope;

const DEFAULT_CAPACITY: usize = 128;

pub type StmtIndex = arena::Index;
pub type SpecIndex = arena::Index;
pub type DeclIndex = arena::Index;
pub type IdentIndex = arena::Index;
pub type FieldIndex = arena::Index;
pub type FieldListIndex = arena::Index;
pub type EntityIndex = arena::Index;
pub type ScopeIndex = arena::Index;

pub type StmtArena = arena::Arena<ast::Stmt>;
pub type SpecArena = arena::Arena<ast::Spec>;
pub type DeclArena = arena::Arena<ast::Decl>;
pub type IdentArena = arena::Arena<ast::Ident>;
pub type FieldArena = arena::Arena<ast::Field>;
pub type FieldListArena = arena::Arena<ast::FieldList>;
pub type EntityArena = arena::Arena<scope::Entity>;
pub type ScopeArena = arena::Arena<scope::Scope>;

pub struct Objects {
    pub stmts: StmtArena,
    pub specs: SpecArena,
    pub decls: DeclArena,
    pub idents: IdentArena,
    pub fields: FieldArena,
    pub field_lists: FieldListArena,
    pub entities: EntityArena,
    pub scopes: ScopeArena,
}

impl Objects {
    pub fn new() -> Objects {
        Objects{
            stmts: arena::Arena::with_capacity(DEFAULT_CAPACITY),
            specs: arena::Arena::with_capacity(DEFAULT_CAPACITY),
            decls: arena::Arena::with_capacity(DEFAULT_CAPACITY),
            idents: arena::Arena::with_capacity(DEFAULT_CAPACITY),
            fields: arena::Arena::with_capacity(DEFAULT_CAPACITY),
            field_lists: arena::Arena::with_capacity(DEFAULT_CAPACITY),
            entities: arena::Arena::with_capacity(DEFAULT_CAPACITY),
            scopes: arena::Arena::with_capacity(DEFAULT_CAPACITY),
        }
    }
}

macro_rules! stmts {($self:ident) => {&$self.objects.stmts};}
macro_rules! stmts_mut {($self:ident) => {&mut $self.objects.stmts};}
macro_rules! stmt {($self:ident, $idx:expr) => {&$self.objects.stmts[$idx]};}
macro_rules! stmt_mut {($self:ident, $idx:expr) => {&mut $self.objects.stmts[$idx]};}

macro_rules! specs {($self:ident) => {&$self.objects.specs};}
macro_rules! specs_mut {($self:ident) => {&mut $self.objects.specs};}
macro_rules! spec {($self:ident, $idx:expr) => {&$self.objects.specs[$idx]};}
macro_rules! spec_mut {($self:ident, $idx:expr) => {&mut $self.objects.specs[$idx]};}

macro_rules! entities {($self:ident) => {&$self.objects.entities};}
macro_rules! entities_mut {($self:ident) => {&mut $self.objects.entities};}
macro_rules! entity {($self:ident, $idx:expr) => {&$self.objects.entities[$idx]};}
macro_rules! entity_mut {($self:ident, $idx:expr) => {&mut $self.objects.entities[$idx]};}

macro_rules! scopes {($self:ident) => {&$self.objects.scopes};}
macro_rules! scopes_mut {($self:ident) => {&mut $self.objects.scopes};}
macro_rules! scope {($self:ident, $idx:expr) => {&$self.objects.scopes[$idx]};}
macro_rules! scope_mut {($self:ident, $idx:expr) => {&mut $self.objects.scopes[$idx]};}

macro_rules! idents {($self:ident) => {&$self.objects.idents};}
macro_rules! idents_mut {($self:ident) => {&mut $self.objects.idents};}
macro_rules! ident {($self:ident, $idx:expr) => {&$self.objects.idents[$idx]};}
macro_rules! ident_mut {($self:ident, $idx:expr) => {&mut $self.objects.idents[$idx]};}
