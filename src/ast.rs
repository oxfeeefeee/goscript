use std::collections::HashMap;
use super::position;
use super::token;
use super::ast_objects::*;

pub trait Node {
    fn pos(&self, arena: &Objects) -> position::Pos;
    fn end(&self, arena: &Objects) -> position::Pos;
}

pub enum Expr {
    Bad(Box<BadExpr>),
    Ident(Box<IdentIndex>),
    Ellipsis(Box<Ellipsis>),
    BasicLit(Box<BasicLit>),
    FuncLit(Box<FuncLit>),
    CompositeLit(Box<CompositeLit>), 
    Paren(Box<ParenExpr>), 
    Selector(Box<SelectorExpr>), 
    Index(Box<IndexExpr>), 
    Slice(Box<SliceExpr>), 
    TypeAssert(Box<TypeAssertExpr>), 
    Call(Box<CallExpr>), 
    Star(Box<StarExpr>), 
    Unary(Box<UnaryExpr>), 
    Binary(Box<BinaryExpr>), 
    KeyValue(Box<KeyValueExpr>), 
    Array(Box<ArrayType>), 
    Struct(Box<StructType>), 
    Func(Box<FuncType>), 
    Interface(Box<InterfaceType>), 
    Map(Box<MapType>), 
    Chan(Box<ChanType>), 
}

pub enum Stmt {
    Bad(Box<BadStmt>),
    Decl(Box<DeclIndex>),
    Empty(Box<EmptyStmt>),
    Labeled(Box<LabeledStmt>),
    Expr(Box<Expr>),
    Send(Box<SendStmt>),
    IncDec(Box<IncDecStmt>),
    Assign(Box<AssignStmt>),
    Go(Box<GoStmt>),
    Defer(Box<DeferStmt>),
    Return(Box<ReturnStmt>),
    Branch(Box<BranchStmt>),
    Block(Box<BlockStmt>),
    If(Box<IfStmt>),
    Case(Box<CaseClause>),
    Switch(Box<SwitchStmt>),
    TypeSwitch(Box<TypeSwitchStmt>),
    Comm(Box<CommClause>),
    Select(Box<SelectStmt>),
    For(Box<ForStmt>),
    Range(Box<RangeStmt>),
}

pub enum Spec {
    Import(Box<ImportSpec>),
    Value(Box<ValueSpec>),
    Type(Box<TypeSpec>),
}

pub enum Decl {
    Bad(Box<BadDecl>),
    Gen(Box<GenDecl>),
    Func(Box<FuncDecl>),
}

pub struct File {
    package: position::Pos,
    name: IdentIndex,
    decls: Vec<DeclIndex>,
    scope: ScopeIndex,
    imports: Vec<SpecIndex>, //ImportSpec
    unresolved: Vec<IdentIndex>,
}

pub struct Package {
    name: String,
    scope: ScopeIndex,
    imports: HashMap<String, EntityIndex>,
    files: HashMap<String, Box<File>>,
}

impl Expr {
    pub fn new_bad(from: position::Pos, to: position::Pos) -> Expr {
        Expr::Bad(Box::new(BadExpr{from:from, to:to}))
    }

    pub fn new_selector(x: Expr, sel: IdentIndex) -> Expr {
        Expr::Selector(Box::new(SelectorExpr{
            expr: x, sel: sel}))
    }

    pub fn new_ellipsis(pos: position::Pos, x :Option<Expr>) -> Expr {
        Expr::Ellipsis(Box::new(Ellipsis{
            pos: pos, elt: x}))
    }

    pub fn new_basic_lit(pos: position::Pos, kind: token::Token, 
        value: String) -> Expr {
        Expr::BasicLit(Box::new(BasicLit{
            pos: pos, kind: kind, value: value
        }))
    }

    pub fn new_func_type(func: Option<position::Pos>, params: FieldList,
        results: Option<FieldList>) -> Expr {
        Expr::Func(Box::new(FuncType{func: func, params: params, results: results}))
    }

    pub fn clone_ident(&self) -> Expr {
        if let Expr::Ident(i) = self 
            {Expr::Ident(i.clone())} else {panic!("unreachable")}
    }
}

impl Node for Expr {
    fn pos(&self, arena: &Objects) -> position::Pos {
        match &self {
            Expr::Bad(e) => e.from,
            Expr::Ident(e) => arena.idents[*e.as_ref()].pos,
            Expr::Ellipsis(e) => e.pos,
            Expr::BasicLit(e) => e.pos,
            Expr::FuncLit(e) => match e.typ.func {
                Some(p) => p,
                None => e.typ.params.pos(arena),
            }, 
            Expr::CompositeLit(e) => match &e.typ {
                Some(expr) => expr.pos(arena),
                None => e.l_brace,
            }, 
            Expr::Paren(e) => e.l_paren, 
            Expr::Selector(e) => e.expr.pos(arena), 
            Expr::Index(e) => e.expr.pos(arena), 
            Expr::Slice(e) => e.expr.pos(arena), 
            Expr::TypeAssert(e) => e.expr.pos(arena), 
            Expr::Call(e) => e.func.pos(arena), 
            Expr::Star(e) => e.star, 
            Expr::Unary(e) => e.op_pos, 
            Expr::Binary(e) => e.expr_a.pos(arena),
            Expr::KeyValue(e) => e.key.pos(arena), 
            Expr::Array(e) => e.l_brack, 
            Expr::Struct(e) => e.struct_pos, 
            Expr::Func(e) => e.pos(arena),
            Expr::Interface(e) => e.interface, 
            Expr::Map(e) => e.map, 
            Expr::Chan(e) => e.begin, 
        }
    }

    fn end(&self, arena: &Objects) -> position::Pos {
        match &self {
            Expr::Bad(e) => e.to,
            Expr::Ident(e) => arena.idents[*e.as_ref()].end(),
            Expr::Ellipsis(e) => match &e.elt {
                Some(expr) => expr.end(arena),
                None => e.pos + 3,
            },
            Expr::BasicLit(e) => e.pos + e.value.len(),
            Expr::FuncLit(e) => e.body.end(),
            Expr::CompositeLit(e) => e.r_brace + 1,
            Expr::Paren(e) => e.r_paren + 1, 
            Expr::Selector(e) => arena.idents[e.sel].end(), 
            Expr::Index(e) => e.r_brack + 1, 
            Expr::Slice(e) => e.r_brack + 1, 
            Expr::TypeAssert(e) => e.r_paren + 1, 
            Expr::Call(e) => e.r_paren + 1, 
            Expr::Star(e) => e.expr.end(arena), 
            Expr::Unary(e) => e.expr.end(arena), 
            Expr::Binary(e) => e.expr_b.end(arena),
            Expr::KeyValue(e) => e.val.end(arena), 
            Expr::Array(e) => e.elt.end(arena), 
            Expr::Struct(e) => e.fields.end(arena), 
            Expr::Func(e) => e.end(arena),
            Expr::Interface(e) => e.methods.end(arena), 
            Expr::Map(e) => e.val.end(arena), 
            Expr::Chan(e) => e.val.end(arena), 
        }
    } 
}

impl Node for Stmt {
    fn pos(&self, arena: &Objects) -> position::Pos {
        match &self {
            Stmt::Bad(s) => s.from,
            Stmt::Decl(d) => arena.decls[*d.as_ref()].pos(arena),
            Stmt::Empty(s) => s.semi,
            Stmt::Labeled(s) => arena.idents[s.label].pos,
            Stmt::Expr(e) => e.pos(arena),
            Stmt::Send(s) => s.chan.pos(arena),
            Stmt::IncDec(s) => s.expr.pos(arena),
            Stmt::Assign(s) => s.lhs[0].pos(arena),
            Stmt::Go(s) => s.go,
            Stmt::Defer(s) => s.defer,
            Stmt::Return(s) => s.ret,
            Stmt::Branch(s) => s.token_pos,
            Stmt::Block(s) => s.l_brace,
            Stmt::If(s) => s.if_pos,
            Stmt::Case(s) => s.case,
            Stmt::Switch(s) => s.switch,
            Stmt::TypeSwitch(s) => s.switch,
            Stmt::Comm(s) => s.case,
            Stmt::Select(s) => s.select,
            Stmt::For(s) => s.for_pos,
            Stmt::Range(s) => s.for_pos,
        }
    }
    fn end(&self, arena: &Objects) -> position::Pos {
        match &self {
            Stmt::Bad(s) => s.to,
            Stmt::Decl(d) =>  arena.decls[*d.as_ref()].end(arena),
            Stmt::Empty(s) => if s.implicit { s.semi }
                else {s.semi + 1}, 
            Stmt::Labeled(s) => arena.stmts[s.stmt].end(arena),
            Stmt::Expr(e) => e.end(arena),
            Stmt::Send(s) => s.val.end(arena),
            Stmt::IncDec(s) => s.token_pos + 2,
            Stmt::Assign(s) => s.rhs[s.rhs.len()-1].end(arena),
            Stmt::Go(s) => s.call.end(arena),
            Stmt::Defer(s) => s.call.end(arena),
            Stmt::Return(s) => {
                let n = s.results.len();
                if n > 0 {
                    s.results[n-1].end(arena)
                } else {
                    s.ret + 6
                }
            },
            Stmt::Branch(s) => match &s.label {
                Some(l) => arena.idents[*l].end(),
                None => s.token_pos + s.token.token_text().len()
            },
            Stmt::Block(s) => s.r_brace + 1,
            Stmt::If(s) => match &s.els {
                Some(e) => arena.stmts[*e].end(arena),
                None => s.body.end(),
            },
            Stmt::Case(s) => {
                let n = s.body.len();
                if n > 0 {
                    arena.stmts[s.body[n-1]].end(arena)
                } else {
                    s.colon + 1
                }
            },
            Stmt::Switch(s) => s.body.end(),
            Stmt::TypeSwitch(s) => s.body.end(),
            Stmt::Comm(s) => {
                let n = s.body.len();
                if n > 0 {
                    arena.stmts[s.body[n-1]].end(arena)
                } else {
                    s.colon + 1
                }
            },
            Stmt::Select(s) => s.body.end(),
            Stmt::For(s) => s.body.end(),
            Stmt::Range(s) => s.body.end(),
        }
    }
}

impl Node for Spec {
    fn pos(&self, arena: &Objects) -> position::Pos {
        match &self {
            Spec::Import(s) => match &s.name {
                Some(i) => arena.idents[*i].pos,
                None => s.path.pos,
            }
            Spec::Value(s) => arena.idents[s.names[0]].pos,
            Spec::Type(s) => arena.idents[s.name].pos,
        }
    }
    fn end(&self, arena: &Objects) -> position::Pos {
        match &self {
            Spec::Import(s) => match s.end_pos {
                Some(p) => p,
                None => s.path.pos,
            },
            Spec::Value(s) => {
                let n = s.values.len();
                if n > 0 {
                    s.values[n-1].end(arena)
                } else {
                    match &s.typ {
                        Some(t) => t.end(arena),
                        None => {
                            arena.idents[s.names[s.names.len()-1]].end()
                        },
                    }
                }
            },
            Spec::Type(t) => t.typ.end(arena)
        }
    }
}

impl Node for Decl {
    fn pos(&self, arena: &Objects) -> position::Pos {
        match &self {
            Decl::Bad(d) => d.from,
            Decl::Gen(d) => d.token_pos,
            Decl::Func(d) => d.typ.pos(arena),
        }
    }
    fn end(&self, arena: &Objects) -> position::Pos {
        match &self {
            Decl::Bad(d) => d.to,
            Decl::Gen(d) => match &d.r_paren {
                Some(p) => p + 1,
                None => arena.specs[d.specs[0]].end(arena)
            },
            Decl::Func(d) => match &d.body {
                Some(b) => b.end(),
                None => d.typ.end(arena),
            }
        }
    }
}

impl Node for File {
     fn pos(&self, _arena: &Objects) -> position::Pos {
       self.package
    }
    fn end(&self, arena: &Objects) -> position::Pos {
        let n = self.decls.len();
        if n > 0 {
            arena.decls[self.decls[n-1]].end(arena)
        } else {
            arena.idents[self.name].end()
        }
    }
}

impl Node for Package {
     fn pos(&self, _arena: &Objects) -> position::Pos {
        0
    }
    fn end(&self, _arena: &Objects) -> position::Pos {
        0
    }
}

// A BadExpr node is a placeholder for expressions containing
// syntax errors for which no correct expression nodes can be
// created.
pub struct BadExpr {
    pub from: position::Pos,
    pub to: position::Pos,
}

pub enum IdentEntity {
    NoEntity,
    Sentinel,
    Entity(EntityIndex),
}

impl IdentEntity {
    pub fn is_none(&self) -> bool {
        match self{
            IdentEntity::NoEntity => true,
            _ => false,
        }
    }
}

// An Ident node represents an identifier.
pub struct Ident {
    pub pos: position::Pos,
    pub name: String,
    pub entity: IdentEntity,
}

impl Ident {
    pub fn end(&self) -> position::Pos {
        self.pos + self.name.len()
    }
}

// An Ellipsis node stands for the "..." type in a
// parameter list or the "..." length in an array type.
pub struct Ellipsis {
    pos: position::Pos,
    elt: Option<Expr>,
}

// A BasicLit node represents a literal of basic type.
pub struct BasicLit {
    pos: position::Pos,
    kind: token::Token,
    value: String,
}

// A FuncLit node represents a function literal.
pub struct FuncLit {
    typ: Box<FuncType>,
    body: Box<BlockStmt>,
}	

// A CompositeLit node represents a composite literal.
pub struct CompositeLit {
    typ: Option<Expr>,
    l_brace: position::Pos,
    elts: Vec<Expr>,
    r_brace: position::Pos,
    incomplete: bool,
}

// A ParenExpr node represents a parenthesized expression.
pub struct ParenExpr {
    l_paren: position::Pos,
    expr: Expr,
    r_paren: position::Pos,
}
	
// A SelectorExpr node represents an expression followed by a selector.
pub struct SelectorExpr {
    pub expr: Expr,
    pub sel: IdentIndex,
}

// An IndexExpr node represents an expression followed by an index.
pub struct IndexExpr {
    expr: Expr,
    l_brack: position::Pos,
    index: Expr,
    r_brack: position::Pos,
}

// An SliceExpr node represents an expression followed by slice indices.
pub struct SliceExpr {
    expr: Expr,
    l_brack: position::Pos,
    low: Option<Expr>,
    high: Option<Expr>,
    max: Option<Expr>,
    slice3: bool,
    r_brack: position::Pos,
}

// A TypeAssertExpr node represents an expression followed by a
// type assertion.
pub struct TypeAssertExpr {
    expr: Expr,
    l_paren: position::Pos,
    typ: Expr,
    r_paren: position::Pos,
}

// A CallExpr node represents an expression followed by an argument list.
pub struct CallExpr {
    func: Expr,
    l_paren: position::Pos,
    args: Vec<Expr>,
    ellipsis: position::Pos,
    r_paren: position::Pos, 
}

// A StarExpr node represents an expression of the form "*" Expression.
// Semantically it could be a unary "*" expression, or a pointer type.
pub struct StarExpr {
    pub star: position::Pos,
    pub expr: Expr,
}

// A UnaryExpr node represents a unary expression.
// Unary "*" expressions are represented via StarExpr nodes.
pub struct UnaryExpr {
    op_pos: position::Pos,
    op: token::Token,
    expr: Expr,
}

// A BinaryExpr node represents a binary expression.
pub struct BinaryExpr {
    expr_a: Expr,
    op_pos: position::Pos,
    op: token::Token,
    expr_b: Expr,
}

// A KeyValueExpr node represents (key : value) pairs
// in composite literals.
pub struct KeyValueExpr {
    key: Expr,
    colon: position::Pos,
    val: Expr,
}

// An ArrayType node represents an array or slice type.
pub struct ArrayType {
    pub l_brack: position::Pos,
    pub len: Option<Expr>, // Ellipsis node for [...]T array types, None for slice types
    pub elt: Expr,
}

// A StructType node represents a struct type.
pub struct StructType {
    pub struct_pos: position::Pos,
    pub fields: FieldList,
    pub incomplete: bool,
}

// Pointer types are represented via StarExpr nodes.

// A FuncType node represents a function type.
pub struct FuncType {
    func: Option<position::Pos>,
    params: FieldList,
    results: Option<FieldList>,
}

impl FuncType {
    fn pos(&self, arena: &Objects) -> position::Pos {
        match self.func {
            Some(p) => p,
            None => self.params.pos(arena),
        } 
    }
    fn end(&self, arena: &Objects) -> position::Pos {
        match &self.results {
            Some(r) => (*r).end(arena),
            None => self.params.end(arena),
        }
    }
}

// An InterfaceType node represents an interface type.
pub struct InterfaceType {
    interface: position::Pos,
    methods: FieldList,
    incomplete: bool, 
}

// A MapType node represents a map type.
pub struct MapType {
    map: position::Pos,
    key: Expr,
    val: Expr,
}

// A ChanType node represents a channel type.
pub enum ChanDir {
    SendTo = 1,
    RecvFrom = 2,
}

pub struct ChanType {
    begin: position::Pos,
    arrow: position::Pos,
    dir: ChanDir,
    val: Expr,
}

// An ImportSpec node represents a single package import.
pub struct ImportSpec {
    name: Option<IdentIndex>,
    path: Box<BasicLit>,
    end_pos: Option<position::Pos>,
}

// A ValueSpec node represents a constant or variable declaration
// (ConstSpec or VarSpec production).
pub struct ValueSpec {
    names: Vec<IdentIndex>,
    typ: Option<Expr>,
    values: Vec<Expr>, 
}

// A TypeSpec node represents a type declaration (TypeSpec production).
pub struct TypeSpec {
    name: IdentIndex,
    assign: position::Pos,
    typ: Expr,
}

pub struct BadDecl {
    from: position::Pos,
    to: position::Pos,
}

// A GenDecl node (generic declaration node) represents an import,
// constant, type or variable declaration. A valid Lparen position
// (Lparen.IsValid()) indicates a parenthesized declaration.
//
// Relationship between Tok value and Specs element type:
//
//	token.IMPORT  *ImportSpec
//	token.CONST   *ValueSpec
//	token.TYPE    *TypeSpec
//	token.VAR     *ValueSpec
pub struct GenDecl {
    token_pos: position::Pos,
    token: token::Token,
    l_paran: Option<position::Pos>,
    specs: Vec<SpecIndex>,
    r_paren: Option<position::Pos>,
}

// A FuncDecl node represents a function declaration.
pub struct FuncDecl {
    recv: Option<FieldList>,
    name: IndexExpr,
    typ: Box<FuncType>,
    body: Option<Box<BlockStmt>>,
}

pub struct BadStmt {
    from: position::Pos,
    to: position::Pos,
}

pub struct EmptyStmt {
    semi: position::Pos,
    implicit: bool,
}

// A LabeledStmt node represents a labeled statement.
pub struct LabeledStmt {
    label: IdentIndex,
    colon: position::Pos,
    stmt: StmtIndex,
}

// A SendStmt node represents a send statement.
pub struct SendStmt {
    chan: Expr,
    arrow: position::Pos,
    val: Expr,
}

// An IncDecStmt node represents an increment or decrement statement.
pub struct IncDecStmt {
    expr: Expr,
    token_pos: position::Pos,
    token: token::Token,
}

// An AssignStmt node represents an assignment or
// a short variable declaration.
pub struct AssignStmt {
    lhs: Vec<Expr>,
    token_pos: position::Pos,
    token: token::Token,
    rhs: Vec<Expr>,
}

pub struct GoStmt {
    go: position::Pos,
    call: Expr,
}
	
pub struct DeferStmt {
    defer: position::Pos,
    call: Expr,
}

pub struct ReturnStmt {
    ret: position::Pos,
    results: Vec<Expr>,
}

// A BranchStmt node represents a break, continue, goto,
// or fallthrough statement.
pub struct BranchStmt {
    token_pos: position::Pos,
    token: token::Token,
    label: Option<IdentIndex>,
}

pub struct BlockStmt {
    l_brace: position::Pos,
    list: Vec<StmtIndex>,
    r_brace: position::Pos,
}

impl BlockStmt {
    fn end(&self) -> position::Pos {
        self.l_brace
    }
}

pub struct IfStmt {
    if_pos: position::Pos,
    init: Option<StmtIndex>,
    cond: Expr,
    body: Box<BlockStmt>,
    els: Option<StmtIndex>,
}

// A CaseClause represents a case of an expression or type switch statement.
pub struct CaseClause {
    case: position::Pos,
    list: Vec<Expr>,
    colon: position::Pos, 
    body: Vec<StmtIndex>,
}

pub struct SwitchStmt {
    switch: position::Pos,
    init: Option<StmtIndex>,
    tag: Option<Expr>,
    body: Box<BlockStmt>,
}

pub struct TypeSwitchStmt {
    switch: position::Pos,
    init: Option<StmtIndex>,
    assign: StmtIndex,
    body: Box<BlockStmt>,
}

// A CommClause node represents a case of a select statement.
pub struct CommClause { //communication
    case: position::Pos,
    comm: Option<StmtIndex>,
    colon: position::Pos,
    body: Vec<StmtIndex>,
}

pub struct SelectStmt {
    select: position::Pos,
    body: Box<BlockStmt>,
}

pub struct ForStmt {
    for_pos: position::Pos,
    init: StmtIndex,
    cond: Expr,
    post: StmtIndex,
    body: Box<BlockStmt>,
}

pub struct RangeStmt {
    for_pos: position::Pos,
    key: Option<Expr>,
    val: Option<Expr>,
    token_pos: position::Pos,
    token: token::Token,
    expr: Expr,
    body: Box<BlockStmt>,   
}

pub struct Field {
    pub names: Vec<IdentIndex>,
    pub typ: Expr,
    pub tag: Option<Expr>,
}

impl Node for Field {
    fn pos(&self, arena: &Objects) -> position::Pos {
        if self.names.len() > 0 {
            arena.idents[self.names[0]].pos
        } else {
            self.typ.pos(arena)
        }
    }
    fn end(&self, arena: &Objects) -> position::Pos {
        match &self.tag {
            Some(t) => t.end(arena),
            None => self.typ.end(arena),
        }
    }
}

pub struct FieldList {
    pub openning: Option<position::Pos>,
    pub list: Vec<FieldIndex>,
    pub closing: Option<position::Pos>,
}

impl FieldList {
    pub fn new(openning: Option<position::Pos>, list: Vec<FieldIndex>,
        closing: Option<position::Pos>) -> FieldList {
            FieldList{openning: openning, list: list, closing: closing}
        }
}

impl Node for FieldList {
    fn pos(&self, arena: &Objects) -> position::Pos {
        match self.openning {
            Some(o) => o,
            None => arena.fields[self.list[0]].pos(arena),
        }
    }
    fn end(&self, arena: &Objects) -> position::Pos {
        match self.closing {
            Some(c) => c,
            None => arena.fields[self.list[self.list.len()-1]].pos(arena),
        }
    }
}


#[cfg(test)]
mod test {
	//use super::*;

	#[test]
    fn ast_test () {
		print!("testxxxxx . ");
	}
} 