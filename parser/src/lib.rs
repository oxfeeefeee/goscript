#![allow(dead_code)]
pub mod ast;
pub mod errors;
pub mod objects;
mod parser;
pub mod position;
mod scanner;
pub mod scope;
pub mod token;
pub mod visitor;

pub use ast::Expr;
pub use parser::Parser;
pub use position::{FileSet, Pos, Position};
pub use token::Token;

pub fn parse_file<'a>(
    o: &'a mut objects::Objects,
    fs: &'a mut position::FileSet,
    el: &'a errors::ErrorList,
    name: &str,
    src: &'a str,
    trace: bool,
) -> (parser::Parser<'a>, Option<ast::File>) {
    let f = fs.add_file(name.to_string(), None, src.chars().count());
    let mut p = parser::Parser::new(o, f, el, src, trace);
    let file = p.parse_file();
    (p, file)
}
