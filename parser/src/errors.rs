use super::position;
use std::cell::RefCell;
use std::fmt;
use std::rc::Rc;

#[derive(Clone, Debug)]
struct Error {
    pos: position::Position,
    order: usize, // display order
    msg: String,
}

#[derive(Clone, Debug)]
pub struct ErrorList {
    errors: Rc<RefCell<Vec<Error>>>,
}

impl fmt::Display for ErrorList {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Result: {} errors\n", self.errors.borrow().len())?;
        for e in self.errors.borrow().iter() {
            write!(f, "{} {}\n", e.pos, e.msg)?;
        }
        Ok(())
    }
}

impl ErrorList {
    pub fn new() -> ErrorList {
        ErrorList {
            errors: Rc::new(RefCell::new(vec![])),
        }
    }

    pub fn add(&self, p: position::Position, msg: String) {
        let order = if msg.starts_with('\t') {
            self.errors
                .borrow()
                .iter()
                .rev()
                .find(|x| !x.msg.starts_with('\t'))
                .unwrap()
                .pos
                .offset
        } else {
            p.offset
        };
        self.errors.borrow_mut().push(Error {
            pos: p,
            order: order,
            msg: msg,
        });
    }

    pub fn len(&self) -> usize {
        self.errors.borrow().len()
    }

    pub fn sort(&mut self) {
        self.errors.borrow_mut().sort_by_key(|e| e.order);
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
