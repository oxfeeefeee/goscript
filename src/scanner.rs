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
    semi1: bool,        // insert semicolon if current char is \n
    semi2: bool,        // insert semicolon if followed by \n

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
            semi1: false,
            semi2: false,
            error_count: 0,
        }
    }

    fn error(&mut self, pos: usize, msg: &str) {
        self.error_count += 1;
        let p = self.file.position(pos);
        (self.err)(p, msg);
    }

    fn position(&self) -> position::Position {
        self.file.position(self.offset)
    }

    // Read the next Unicode char 
    #[allow(dead_code)]
    fn scan(&mut self) -> Token {
        self.semi1 = self.semi2;
        self.semi2 = false;
        self.skip_whitespace();
        match self.peek_char() {
            Some(&ch) if is_letter(ch) => {
                let t = self.scan_identifier(ch);
                match t {
                    Token::BREAK | Token::CONTINUE | Token::FALLTHROUGH | Token::RETURN =>
                        self.semi2 = true,
                    Token::IDENT(_) => self.semi2 = true,
                    _ => {}
                }
                t
            },
            Some(&ch) if is_decimal(ch) => { self.semi2 = true; self.scan_number(ch) },
            // we only reach here if s.insertSemi was
            // set in the first place and exited early
            // from skip_whitespace()
            Some(&'\n') => { self.semi1 = false; Token::SEMICOLON },
            Some(&'"') => { self.semi2 = true; self.scan_string() },
            Some(&'\'') => { self.semi2 = true; self.scan_char() },
            Some(&'`') => { self.semi2 = true; self.scan_raw_string() },
            Some(&':') => {
                self.read_char();
                self.switch2(&Token::COLON, &Token::DEFINE).clone()
            }
            Some(&'.') => {
                match self.get_char2nd() {
                    Some(ch) if is_decimal(ch) => {
                        self.semi2 = true;
                        self.scan_number('.') 
                    },
                    Some('.') => {
                        self.read_char();
                        self.read_char();
                        match self.peek_char() {
                            Some(&'.') => { self.read_char(); Token::ELLIPSIS },
                            _ => { 
                                self.semi2 = self.semi1; // preserve insert semi info
                                self.read_char();
                                Token::ILLEGAL 
                            }, 
                        }
                    }
                    _ => { self.read_char(); Token::PERIOD },
                }
            }
            Some(_) => { 
                self.semi2 = self.semi1; // preserve insert semi info
                self.read_char();
                Token::ILLEGAL 
            }, 
            None => {
                if self.semi1 {
                    self.semi1 = false;
                    Token::SEMICOLON
                } else {
                    Token::EOF
                }
            }
        }
    }

    fn scan_identifier(&mut self, ch: char) -> Token {
        let mut s = String::new(); 
        loop {
            match self.peek_char() {
                Some(&ch) if is_letter(ch) || is_decimal(ch) => {
                    self.advance_and_push(&mut s, ch);
                },
                _ => break,
            }
        }
        Token::ident_token(s)
    }

    fn scan_number(&mut self, ch: char) -> Token {
        let mut tok = Token::ILLEGAL;
        let mut literal = String::new(); 
        if ch == '.' { //.34
            self.append_fraction(&mut literal);
            tok = Token::FLOAT(literal);
        } else if ch == '0' {
            self.advance_and_push(&mut literal, '0');
            match self.peek_char() {
                // hexadecimal int
                Some('x') | Some('X') => {
                    self.advance_and_push(&mut literal, 'x');
                    let count = self.scan_digits(&mut literal, is_hex);
                    if count == 0 {
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
                            self.append_exponent(&mut literal);
                            tok = Token::FLOAT(literal);
                        },
                        Some('.') => {
                            self.append_fraction(&mut literal); 
                            tok = Token::FLOAT(literal);
                        },
                        _ => {
                            tok = Token::INT(literal);
                        },
                    }
                }
            }
        } else {
            // decimal int or float 3 / 3.14
            self.scan_digits(&mut literal, is_decimal);
            match self.peek_char() {
                Some('.') => {
                    self.append_fraction(&mut literal);
                    tok = Token::FLOAT(literal);
                },
                _ => {
                    tok = Token::INT(literal);
                }
            }
        }
        // Handles the 'i' at the end
        match self.peek_char() {
            Some('i') => {
                match tok {
                    Token::INT(mut lit) | Token::FLOAT(mut lit) => {
                        self.advance_and_push(&mut lit, 'i');
                        tok = Token::IMAG(lit); 
                    },
                    _ => {},
                }
            },
            _ => {},
        }
        tok
    }

    fn scan_string(&mut self) -> Token {
        Token::STRING(String::new())
    }

    fn scan_raw_string(&mut self) -> Token {
        Token::STRING(String::new())
    }

    fn scan_char(&mut self) -> Token {
        Token::CHAR(String::new())
    }

    fn switch2<'b>(&mut self, t1: &'b Token, t2: &'b Token) -> &'b Token {
        match self.peek_char() {
            Some(&'=') => { self.read_char(); t2 },
            _ => t1,
        }
    }

    fn switch3<'b>(&mut self, t1: &'b Token, t2: &'b Token, 
        ch: char, t3: &'b Token) -> &'b Token {
        match self.peek_char() {
            Some(&'=') => { self.read_char(); t2 },
            Some(&c) if c == ch => { self.read_char(); t3 },
            _ => t1,
        }
    }

    fn switch4<'b>(&mut self, t1: &'b Token, t2: &'b Token, 
        ch: char, t3: &'b Token, t4: &'b Token) -> &'b Token {
        match self.peek_char() {
            Some(&'=') => { self.read_char(); t2 },
            Some(&c) if c == ch => 
                match self.peek_char() {
                    Some(&'=') => { self.read_char(); t4 },
                    _ => t3,
                }
            _ => t1,
        }
    }

    fn append_fraction(&mut self, lit: &mut String) {
        self.advance_and_push(lit, '.');
        self.scan_digits(lit, is_decimal);
        match self.peek_char() {
            Some('e') | Some('E') => {
                self.append_exponent(lit);
            },
            _ => {},
        }
    }

    fn append_exponent(&mut self, lit: &mut String) {
        self.advance_and_push(lit, 'e');
        match self.peek_char(){
            Some(&ch) if ch == '+' || ch == '-' => {
                self.advance_and_push(lit, ch);
            },
            _ => {},
        }
        let count = self.scan_digits(lit, is_decimal);
        if count == 0 {
            self.error(self.offset, "illegal floating-point exponent")
        }
    }

    fn scan_digits(&mut self, digits: &mut String, ch_valid: fn(char) -> bool) -> usize {
        let mut count = 0;
        loop {
            match self.peek_char() {
                Some(&ch) if ch_valid(ch) => {
                    self.advance_and_push(digits, ch);
                    count += 1;
                }
                _ => break,
            }
        }
        count
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
            if ch == ' ' || ch == '\t' || ch == '\n' && !self.semi1 || ch == '\r' {
                self.read_char();
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

    fn get_char2nd(&mut self) -> Option<char> {
        let mut iter = self.src.clone();
        iter.next();
        iter.next()
    }

    fn advance_and_push(&mut self, literal: &mut String, ch: char) {
        self.read_char();
        literal.push(ch);
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
    use super::position::{File, FileSet};
    use std::rc::Rc;
    use std::rc::Weak;
    use std::cell::RefCell;

    fn error_handler(pos: position::Position, msg: &str) {
        print!("scan error at {}: {}", pos, msg);
    }

    #[test]
    fn test_scanner(){
        let fs = FileSet::new_rrc();
        FileSet::add_file(fs.clone(), "testfile1.gs", 0, 1000);
        let mut fsm = fs.borrow_mut();
        let f = fsm.recent_file().unwrap();

        let src = "3.14e6 2.33e-10 025 0xaa break\n 333 abc 0 ... . .. .23";
        //let src = ". 123";
        let mut scanner = Scanner::new(f, src, error_handler);
        loop {
            let tok = scanner.scan();
            let pos = scanner.position();
            print!("Token:{} Pos:{}\n", tok, pos);
            if tok == Token::EOF {
                break
            }
        }
    }
}