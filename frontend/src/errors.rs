use std::fmt;
use super::position;

struct Error{
    pos: position::Position,
    msg: String,
}

pub struct ErrorList {
    errs: Vec<Error>,
}

impl fmt::Display for ErrorList {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "parse result: {} errors\n", self.errs.len())?;
        for e in self.errs.iter() {
            write!(f, "{} {}\n", e.pos, e.msg)?;
        }
        Ok(())
    }
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