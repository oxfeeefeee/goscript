use super::position;
use std::cell::{Ref, RefCell};
use std::fmt;
use std::rc::Rc;

#[derive(Clone, Debug)]
struct Error {
    pos: position::Position,
    msg: String,
}

#[derive(Clone, Debug)]
struct ErrorListImpl {
    errs: Vec<Error>,
}

#[derive(Clone, Debug)]
pub struct ErrorList {
    imp: Rc<RefCell<ErrorListImpl>>,
}

impl fmt::Display for ErrorList {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Result: {} errors\n", self.imp.borrow().errs.len())?;
        for e in self.imp.borrow().errs.iter() {
            write!(f, "{} {}\n", e.pos, e.msg)?;
        }
        Ok(())
    }
}

impl ErrorList {
    pub fn new() -> ErrorList {
        ErrorList {
            imp: Rc::new(RefCell::new(ErrorListImpl { errs: vec![] })),
        }
    }

    pub fn add(&self, p: position::Position, msg: String) {
        self.imp.borrow_mut().errs.push(Error { pos: p, msg: msg });
    }

    pub fn len(&self) -> usize {
        self.imp.borrow().errs.len()
    }
}

#[derive(Clone, Debug)]
pub struct FilePosErrors<'a> {
    file: &'a position::File,
    elist: &'a ErrorList,
}

impl<'a> FilePosErrors<'a> {
    pub fn new(file: &'a position::File, elist: &'a ErrorList) -> FilePosErrors<'a> {
        FilePosErrors {
            file: file,
            elist: elist,
        }
    }

    pub fn add(&self, pos: position::Pos, msg: String) {
        let p = self.file.position(pos);
        self.elist.add(p, msg);
    }

    pub fn add_str(&self, pos: position::Pos, s: &str) {
        self.add(pos, s.to_string());
    }
}
