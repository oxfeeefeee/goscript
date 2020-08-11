use super::position;
use std::cell::{Ref, RefCell};
use std::fmt;
use std::rc::Rc;

#[derive(Clone, Debug)]
pub struct Error {
    pub pos: position::Position,
    pub msg: String,
    pub soft: bool,
    by_parser: bool, // reported by parser (not type checker)
    order: usize,    // display order
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let p = if self.by_parser { "[Parser]" } else { "[TC]" };
        write!(f, "{} {}  {}\n", p, self.pos, self.msg)?;
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct ErrorList {
    errors: Rc<RefCell<Vec<Error>>>,
}

impl fmt::Display for ErrorList {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Result: {} errors\n", self.errors.borrow().len())?;
        for e in self.errors.borrow().iter() {
            e.fmt(f)?;
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

    pub fn add(&self, p: position::Position, msg: String, soft: bool, by_parser: bool) {
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
            msg: msg,
            soft: soft,
            by_parser: by_parser,
            order: order,
        });
    }

    pub fn len(&self) -> usize {
        self.errors.borrow().len()
    }

    pub fn sort(&mut self) {
        self.errors.borrow_mut().sort_by_key(|e| e.order);
    }

    pub fn borrow(&self) -> Ref<Vec<Error>> {
        self.errors.borrow()
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

    pub fn add(&self, pos: position::Pos, msg: String, soft: bool) {
        let p = self.file.position(pos);
        self.elist.add(p, msg, soft, false);
    }

    pub fn add_str(&self, pos: position::Pos, s: &str, soft: bool) {
        self.add(pos, s.to_string(), soft);
    }

    pub fn parser_add(&self, pos: position::Pos, msg: String) {
        let p = self.file.position(pos);
        self.elist.add(p, msg, false, true);
    }

    pub fn parser_add_str(&self, pos: position::Pos, s: &str) {
        self.parser_add(pos, s.to_string());
    }
}
