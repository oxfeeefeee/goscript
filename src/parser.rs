use std::fmt;
use std::cell::{Ref, RefMut, RefCell};
use super::position;
use super::scanner;
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

    trace: bool,
    indent: isize,
}

impl<'a, 'b> Parser<'a> {
    fn new(file: &'b mut position::File, src: &'b str, trace: bool) -> Parser<'b> {
        let mut s = scanner::Scanner::new(file, src, |pos: position::Position, msg: &str| {
            print!("scan error at {}: {}\n", pos, msg);
        });
        Parser{
            scanner: s,
            trace: trace,
            indent: 0,
        }
    }

    fn print_trace(&self, msg: &str) {
        println!("{}", msg);
    }
    
    fn parse(&mut self) {
        trace!(self, "aa");
        println!("222");
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