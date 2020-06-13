#![allow(dead_code)]

use super::super::obj::LangObj;
use super::check::Checker;
use goscript_parser::position::{Pos, Position};

impl<'a> Checker<'a> {
    /// invalid_ast helps to report ast error
    pub fn invalid_ast(&self, pos: Pos, err: &str) {
        self.error(pos, format!("invalid AST: {}", err));
    }

    // has_cycle reports whether obj appears in path or not.
    // If it does, and report is set, it also reports a cycle error.
    pub fn has_cycle(&self, obj: &LangObj, path: &Vec<&LangObj>, report: bool) -> bool {
        unimplemented!()
    }
}
