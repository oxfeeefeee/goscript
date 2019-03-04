use std::fmt;
use std::path::Path;
use super::position;
use super::token;

type ErrorHandler = fn(pos: position::Position, msg: &str);

pub struct Scanner<'a> {
    file: &'a mut position::File,  // source file handle
    dir: String,                // directory portion of file.Name()
    src: String,                // source
    err: ErrorHandler,          // error reporting

    ch: char,           //current utf8 character
    offset: usize,      // character offset
	read_offset: usize, // reading offset (position after current character)
	line_offset: usize, // current line offset

    error_count: isize,
}

impl<'a> Scanner<'a> {
    fn new<'b>(file: &'b mut position::File, src: &Vec<u8>, err: ErrorHandler) -> Scanner<'b> {
        let dir = Path::new(file.name()).parent().unwrap().
                to_string_lossy().into_owned();
        let mut scanner = Scanner{
            file: file,
            dir: dir,
            src: String::new(),
            err: err,
            ch: 0 as char,
            offset: 0,
            read_offset: 0,
            line_offset: 0,
            error_count: 0,
        };
        match String::from_utf8(src.to_vec()) {
            Ok(s) => scanner.src = s,
            Err(e) => scanner.error(0, "Invalid utf_8 data"),
        }
        scanner
    }

    fn error(&mut self, pos: usize, msg: &str) {
        self.error_count += 1;
        let p = self.file.position(pos);
        (self.err)(p, msg);
    }

    // Read the next Unicode char into self.ch.
    // self.ch < 0 means end-of-file.
    fn next(&mut self) {
        if self.read_offset < self.src.len() {
            self.offset = self.read_offset;
            if self.ch == '\n' {
                
            }
        } else {

        }
    }
}



#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_scanner(){

    }
}