// Copyright 2022 The Goscript Authors. All rights reserved.
// Use of this source code is governed by a BSD-style
// license that can be found in the LICENSE file.

mod errors;
mod map;
mod objects;
mod parser;
mod position;
mod scanner;
mod token;

pub mod ast;
pub mod scope;
pub mod visitor;

pub use errors::*;
pub use map::{Map, MapIter};
pub use objects::*;
pub use parser::Parser;
pub use position::*;
pub use token::*;

pub fn parse_file<'a>(
    o: &'a mut AstObjects,
    fs: &'a mut FileSet,
    el: &'a ErrorList,
    name: &str,
    src: &'a str,
    trace: bool,
) -> (parser::Parser<'a>, Option<ast::File>) {
    let f = fs.add_file(name.to_string(), None, src.chars().count());
    let mut p = parser::Parser::new(o, f, el, src, trace);
    let file = p.parse_file();
    (p, file)
}
