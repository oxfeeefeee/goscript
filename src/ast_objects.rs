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

pub struct Objects {
    pub stmts: arena::Arena<ast::Stmt>,
    pub specs: arena::Arena<ast::Spec>,
    pub decls: arena::Arena<ast::Decl>,
    pub idents: arena::Arena<ast::Ident>,
    pub fields: arena::Arena<ast::Field>,
    pub field_lists: arena::Arena<ast::FieldList>,
    pub entities: arena::Arena<scope::Entity>,
    pub scopes: arena::Arena<scope::Scope>,
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
