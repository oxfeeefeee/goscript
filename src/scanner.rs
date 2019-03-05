use std::fmt;
use std::iter::Peekable;
use std::str::Chars;
use std::path::Path;
use super::position;
use super::token::Token;

type ErrorHandler = fn(pos: position::Position, msg: &str);

pub struct Scanner<'a> {
    file: &'a mut position::File,  // source file handle
    dir: String,                // directory portion of file.Name()
    src: Peekable<Chars<'a>>,   // source
    err: ErrorHandler,          // error reporting

    offset: usize,      // character offset
	line_offset: usize, // current line offset
    insert_semi: bool,

    error_count: isize,
}

impl<'a> Scanner<'a> {
    fn new<'b>(file: &'b mut position::File, src: &'b str, err: ErrorHandler) -> Scanner<'b> {
        let dir = Path::new(file.name()).parent().unwrap().
                to_string_lossy().into_owned();
        Scanner{
            file: file,
            dir: dir,
            src: src.chars().peekable(),
            err: err,
            offset: 0,
            line_offset: 0,
            insert_semi: false,
            error_count: 0,
        }
    }

    fn error(&mut self, pos: usize, msg: &str) {
        self.error_count += 1;
        let p = self.file.position(pos);
        (self.err)(p, msg);
    }

    // Read the next Unicode char 
    fn scan(&mut self) -> (Token, position::Position) {
        self.skip_whitespace();

        let pos = self.file.position(self.offset);

        let insert_semi = false;
        match self.read_char() {
            Some(ch) if is_letter(ch) => {
                let t = self.scan_identifier(ch);
                match t {
                    Token::BREAK | Token::CONTINUE | Token::FALLTHROUGH | Token::RETURN =>
                        self.insert_semi = true,
                    Token::IDENT(_) => self.insert_semi = true,
                    _ => {}
                }
                (t, pos)
            },
            Some(ch) if is_digit(ch) => {
                self.insert_semi = true;
                let t = self.scan_number(ch);
                (t, pos)
            },
            Some(_) => {
                (Token::ILLEGAL, pos)
            },
            None => {
                if self.insert_semi {
                    self.insert_semi = false;
                    (Token::SEMICOLON, pos)
                } else {
                    (Token::EOF, pos)
                }
            }
        }
    }

    fn scan_identifier(&mut self, ch: char) -> Token {
        let mut s = ch.to_string(); 
        loop {
            match self.peek_char() {
                Some(&ch) if is_letter(ch) || is_digit(ch) => {
                    self.read_char().unwrap(); //advance
                    s.push(ch);
                },
                _ => break,
            }
        }
        Token::ident_token(s)
    }

    fn scan_number(&mut self, ch: char) -> Token {


        Token::INT(String::new(), 0)
    }

    fn skip_whitespace(&mut self) {
        while let Some(&ch) = self.peek_char() {
            // skip \n as whitespace only when we don't need to insert semicolum
            if ch == ' ' || ch == '\t' || ch == '\n' && !self.insert_semi || ch == '\r' {
                self.read_char().unwrap();
            } else {
                break;
            }
        }
    }

    fn read_char(&mut self) -> Option<char> {
        let next = self.src.next();
        match next {
            Some(ch) => {
                if ch == '\n' {
                    self.line_offset = self.offset;
                    self.file.add_line(self.offset);
                }
                self.offset+=1;
            }
            None => {}
        }
        next
    }

    fn peek_char(&mut self) -> Option<&char> {
        self.src.peek()
    }
}

fn is_letter(ch: char) -> bool {
    ch.is_alphabetic() || ch == '_'
}

fn is_digit(ch: char) -> bool {
    ch.is_digit(10)
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_scanner(){

    }
}