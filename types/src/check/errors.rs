#![allow(dead_code)]
use super::check::Checker;
use goscript_parser::errors::FilePosErrors;
use goscript_parser::position::Pos;

impl<'a> Checker<'a> {
    fn error_undefined(&self, pos: Pos, name: &String) {
        let file = self.fset().file(pos).unwrap();
        FilePosErrors::new(file, self.errors()).add(pos, format!("undefined: {}", name));
    }
}
