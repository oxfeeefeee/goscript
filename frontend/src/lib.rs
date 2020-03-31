#![allow(dead_code)]
pub mod ast;
pub mod ast_objects;
mod errors;
mod parser;
mod position;
mod scanner;
mod scope;
pub mod token;
pub mod visitor;

pub use parser::Parser;
pub use position::FileSet;
pub use token::Token;

pub fn parse_file<'o, 'f, 's>(
    o: &'o mut ast_objects::Objects,
    fs: &'f mut position::FileSet,
    name: &str,
    src: &'s str,
    trace: bool,
) -> (parser::Parser<'o, 'f, 's>, Option<ast::File>) {
    let f = fs.add_file(name, None, src.chars().count());
    let mut p = parser::Parser::new(o, f, src, trace);
    let file = p.parse_file();
    (p, file)
}
