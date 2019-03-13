use std::fmt;
use super::position;
use super::token;
use super::scope;

pub trait Node {
    fn pos(&self) -> position::Pos;
    fn end(&self) -> position::Pos;
}

pub enum Expr {

}

// A BadExpr node is a placeholder for expressions containing
// syntax errors for which no correct expression nodes can be
// created.
pub struct BadExpr {
    from: position::Pos,
    to: position::Pos,
}

// An Ident node represents an identifier.
pub struct Ident {
    pos: position::Pos,
    name: String,
    obj: Box<scope::Object>,
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
    //typ: Box<FuncType>,
    //body: Box<BlockStmt>,
}	

// A CompositeLit node represents a composite literal.
pub struct CompositLit {
    typ: Expr,
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
    expr: Expr,
    sel: Box<Ident>,
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
    star: Expr,
    expr: Expr,
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
    value: Expr,
}


pub enum Stmt {

}

pub enum Decl {

}

pub struct Field {
    names: Option<Vec<Box<Expr>>>,
    typ: Box<Expr>,
    tag: Option<Box<Expr>>,
}

impl Node for Field {
    fn pos(&self) -> position::Pos {
        0 as position::Pos
    }

    fn end(&self) -> position::Pos {
        0 as position::Pos
    }
}

pub struct FieldList {
    openning: token::Token,
    list: Vec<Box<Field>>,
    closing: token::Token,
}



