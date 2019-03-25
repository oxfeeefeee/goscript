use super::position;

struct Error{
    pos: position::Position,
    msg: String,
}

pub struct ErrorList {
    errs: Vec<Error>,
}

impl ErrorList {
    pub fn new() -> ErrorList {
        ErrorList{errs: vec![]}
    }

    pub fn add(&mut self, p: position::Position, msg: String) {
        self.errs.push(Error{pos:p, msg:msg});
    }

    pub fn len(&self) -> usize {
        self.errs.len()
    } 
}