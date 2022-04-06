use slotmap::KeyData;

use super::objects::*;
use super::position;
use super::scope;
use super::token;
use std::collections::HashMap;
use std::hash::Hash;
use std::rc::Rc;

/// NodeId can be used as key of HashMaps
#[derive(PartialEq, Eq, Hash, Clone, Debug)]
pub enum NodeId {
    Address(usize),
    IdentExpr(IdentKey),
    FuncTypeExpr(FuncTypeKey),
    LabeledStmt(LabeledStmtKey),
    AssignStmt(AssignStmtKey),
    FuncDecl(FuncDeclKey),
    FuncType(FuncTypeKey),
    Field(FieldKey),
    File(IdentKey),
}

pub trait Node {
    fn pos(&self, objs: &Objects) -> position::Pos;

    fn end(&self, objs: &Objects) -> position::Pos;

    fn id(&self) -> NodeId;
}

#[derive(Clone, Debug)]
pub enum Expr {
    Bad(Rc<BadExpr>),
    Ident(IdentKey),
    Ellipsis(Rc<Ellipsis>),
    BasicLit(Rc<BasicLit>),
    FuncLit(Rc<FuncLit>),
    CompositeLit(Rc<CompositeLit>),
    Paren(Rc<ParenExpr>),
    Selector(Rc<SelectorExpr>),
    Index(Rc<IndexExpr>),
    Slice(Rc<SliceExpr>),
    TypeAssert(Rc<TypeAssertExpr>),
    Call(Rc<CallExpr>),
    Star(Rc<StarExpr>),
    Unary(Rc<UnaryExpr>),
    Binary(Rc<BinaryExpr>),
    KeyValue(Rc<KeyValueExpr>),
    Array(Rc<ArrayType>),
    Struct(Rc<StructType>),
    Func(FuncTypeKey),
    Interface(Rc<InterfaceType>),
    Map(Rc<MapType>),
    Chan(Rc<ChanType>),
}

#[derive(Debug, Clone)]
pub enum Stmt {
    Bad(Rc<BadStmt>),
    Decl(Rc<Decl>),
    Empty(Rc<EmptyStmt>),
    Labeled(LabeledStmtKey),
    Expr(Box<Expr>), // Doesn't need to be Rc since Expr is Rc
    Send(Rc<SendStmt>),
    IncDec(Rc<IncDecStmt>),
    Assign(AssignStmtKey),
    Go(Rc<GoStmt>),
    Defer(Rc<DeferStmt>),
    Return(Rc<ReturnStmt>),
    Branch(Rc<BranchStmt>),
    Block(Rc<BlockStmt>),
    If(Rc<IfStmt>),
    Case(Rc<CaseClause>),
    Switch(Rc<SwitchStmt>),
    TypeSwitch(Rc<TypeSwitchStmt>),
    Comm(Rc<CommClause>),
    Select(Rc<SelectStmt>),
    For(Rc<ForStmt>),
    Range(Rc<RangeStmt>),
}

#[derive(Clone, Debug)]
pub enum Spec {
    Import(Rc<ImportSpec>),
    Value(Rc<ValueSpec>),
    Type(Rc<TypeSpec>),
}

#[derive(Clone, Debug)]
pub enum Decl {
    Bad(Rc<BadDecl>),
    Gen(Rc<GenDecl>),
    Func(FuncDeclKey),
}

impl Expr {
    pub fn new_bad(from: position::Pos, to: position::Pos) -> Expr {
        Expr::Bad(Rc::new(BadExpr { from: from, to: to }))
    }

    pub fn new_selector(x: Expr, sel: IdentKey) -> Expr {
        Expr::Selector(Rc::new(SelectorExpr { expr: x, sel: sel }))
    }

    pub fn new_ellipsis(pos: position::Pos, x: Option<Expr>) -> Expr {
        Expr::Ellipsis(Rc::new(Ellipsis { pos: pos, elt: x }))
    }

    pub fn new_basic_lit(pos: position::Pos, token: token::Token) -> Expr {
        Expr::BasicLit(Rc::new(BasicLit {
            pos: pos,
            token: token,
        }))
    }

    pub fn new_unary_expr(pos: position::Pos, op: token::Token, expr: Expr) -> Expr {
        Expr::Unary(Rc::new(UnaryExpr {
            op_pos: pos,
            op: op,
            expr: expr,
        }))
    }

    pub fn box_func_type(ft: FuncType, objs: &mut Objects) -> Expr {
        Expr::Func(objs.ftypes.insert(ft))
    }

    pub fn clone_ident(&self) -> Option<Expr> {
        if let Expr::Ident(i) = self {
            Some(Expr::Ident(i.clone()))
        } else {
            None
        }
    }

    pub fn try_as_ident(&self) -> Option<&IdentKey> {
        if let Expr::Ident(ident) = self {
            Some(ident)
        } else {
            None
        }
    }

    pub fn is_bad(&self) -> bool {
        if let Expr::Bad(_) = self {
            true
        } else {
            false
        }
    }

    pub fn is_type_switch_assert(&self) -> bool {
        if let Expr::TypeAssert(t) = self {
            t.typ.is_none()
        } else {
            false
        }
    }
}

impl Node for Expr {
    fn pos(&self, objs: &Objects) -> position::Pos {
        match &self {
            Expr::Bad(e) => e.from,
            Expr::Ident(e) => objs.idents[*e].pos,
            Expr::Ellipsis(e) => e.pos,
            Expr::BasicLit(e) => e.pos,
            Expr::FuncLit(e) => {
                let typ = &objs.ftypes[e.typ];
                match typ.func {
                    Some(p) => p,
                    None => typ.params.pos(objs),
                }
            }
            Expr::CompositeLit(e) => match &e.typ {
                Some(expr) => expr.pos(objs),
                None => e.l_brace,
            },
            Expr::Paren(e) => e.l_paren,
            Expr::Selector(e) => e.expr.pos(objs),
            Expr::Index(e) => e.expr.pos(objs),
            Expr::Slice(e) => e.expr.pos(objs),
            Expr::TypeAssert(e) => e.expr.pos(objs),
            Expr::Call(e) => e.func.pos(objs),
            Expr::Star(e) => e.star,
            Expr::Unary(e) => e.op_pos,
            Expr::Binary(e) => e.expr_a.pos(objs),
            Expr::KeyValue(e) => e.key.pos(objs),
            Expr::Array(e) => e.l_brack,
            Expr::Struct(e) => e.struct_pos,
            Expr::Func(e) => e.pos(objs),
            Expr::Interface(e) => e.interface,
            Expr::Map(e) => e.map,
            Expr::Chan(e) => e.begin,
        }
    }

    fn end(&self, objs: &Objects) -> position::Pos {
        match &self {
            Expr::Bad(e) => e.to,
            Expr::Ident(e) => objs.idents[*e].end(),
            Expr::Ellipsis(e) => match &e.elt {
                Some(expr) => expr.end(objs),
                None => e.pos + 3,
            },
            Expr::BasicLit(e) => e.pos + e.token.get_literal().len(),
            Expr::FuncLit(e) => e.body.end(),
            Expr::CompositeLit(e) => e.r_brace + 1,
            Expr::Paren(e) => e.r_paren + 1,
            Expr::Selector(e) => objs.idents[e.sel].end(),
            Expr::Index(e) => e.r_brack + 1,
            Expr::Slice(e) => e.r_brack + 1,
            Expr::TypeAssert(e) => e.r_paren + 1,
            Expr::Call(e) => e.r_paren + 1,
            Expr::Star(e) => e.expr.end(objs),
            Expr::Unary(e) => e.expr.end(objs),
            Expr::Binary(e) => e.expr_b.end(objs),
            Expr::KeyValue(e) => e.val.end(objs),
            Expr::Array(e) => e.elt.end(objs),
            Expr::Struct(e) => e.fields.end(objs),
            Expr::Func(e) => e.end(objs),
            Expr::Interface(e) => e.methods.end(objs),
            Expr::Map(e) => e.val.end(objs),
            Expr::Chan(e) => e.val.end(objs),
        }
    }

    fn id(&self) -> NodeId {
        match &self {
            Expr::Bad(e) => NodeId::Address(&**e as *const BadExpr as usize),
            Expr::Ident(e) => NodeId::IdentExpr(*e),
            Expr::Ellipsis(e) => NodeId::Address(&**e as *const Ellipsis as usize),
            Expr::BasicLit(e) => NodeId::Address(&**e as *const BasicLit as usize),
            Expr::FuncLit(e) => NodeId::Address(&**e as *const FuncLit as usize),
            Expr::CompositeLit(e) => NodeId::Address(&**e as *const CompositeLit as usize),
            Expr::Paren(e) => NodeId::Address(&**e as *const ParenExpr as usize),
            Expr::Selector(e) => e.id(),
            Expr::Index(e) => NodeId::Address(&**e as *const IndexExpr as usize),
            Expr::Slice(e) => NodeId::Address(&**e as *const SliceExpr as usize),
            Expr::TypeAssert(e) => NodeId::Address(&**e as *const TypeAssertExpr as usize),
            Expr::Call(e) => e.id(),
            Expr::Star(e) => NodeId::Address(&**e as *const StarExpr as usize),
            Expr::Unary(e) => NodeId::Address(&**e as *const UnaryExpr as usize),
            Expr::Binary(e) => NodeId::Address(&**e as *const BinaryExpr as usize),
            Expr::KeyValue(e) => NodeId::Address(&**e as *const KeyValueExpr as usize),
            Expr::Array(e) => NodeId::Address(&**e as *const ArrayType as usize),
            Expr::Struct(e) => NodeId::Address(&**e as *const StructType as usize),
            Expr::Func(e) => NodeId::FuncTypeExpr(*e),
            Expr::Interface(e) => NodeId::Address(&**e as *const InterfaceType as usize),
            Expr::Map(e) => NodeId::Address(&**e as *const MapType as usize),
            Expr::Chan(e) => NodeId::Address(&**e as *const ChanType as usize),
        }
    }
}

impl Stmt {
    pub fn new_bad(from: position::Pos, to: position::Pos) -> Stmt {
        Stmt::Bad(Rc::new(BadStmt { from: from, to: to }))
    }

    pub fn new_assign(
        objs: &mut Objects,
        lhs: Vec<Expr>,
        tpos: position::Pos,
        tok: token::Token,
        rhs: Vec<Expr>,
    ) -> Stmt {
        Stmt::Assign(AssignStmt::arena_new(objs, lhs, tpos, tok, rhs))
    }

    pub fn box_block(block: BlockStmt) -> Stmt {
        Stmt::Block(Rc::new(block))
    }
}

impl Node for Stmt {
    fn pos(&self, objs: &Objects) -> position::Pos {
        match &self {
            Stmt::Bad(s) => s.from,
            Stmt::Decl(d) => d.pos(objs),
            Stmt::Empty(s) => s.semi,
            Stmt::Labeled(s) => {
                let label = objs.l_stmts[*s].label;
                objs.idents[label].pos
            }
            Stmt::Expr(e) => e.pos(objs),
            Stmt::Send(s) => s.chan.pos(objs),
            Stmt::IncDec(s) => s.expr.pos(objs),
            Stmt::Assign(s) => {
                let assign = &objs.a_stmts[*s];
                assign.pos(objs)
            }
            Stmt::Go(s) => s.go,
            Stmt::Defer(s) => s.defer,
            Stmt::Return(s) => s.ret,
            Stmt::Branch(s) => s.token_pos,
            Stmt::Block(s) => s.pos(),
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
    fn end(&self, objs: &Objects) -> position::Pos {
        match &self {
            Stmt::Bad(s) => s.to,
            Stmt::Decl(d) => d.end(objs),
            Stmt::Empty(s) => {
                if s.implicit {
                    s.semi
                } else {
                    s.semi + 1
                }
            }
            Stmt::Labeled(s) => {
                let ls = &objs.l_stmts[*s];
                ls.stmt.end(objs)
            }
            Stmt::Expr(e) => e.end(objs),
            Stmt::Send(s) => s.val.end(objs),
            Stmt::IncDec(s) => s.token_pos + 2,
            Stmt::Assign(s) => {
                let assign = &objs.a_stmts[*s];
                assign.rhs[assign.rhs.len() - 1].end(objs)
            }
            Stmt::Go(s) => s.call.end(objs),
            Stmt::Defer(s) => s.call.end(objs),
            Stmt::Return(s) => {
                let n = s.results.len();
                if n > 0 {
                    s.results[n - 1].end(objs)
                } else {
                    s.ret + 6
                }
            }
            Stmt::Branch(s) => match &s.label {
                Some(l) => objs.idents[*l].end(),
                None => s.token_pos + s.token.text().len(),
            },
            Stmt::Block(s) => s.end(),
            Stmt::If(s) => match &s.els {
                Some(e) => e.end(objs),
                None => s.body.end(),
            },
            Stmt::Case(s) => {
                let n = s.body.len();
                if n > 0 {
                    s.body[n - 1].end(objs)
                } else {
                    s.colon + 1
                }
            }
            Stmt::Switch(s) => s.body.end(),
            Stmt::TypeSwitch(s) => s.body.end(),
            Stmt::Comm(s) => {
                let n = s.body.len();
                if n > 0 {
                    s.body[n - 1].end(objs)
                } else {
                    s.colon + 1
                }
            }
            Stmt::Select(s) => s.body.end(),
            Stmt::For(s) => s.body.end(),
            Stmt::Range(s) => s.body.end(),
        }
    }

    fn id(&self) -> NodeId {
        match &self {
            Stmt::Bad(s) => NodeId::Address(&**s as *const BadStmt as usize),
            Stmt::Decl(d) => NodeId::Address(&**d as *const Decl as usize),
            Stmt::Empty(e) => NodeId::Address(&**e as *const EmptyStmt as usize),
            Stmt::Labeled(s) => NodeId::LabeledStmt(*s),
            Stmt::Expr(e) => NodeId::Address(&**e as *const Expr as usize),
            Stmt::Send(s) => NodeId::Address(&**s as *const SendStmt as usize),
            Stmt::IncDec(s) => NodeId::Address(&**s as *const IncDecStmt as usize),
            Stmt::Assign(s) => NodeId::AssignStmt(*s),
            Stmt::Go(s) => NodeId::Address(&**s as *const GoStmt as usize),
            Stmt::Defer(s) => NodeId::Address(&**s as *const DeferStmt as usize),
            Stmt::Return(s) => NodeId::Address(&**s as *const ReturnStmt as usize),
            Stmt::Branch(s) => NodeId::Address(&**s as *const BranchStmt as usize),
            Stmt::Block(s) => NodeId::Address(&**s as *const BlockStmt as usize),
            Stmt::If(s) => NodeId::Address(&**s as *const IfStmt as usize),
            Stmt::Case(s) => NodeId::Address(&**s as *const CaseClause as usize),
            Stmt::Switch(s) => NodeId::Address(&**s as *const SwitchStmt as usize),
            Stmt::TypeSwitch(s) => NodeId::Address(&**s as *const TypeSwitchStmt as usize),
            Stmt::Comm(s) => NodeId::Address(&**s as *const CommClause as usize),
            Stmt::Select(s) => NodeId::Address(&**s as *const SelectStmt as usize),
            Stmt::For(s) => NodeId::Address(&**s as *const ForStmt as usize),
            Stmt::Range(s) => NodeId::Address(&**s as *const RangeStmt as usize),
        }
    }
}

impl Node for Spec {
    fn pos(&self, objs: &Objects) -> position::Pos {
        match &self {
            Spec::Import(s) => match &s.name {
                Some(i) => objs.idents[*i].pos,
                None => s.path.pos,
            },
            Spec::Value(s) => objs.idents[s.names[0]].pos,
            Spec::Type(s) => objs.idents[s.name].pos,
        }
    }

    fn end(&self, objs: &Objects) -> position::Pos {
        match &self {
            Spec::Import(s) => match s.end_pos {
                Some(p) => p,
                None => s.path.pos,
            },
            Spec::Value(s) => {
                let n = s.values.len();
                if n > 0 {
                    s.values[n - 1].end(objs)
                } else {
                    match &s.typ {
                        Some(t) => t.end(objs),
                        None => objs.idents[s.names[s.names.len() - 1]].end(),
                    }
                }
            }
            Spec::Type(t) => t.typ.end(objs),
        }
    }

    fn id(&self) -> NodeId {
        match &self {
            Spec::Import(s) => NodeId::Address(&**s as *const ImportSpec as usize),
            Spec::Value(s) => NodeId::Address(&**s as *const ValueSpec as usize),
            Spec::Type(s) => NodeId::Address(&**s as *const TypeSpec as usize),
        }
    }
}

impl Node for Decl {
    fn pos(&self, objs: &Objects) -> position::Pos {
        match &self {
            Decl::Bad(d) => d.from,
            Decl::Gen(d) => d.token_pos,
            Decl::Func(d) => objs.fdecls[*d].pos(objs),
        }
    }

    fn end(&self, objs: &Objects) -> position::Pos {
        match &self {
            Decl::Bad(d) => d.to,
            Decl::Gen(d) => match &d.r_paren {
                Some(p) => p + 1,
                None => objs.specs[d.specs[0]].end(objs),
            },
            Decl::Func(d) => {
                let fd = &objs.fdecls[*d];
                match &fd.body {
                    Some(b) => b.end(),
                    None => fd.typ.end(objs),
                }
            }
        }
    }

    fn id(&self) -> NodeId {
        match self {
            Decl::Bad(d) => NodeId::Address(&**d as *const BadDecl as usize),
            Decl::Gen(d) => NodeId::Address(&**d as *const GenDecl as usize),
            Decl::Func(d) => NodeId::FuncDecl(*d),
        }
    }
}

#[derive(Debug)]
pub struct File {
    pub package: position::Pos,
    pub name: IdentKey,
    pub decls: Vec<Decl>,
    pub scope: ScopeKey,
    pub imports: Vec<SpecKey>, //ImportSpec
    pub unresolved: Vec<IdentKey>,
}

impl Node for File {
    fn pos(&self, _arena: &Objects) -> position::Pos {
        self.package
    }

    fn end(&self, objs: &Objects) -> position::Pos {
        let n = self.decls.len();
        if n > 0 {
            self.decls[n - 1].end(objs)
        } else {
            objs.idents[self.name].end()
        }
    }

    fn id(&self) -> NodeId {
        NodeId::File(self.name)
    }
}

pub struct Package {
    name: String,
    scope: ScopeKey,
    imports: HashMap<String, EntityKey>,
    files: HashMap<String, Box<File>>,
}

// A BadExpr node is a placeholder for expressions containing
// syntax errors for which no correct expression nodes can be
// created.
#[derive(Debug)]
pub struct BadExpr {
    pub from: position::Pos,
    pub to: position::Pos,
}

#[derive(Debug, Clone)]
pub enum IdentEntity {
    NoEntity,
    Sentinel,
    Entity(EntityKey),
}

impl IdentEntity {
    pub fn is_none(&self) -> bool {
        match self {
            IdentEntity::NoEntity => true,
            _ => false,
        }
    }

    pub fn into_key_data(self) -> Option<KeyData> {
        match self {
            IdentEntity::Entity(key) => Some(key.into()),
            _ => None,
        }
    }
}

pub fn is_exported(s: &str) -> bool {
    s.chars().next().unwrap().is_uppercase()
}

// An Ident node represents an identifier.
#[derive(Debug, Clone)]
pub struct Ident {
    pub pos: position::Pos,
    pub name: String,
    pub entity: IdentEntity,
}

impl Ident {
    pub fn blank(pos: position::Pos) -> Ident {
        Ident::with_str(pos, "_")
    }

    pub fn true_(pos: position::Pos) -> Ident {
        Ident::with_str(pos, "true")
    }

    pub fn with_str(pos: position::Pos, s: &str) -> Ident {
        Ident {
            pos: pos,
            name: s.to_owned(),
            entity: IdentEntity::NoEntity,
        }
    }

    pub fn end(&self) -> position::Pos {
        self.pos + self.name.len()
    }

    pub fn entity_key_data(&self) -> Option<KeyData> {
        self.entity.clone().into_key_data()
    }

    pub fn entity_obj<'a>(&self, objs: &'a Objects) -> Option<&'a scope::Entity> {
        match self.entity {
            IdentEntity::Entity(i) => Some(&objs.entities[i]),
            _ => None,
        }
    }

    pub fn is_blank(&self) -> bool {
        &self.name == "_"
    }

    pub fn is_exported(&self) -> bool {
        is_exported(&self.name)
    }
}

// An Ellipsis node stands for the "..." type in a
// parameter list or the "..." length in an array type.
#[derive(Debug)]
pub struct Ellipsis {
    pub pos: position::Pos,
    pub elt: Option<Expr>, // ellipsis element type (parameter lists only)
}

// A BasicLit node represents a literal of basic type.
#[derive(Debug)]
pub struct BasicLit {
    pub pos: position::Pos,
    pub token: token::Token,
}

// A FuncLit node represents a function literal.
#[derive(Debug)]
pub struct FuncLit {
    pub typ: FuncTypeKey,
    pub body: Rc<BlockStmt>,
}

// A CompositeLit node represents a composite literal.
#[derive(Debug)]
pub struct CompositeLit {
    pub typ: Option<Expr>,
    pub l_brace: position::Pos,
    pub elts: Vec<Expr>,
    pub r_brace: position::Pos,
    pub incomplete: bool,
}

// A ParenExpr node represents a parenthesized expression.
#[derive(Debug)]
pub struct ParenExpr {
    pub l_paren: position::Pos,
    pub expr: Expr,
    pub r_paren: position::Pos,
}
// A SelectorExpr node represents an expression followed by a selector.
#[derive(Debug)]
pub struct SelectorExpr {
    pub expr: Expr,
    pub sel: IdentKey,
}

impl SelectorExpr {
    pub fn id(&self) -> NodeId {
        NodeId::Address(self as *const SelectorExpr as usize)
    }
}

// An IndexExpr node represents an expression followed by an index.
#[derive(Debug)]
pub struct IndexExpr {
    pub expr: Expr,
    pub l_brack: position::Pos,
    pub index: Expr,
    pub r_brack: position::Pos,
}

// An SliceExpr node represents an expression followed by slice indices.
#[derive(Debug)]
pub struct SliceExpr {
    pub expr: Expr,
    pub l_brack: position::Pos,
    pub low: Option<Expr>,
    pub high: Option<Expr>,
    pub max: Option<Expr>,
    pub slice3: bool,
    pub r_brack: position::Pos,
}

// A TypeAssertExpr node represents an expression followed by a
// type assertion.
#[derive(Debug)]
pub struct TypeAssertExpr {
    pub expr: Expr,
    pub l_paren: position::Pos,
    pub typ: Option<Expr>,
    pub r_paren: position::Pos,
}

// A CallExpr node represents an expression followed by an argument list.
#[derive(Debug)]
pub struct CallExpr {
    pub func: Expr,
    pub l_paren: position::Pos,
    pub args: Vec<Expr>,
    pub ellipsis: Option<position::Pos>,
    pub r_paren: position::Pos,
}

impl CallExpr {
    pub fn id(&self) -> NodeId {
        NodeId::Address(self as *const CallExpr as usize)
    }
}

// A StarExpr node represents an expression of the form "*" Expression.
// Semantically it could be a unary "*" expression, or a pointer type.
#[derive(Debug)]
pub struct StarExpr {
    pub star: position::Pos,
    pub expr: Expr,
}

// A UnaryExpr node represents a unary expression.
// Unary "*" expressions are represented via StarExpr nodes.
#[derive(Debug)]
pub struct UnaryExpr {
    pub op_pos: position::Pos,
    pub op: token::Token,
    pub expr: Expr,
}

// A BinaryExpr node represents a binary expression.
#[derive(Debug)]
pub struct BinaryExpr {
    pub expr_a: Expr,
    pub op_pos: position::Pos,
    pub op: token::Token,
    pub expr_b: Expr,
}

// A KeyValueExpr node represents (key : value) pairs
// in composite literals.
#[derive(Debug)]
pub struct KeyValueExpr {
    pub key: Expr,
    pub colon: position::Pos,
    pub val: Expr,
}

// An ArrayType node represents an array or slice type.
#[derive(Debug)]
pub struct ArrayType {
    pub l_brack: position::Pos,
    pub len: Option<Expr>, // Ellipsis node for [...]T array types, None for slice types
    pub elt: Expr,
}

// A StructType node represents a struct type.
#[derive(Debug)]
pub struct StructType {
    pub struct_pos: position::Pos,
    pub fields: FieldList,
    pub incomplete: bool,
}

// Pointer types are represented via StarExpr nodes.

// A FuncType node represents a function type.
#[derive(Clone, Debug)]
pub struct FuncType {
    pub func: Option<position::Pos>,
    pub params: FieldList,
    pub results: Option<FieldList>,
}

impl FuncType {
    pub fn new(
        func: Option<position::Pos>,
        params: FieldList,
        results: Option<FieldList>,
    ) -> FuncType {
        FuncType {
            func: func,
            params: params,
            results: results,
        }
    }
}

impl Node for FuncTypeKey {
    fn pos(&self, objs: &Objects) -> position::Pos {
        let self_ = &objs.ftypes[*self];
        match self_.func {
            Some(p) => p,
            None => self_.params.pos(objs),
        }
    }

    fn end(&self, objs: &Objects) -> position::Pos {
        let self_ = &objs.ftypes[*self];
        match &self_.results {
            Some(r) => (*r).end(objs),
            None => self_.params.end(objs),
        }
    }

    fn id(&self) -> NodeId {
        NodeId::FuncType(*self)
    }
}

// An InterfaceType node represents an interface type.
#[derive(Clone, Debug)]
pub struct InterfaceType {
    pub interface: position::Pos,
    pub methods: FieldList,
    pub incomplete: bool,
}

// A MapType node represents a map type.
#[derive(Debug)]
pub struct MapType {
    pub map: position::Pos,
    pub key: Expr,
    pub val: Expr,
}

// A ChanType node represents a channel type.
#[derive(Hash, Eq, PartialEq, Clone, Debug)]
pub enum ChanDir {
    Send = 1,
    Recv = 2,
    SendRecv = 3,
}

#[derive(Clone, Debug)]
pub struct ChanType {
    pub begin: position::Pos,
    pub arrow: position::Pos,
    pub dir: ChanDir,
    pub val: Expr,
}

// An ImportSpec node represents a single package import.
#[derive(Debug)]
pub struct ImportSpec {
    pub name: Option<IdentKey>,
    pub path: BasicLit,
    pub end_pos: Option<position::Pos>,
}

// A ValueSpec node represents a constant or variable declaration
// (ConstSpec or VarSpec production).
#[derive(Debug)]
pub struct ValueSpec {
    pub names: Vec<IdentKey>,
    pub typ: Option<Expr>,
    pub values: Vec<Expr>,
}

// A TypeSpec node represents a type declaration (TypeSpec production).
#[derive(Debug)]
pub struct TypeSpec {
    pub name: IdentKey,
    pub assign: position::Pos,
    pub typ: Expr,
}

#[derive(Debug)]
pub struct BadDecl {
    pub from: position::Pos,
    pub to: position::Pos,
}

// A GenDecl node (generic declaration node) represents an import,
// constant, type or variable declaration. A valid Lparen position
// (Lparen.IsValid()) indicates a parenthesized declaration.
//
// Relationship between Tok value and Specs element type:
//
//	Token::IMPORT  ImportSpec
//	Token::CONST   ValueSpec
//	Token::TYPE    TypeSpec
//	Token::VAR     ValueSpec
#[derive(Debug)]
pub struct GenDecl {
    pub token_pos: position::Pos,
    pub token: token::Token,
    pub l_paran: Option<position::Pos>,
    pub specs: Vec<SpecKey>,
    pub r_paren: Option<position::Pos>,
}

// A FuncDecl node represents a function declaration.
#[derive(Debug)]
pub struct FuncDecl {
    pub recv: Option<FieldList>,
    pub name: IdentKey,
    pub typ: FuncTypeKey,
    pub body: Option<Rc<BlockStmt>>,
}

impl FuncDecl {
    pub fn pos(&self, objs: &Objects) -> position::Pos {
        self.typ.pos(objs)
    }
}

#[derive(Debug)]
pub struct BadStmt {
    pub from: position::Pos,
    pub to: position::Pos,
}

#[derive(Debug)]
pub struct EmptyStmt {
    pub semi: position::Pos,
    pub implicit: bool,
}

// A LabeledStmt node represents a labeled statement.
#[derive(Debug)]
pub struct LabeledStmt {
    pub label: IdentKey,
    pub colon: position::Pos,
    pub stmt: Stmt,
}

impl LabeledStmt {
    pub fn arena_new(
        objs: &mut Objects,
        label: IdentKey,
        colon: position::Pos,
        stmt: Stmt,
    ) -> LabeledStmtKey {
        let l = LabeledStmt {
            label: label,
            colon: colon,
            stmt: stmt,
        };
        objs.l_stmts.insert(l)
    }

    pub fn pos(&self, objs: &Objects) -> position::Pos {
        objs.idents[self.label].pos
    }
}

// A SendStmt node represents a send statement.
#[derive(Debug)]
pub struct SendStmt {
    pub chan: Expr,
    pub arrow: position::Pos,
    pub val: Expr,
}

// An IncDecStmt node represents an increment or decrement statement.
#[derive(Debug)]
pub struct IncDecStmt {
    pub expr: Expr,
    pub token_pos: position::Pos,
    pub token: token::Token,
}

// An AssignStmt node represents an assignment or
// a short variable declaration.
#[derive(Debug)]
pub struct AssignStmt {
    pub lhs: Vec<Expr>,
    pub token_pos: position::Pos,
    pub token: token::Token,
    pub rhs: Vec<Expr>,
}

impl AssignStmt {
    pub fn arena_new(
        objs: &mut Objects,
        lhs: Vec<Expr>,
        tpos: position::Pos,
        tok: token::Token,
        rhs: Vec<Expr>,
    ) -> AssignStmtKey {
        let ass = AssignStmt {
            lhs: lhs,
            token_pos: tpos,
            token: tok,
            rhs: rhs,
        };
        objs.a_stmts.insert(ass)
    }

    pub fn pos(&self, objs: &Objects) -> position::Pos {
        self.lhs[0].pos(objs)
    }
}

#[derive(Debug)]
pub struct GoStmt {
    pub go: position::Pos,
    pub call: Expr,
}
#[derive(Debug)]
pub struct DeferStmt {
    pub defer: position::Pos,
    pub call: Expr,
}

#[derive(Debug)]
pub struct ReturnStmt {
    pub ret: position::Pos,
    pub results: Vec<Expr>,
}

// A BranchStmt node represents a break, continue, goto,
// or fallthrough statement.
#[derive(Debug)]
pub struct BranchStmt {
    pub token_pos: position::Pos,
    pub token: token::Token,
    pub label: Option<IdentKey>,
}

#[derive(Debug)]
pub struct BlockStmt {
    pub l_brace: position::Pos,
    pub list: Vec<Stmt>,
    pub r_brace: position::Pos,
}

impl BlockStmt {
    pub fn new(l: position::Pos, list: Vec<Stmt>, r: position::Pos) -> BlockStmt {
        BlockStmt {
            l_brace: l,
            list: list,
            r_brace: r,
        }
    }

    pub fn pos(&self) -> position::Pos {
        self.l_brace
    }

    pub fn end(&self) -> position::Pos {
        self.r_brace + 1
    }
}

#[derive(Debug)]
pub struct IfStmt {
    pub if_pos: position::Pos,
    pub init: Option<Stmt>,
    pub cond: Expr,
    pub body: Rc<BlockStmt>,
    pub els: Option<Stmt>,
}

// A CaseClause represents a case of an expression or type switch statement.
#[derive(Debug)]
pub struct CaseClause {
    pub case: position::Pos,
    pub list: Option<Vec<Expr>>,
    pub colon: position::Pos,
    pub body: Vec<Stmt>,
}

#[derive(Debug)]
pub struct SwitchStmt {
    pub switch: position::Pos,
    pub init: Option<Stmt>,
    pub tag: Option<Expr>,
    pub body: Rc<BlockStmt>,
}

#[derive(Debug)]
pub struct TypeSwitchStmt {
    pub switch: position::Pos,
    pub init: Option<Stmt>,
    pub assign: Stmt,
    pub body: Rc<BlockStmt>,
}

// A CommClause node represents a case of a select statement.
#[derive(Debug)]
pub struct CommClause {
    //communication
    pub case: position::Pos,
    pub comm: Option<Stmt>,
    pub colon: position::Pos,
    pub body: Vec<Stmt>,
}

#[derive(Debug)]
pub struct SelectStmt {
    pub select: position::Pos,
    pub body: Rc<BlockStmt>,
}

#[derive(Debug)]
pub struct ForStmt {
    pub for_pos: position::Pos,
    pub init: Option<Stmt>,
    pub cond: Option<Expr>,
    pub post: Option<Stmt>,
    pub body: Rc<BlockStmt>,
}

#[derive(Debug)]
pub struct RangeStmt {
    pub for_pos: position::Pos,
    pub key: Option<Expr>,
    pub val: Option<Expr>,
    pub token_pos: position::Pos,
    pub token: token::Token,
    pub expr: Expr,
    pub body: Rc<BlockStmt>,
}

#[derive(Debug)]
pub struct Field {
    pub names: Vec<IdentKey>,
    pub typ: Expr,
    pub tag: Option<Expr>,
}

impl Node for FieldKey {
    fn pos(&self, objs: &Objects) -> position::Pos {
        let self_ = &objs.fields[*self];
        if self_.names.len() > 0 {
            objs.idents[self_.names[0]].pos
        } else {
            self_.typ.pos(objs)
        }
    }

    fn end(&self, objs: &Objects) -> position::Pos {
        let self_ = &objs.fields[*self];
        match &self_.tag {
            Some(t) => t.end(objs),
            None => self_.typ.end(objs),
        }
    }

    fn id(&self) -> NodeId {
        NodeId::Field(*self)
    }
}

#[derive(Clone, Debug)]
pub struct FieldList {
    pub openning: Option<position::Pos>,
    pub list: Vec<FieldKey>,
    pub closing: Option<position::Pos>,
}

impl FieldList {
    pub fn new(
        openning: Option<position::Pos>,
        list: Vec<FieldKey>,
        closing: Option<position::Pos>,
    ) -> FieldList {
        FieldList {
            openning: openning,
            list: list,
            closing: closing,
        }
    }

    pub fn pos(&self, objs: &Objects) -> position::Pos {
        match self.openning {
            Some(o) => o,
            None => self.list[0].pos(objs),
        }
    }

    pub fn end(&self, objs: &Objects) -> position::Pos {
        match self.closing {
            Some(c) => c,
            None => self.list[self.list.len() - 1].pos(objs),
        }
    }
}
