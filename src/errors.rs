use std::fmt;
use super::position;

struct Error{
    pos: position::Position,
    msg: String,
}

pub struct ErrorList {
    errs: Vec<Box<Error>>,
}

impl ErrorList {
    pub fn new() -> ErrorList {
        ErrorList{errs: vec![]}
    }

    pub fn add(&mut self, p: position::Position, msg: String) {
        self.errs.push(Box::new(Error{pos:p, msg:msg}));
    }
}