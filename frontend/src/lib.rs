#![allow(dead_code)]
mod token;
mod position;
mod scanner;
mod ast;
mod scope;
mod ast_objects;
mod errors;
mod parser;

pub use position::FileSet;
pub use token::Token;
pub use parser::Parser;

pub fn parse_file<'a>(fs: &'a mut position::FileSet,
    name: &'a str, src: &'a str, trace: bool) -> parser::Parser<'a> {
    let f = fs.add_file(name, None, src.chars().count());
    let mut p = parser::Parser::new(f, src, trace);
    p.parse_file();
    p
}