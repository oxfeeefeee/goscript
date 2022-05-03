// Copyright 2022 The Goscript Authors. All rights reserved.
// Use of this source code is governed by a BSD-style
// license that can be found in the LICENSE file.
//
//
// This code is adapted from the offical Go code written in Go
// with license as follows:
// Copyright 2013 The Go Authors. All rights reserved.
// Use of this source code is governed by a BSD-style
// license that can be found in the LICENSE file.

use super::position::{File, FilePos, Pos};
use std::cell::{Ref, RefCell};
use std::fmt;
use std::rc::Rc;

#[derive(Clone, Debug)]
pub struct Error {
    pub pos: FilePos,
    pub msg: String,
    pub soft: bool,
    pub by_parser: bool, // reported by parser (not type checker)
    order: usize,        // display order
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

    pub fn add(&self, p: Option<FilePos>, msg: String, soft: bool, by_parser: bool) {
        let fp = p.unwrap_or(FilePos::null());
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
            fp.offset
        };
        self.errors.borrow_mut().push(Error {
            pos: fp,
            msg: msg,
            soft: soft,
            by_parser: by_parser,
            order: order,
        });
    }

    pub fn len(&self) -> usize {
        self.errors.borrow().len()
    }

    pub fn sort(&self) {
        self.errors.borrow_mut().sort_by_key(|e| e.order);
    }

    pub fn borrow(&self) -> Ref<Vec<Error>> {
        self.errors.borrow()
    }
}

#[derive(Clone, Debug)]
pub struct FilePosErrors<'a> {
    file: &'a File,
    elist: &'a ErrorList,
}

impl<'a> FilePosErrors<'a> {
    pub fn new(file: &'a File, elist: &'a ErrorList) -> FilePosErrors<'a> {
        FilePosErrors {
            file: file,
            elist: elist,
        }
    }

    pub fn add(&self, pos: Pos, msg: String, soft: bool) {
        let p = self.file.position(pos);
        self.elist.add(Some(p), msg, soft, false);
    }

    pub fn add_str(&self, pos: Pos, s: &str, soft: bool) {
        self.add(pos, s.to_string(), soft);
    }

    pub fn parser_add(&self, pos: Pos, msg: String) {
        let p = self.file.position(pos);
        self.elist.add(Some(p), msg, false, true);
    }

    pub fn parser_add_str(&self, pos: Pos, s: &str) {
        self.parser_add(pos, s.to_string());
    }
}
