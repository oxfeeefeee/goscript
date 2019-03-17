use std::fmt;
use std::rc::Rc;
use std::cell::{Ref, RefMut, RefCell};
use super::position;
use super::scanner;
use super::errors;
use super::helpers::{Defer};

macro_rules! trace {
    ($self:ident, $msg:expr) => {
        let mut trace_str = $msg.to_string();
        trace_str.push('(');
        $self.print_trace(&trace_str);
        $self.indent += 1;
        let defer_ins = Defer::new(|| {
            $self.indent -= 1;
            $self.print_trace(")");

        });
    };
}

pub struct Parser<'a> {
    scanner: scanner::Scanner<'a>,
    errors: Rc<RefCell<errors::ErrorList>>,

    trace: bool,
    indent: isize,
}

impl<'a> Parser<'a> {
    fn new(file: &'a mut position::File, src: &'a str, trace: bool) -> Parser<'a> {
        let err = Rc::new(RefCell::new(errors::ErrorList::new()));
        let s = scanner::Scanner::new(file, src, err.clone());
        Parser{
            scanner: s,
            errors: err,
            trace: trace,
            indent: 0,
        }
    }

    fn error(&mut self, pos: position::Position, msg: &str) {
        self.errors.borrow_mut().add(pos, msg.to_string())
    }

    fn print_trace(&self, msg: &str) {
        print!("{}", msg);
    }
    
    fn parse(&mut self) {
        trace!(self, "aa");
        print!("222");
    }
}

#[cfg(test)]
mod test {
	use super::*;

	#[test]
	fn test_parser () {
        let fs = position::SharedFileSet::new();
        let mut fsm = fs.borrow_mut();
        let f = fsm.add_file(fs.weak(), "testfile1.gs", 0, 1000);

        let mut p = Parser::new(f, "", true);
        p.parse();
    }
}