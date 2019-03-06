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
    #[allow(dead_code)]
    fn scan(&mut self) -> (Token, position::Position) {
        self.skip_whitespace();

        let pos = self.file.position(self.offset);

        let insert_semi = false;
        match self.read_char() {
            Some(ch) => {
                if is_letter(ch) {
                    let t = self.scan_identifier(ch);
                    match t {
                        Token::BREAK | Token::CONTINUE | Token::FALLTHROUGH | Token::RETURN =>
                            self.insert_semi = true,
                        Token::IDENT(_) => self.insert_semi = true,
                        _ => {}
                    }
                    (t, pos)
                } else if self.number_start_with(ch) {
                    self.insert_semi = true;
                    let t = self.scan_number(ch);
                    (t, pos)
                } else {
                    (Token::ILLEGAL, pos)
                }
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
                Some(&ch) if is_letter(ch) || is_decimal(ch) => {
                    self.read_char().unwrap(); //advance
                    s.push(ch);
                },
                _ => break,
            }
        }
        Token::ident_token(s)
    }

    fn scan_number(&mut self, ch: char) -> Token {
        let mut tok = Token::ILLEGAL;
        let mut literal = ch.to_string(); 
        if ch != '.' {
            if ch == '0' {
                match self.read_char() {
                    // hexadecimal int
                    Some('x') | Some('X') => {
                        literal.push('x');
                        self.scan_digits(&mut literal, is_hex);
                        if literal.len() == 2 {
                            self.error(self.offset, "illegal hexadecimal number")
                        } else {
                            tok = Token::INT(literal);
                        }
                    },
                    _ => {
                        // octal int or float
                        self.scan_digits(&mut literal, is_octal);
                        match self.peek_char() {
                            Some('8') | Some('9') => self.error(self.offset, "illegal octal number"),
                            Some('e') | Some('E') => {
                                self.read_char().unwrap();
                                self.scan_exponent(&mut literal);
                                tok = Token::FLOAT(literal);
                            },
                            Some('.') => {
                                self.read_char().unwrap();
                                self.scan_fraction(&mut literal);
                                tok = Token::FLOAT(literal);
                            },
                            Some('i') => {

                            },
                            _ => {
                                tok = Token::INT(literal);
                            },
                        }
                    }
                }
            } else {
                // decimal int or float
                self.scan_digits(&mut literal, is_decimal);
                match self.peek_char() {
                    Some('.') => {

                    },
                    _ => {
                        tok = Token::INT(literal);
                    }
                }
            }
        } else {
            self.scan_fraction(&mut literal);
            tok = Token::FLOAT(literal);
        }
        tok
    }

    fn scan_fraction(&mut self, lit: &mut String) {

    }

    fn scan_exponent(&mut self, lit: &mut String) {

    }

    fn scan_digits(&mut self, digits: &mut String, ch_valid: fn(char) -> bool) {
        loop {
            match self.peek_char() {
                Some(&ch) if ch_valid(ch) => {
                  digits.push(ch);
                  self.read_char().unwrap();
                }
                _ => break,
            }
        }
    }

    fn number_start_with(&mut self, ch: char) -> bool {
        ch.is_digit(10) || (ch == '.' && 
            match self.peek_char() {
                Some(c) => c.is_digit(10),
                None => false,
            })
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

fn is_decimal(ch: char) -> bool {
    ch >= '0' && ch <= '9'
}

fn is_octal(ch: char) -> bool {
    ch >= '0' && ch <= '7'
}

fn is_hex(ch: char) -> bool {
    (ch >= '0' && ch <= '9') ||
        (ch.to_ascii_lowercase() >= 'a'&& ch.to_ascii_lowercase() <= 'f')
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_scanner(){

    }
}