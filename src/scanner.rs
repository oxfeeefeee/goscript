use std::rc::Rc;
use std::cell::{RefCell};
use std::iter::Peekable;
use std::str::Chars;
use std::path::Path;
use super::errors;
use super::position;
use super::token::Token;

type ErrorHandler = fn(pos: position::Position, msg: &str);

pub struct Scanner<'a> {
    file: &'a mut position::File,  // source file handle
    dir: String,                // directory portion of file.Name()
    src: Peekable<Chars<'a>>,   // source
    errors: Rc<RefCell<errors::ErrorList>>,

    offset: usize,      // character offset
	line_offset: usize, // current line offset
    semi1: bool,        // insert semicolon if current char is \n
    semi2: bool,        // insert semicolon if followed by \n

    error_count: isize,
}

impl<'a> Scanner<'a> {
    pub fn new<'b>(file: &'b mut position::File, src: &'b str,
        err: Rc<RefCell<errors::ErrorList>>) -> Scanner<'b> {
        let dir = Path::new(file.name()).parent().unwrap().
            to_string_lossy().into_owned();
        Scanner{
            file: file,
            dir: dir,
            src: src.chars().peekable(),
            errors: err,
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
        self.errors.borrow_mut().add(p, msg.to_string())
    }

    fn position(&self) -> position::Position {
        self.file.position(self.pos())
    }

    pub fn pos(&self) -> position::Pos {
        self.file().pos(self.offset)
    }

    // Read the next Unicode char 
    #[allow(dead_code)]
    pub fn scan(&mut self) -> Token {
        self.semi1 = self.semi2;
        self.semi2 = false;
        self.skip_whitespace();
        match self.peek_char() {
            Some(&ch) if is_letter(ch) => {
                let t = self.scan_identifier();
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
            Some('\n') => { self.semi1 = false; Token::SEMICOLON(false) },
            Some('"') => { self.semi2 = true; self.scan_string() },
            Some('\'') => { self.semi2 = true; self.scan_char() },
            Some('`') => { self.semi2 = true; self.scan_raw_string() },
            Some(':') => self.scan_switch2(&Token::COLON, &Token::DEFINE).clone(),
            Some('.') => {
                match self.get_char2nd() {
                    Some(ch) if is_decimal(ch) => {
                        self.semi2 = true;
                        self.scan_number('.') 
                    },
                    Some('.') => {
                        self.read_char();
                        self.read_char();
                        match self.peek_char() {
                            Some('.') => { self.read_char(); Token::ELLIPSIS },
                            _ => {
                                self.semi2 = self.semi1; // preserve insert semi info
                                Token::ILLEGAL("..".to_string())
                            }
                        }
                    }
                    _ => { self.read_char(); Token::PERIOD },
                }
            }
            Some(',') => self.scan_token(Token::COMMA, false),
            Some(';') => self.scan_token(Token::SEMICOLON(true), false),
            Some('(') => self.scan_token(Token::LPAREN, false),
            Some(')') => self.scan_token(Token::RPAREN, true),
            Some('[') => self.scan_token(Token::LBRACK, false),
            Some(']') => self.scan_token(Token::RBRACK, true),
            Some('{') => self.scan_token(Token::LBRACE, false),
            Some('}') => self.scan_token(Token::RBRACE, true),
            Some('+') => {
                let t = self.scan_switch3(&Token::ADD, &Token::ADD_ASSIGN, '+', &Token::INC);
                self.semi2 = *t == Token::INC;
                t.clone()
            }
            Some('-') => {
                let t = self.scan_switch3(&Token::SUB, &Token::SUB_ASSIGN, '+', &Token::DEC);
                self.semi2 = *t == Token::DEC;
                t.clone()
            }
            Some('*') => self.scan_switch2(&Token::MUL, &Token::MUL_ASSIGN).clone(),
            Some('/') => {
                let ch = self.get_char2nd();
                match ch  {
                    Some('/') | Some('*') => {
                        if self.semi1 && self.comment_to_end() {
                             self.semi1 = false;
                            Token::SEMICOLON(false)
                        } else {
                            self.scan_comment(ch.unwrap())
                        }
                    },
                    _ => self.scan_switch2(&Token::QUO, &Token::QUO_ASSIGN).clone(),
                }
            }
            Some('%') => self.scan_switch2(&Token::REM, &Token::REM_ASSIGN).clone(),
            Some('^') => self.scan_switch2(&Token::XOR, &Token::XOR_ASSIGN).clone(),
            Some('<') => {
                match self.get_char2nd(){
                    Some('-') => { 
                        self.read_char();
                        self.scan_token(Token::ARROW, false)
                    },
                    _ => self.scan_switch4(&Token::LSS, &Token::LEQ, '<',
                        &Token::SHL, &Token::SHL_ASSIGN).clone(),
                }
            }
            Some('>') => self.scan_switch4(&Token::GTR, &Token::GEQ, '<',
                        &Token::SHR, &Token::SHR_ASSIGN).clone(),
            Some('=') => self.scan_switch2(&Token::ASSIGN, &Token::EQL).clone(),
            Some('!') => self.scan_switch2(&Token::NOT, &Token::NEQ).clone(),
            Some('&') => {
                 match self.get_char2nd(){
                    Some('^') => { 
                        self.read_char();
                        self.scan_switch2(&Token::AND_NOT, &Token::AND_NOT_ASSIGN).clone()
                    },
                    _ => self.scan_switch3(&Token::AND, &Token::AND_ASSIGN,
                        '&', &Token::LAND).clone(),
                 }
            }
            Some('|') => self.scan_switch3(&Token::OR, &Token::OR_ASSIGN,
                        '|', &Token::LOR).clone(),
            Some(&c) => {
                self.semi2 = self.semi1; // preserve insert semi info
                self.read_char();
                Token::ILLEGAL(c.to_string())
            }
            None => {
                if self.semi1 {
                    self.semi1 = false;
                    Token::SEMICOLON(false)
                } else {
                    Token::EOF
                }
            }
        }
    }

    fn scan_identifier(&mut self) -> Token {
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
        let mut tok = self.scan_number_without_i(ch);
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

    fn scan_number_without_i(&mut self, ch: char) -> Token {
        let mut literal = String::new(); 
        if ch == '.' { //.34
            self.append_fraction(&mut literal);
            Token::FLOAT(literal)
        } else if ch == '0' {
            self.advance_and_push(&mut literal, '0');
            match self.peek_char() {
                // hexadecimal int
                Some('x') | Some('X') => {
                    self.advance_and_push(&mut literal, 'x');
                    let count = self.scan_digits(&mut literal, is_hex);
                    if count == 0 {
                        self.error(self.offset, "illegal hexadecimal number");
                        Token::ILLEGAL(literal)
                    } else {
                        Token::INT(literal)
                    }
                },
                _ => {
                    // octal int or float
                    self.scan_digits(&mut literal, is_octal);
                    match self.peek_char() {
                        Some(&c) if (c == '8'|| c == '9') => {
                            self.error(self.offset, "illegal octal number");
                            self.advance_and_push(&mut literal, c);
                            Token::ILLEGAL(literal)
                        },
                        Some('e') | Some('E') => {
                            self.append_exponent(&mut literal);
                            Token::FLOAT(literal)
                        },
                        Some('.') => {
                            self.append_fraction(&mut literal); 
                            Token::FLOAT(literal)
                        },
                        _ => Token::INT(literal),
                    }
                }
            }
        } else {
            // decimal int or float 3 / 3.14
            self.scan_digits(&mut literal, is_decimal);
            match self.peek_char() {
                Some('.') => {
                    self.append_fraction(&mut literal);
                    Token::FLOAT(literal)
                },
                _ => Token::INT(literal)
            }
        }
    }

    fn scan_token(&mut self, t: Token, semi: bool) -> Token {
        self.read_char();
        self.semi2 = semi;
        t
    }

    fn scan_char(&mut self) -> Token {
        let mut lit = String::new();
        let (ok, count) = self.scan_string_char_lit(&mut lit, '\'');
        if ok && count == 1 {
            Token::CHAR(lit)
        } else {
            Token::ILLEGAL(lit)
        }
    }

    fn scan_string(&mut self) -> Token {
        let mut lit = String::new();
        let (ok, _) = self.scan_string_char_lit(&mut lit, '"');
        if ok {
            Token::STRING(lit)
        } else {
            Token::ILLEGAL(lit)
        }
    }

    fn scan_raw_string(&mut self) -> Token {
        let mut lit = self.read_char().unwrap().to_string();
        loop {
            match self.peek_char(){
                Some('`') => {
                    self.advance_and_push(&mut lit, '`');
                    break;
                },
                Some('\r') => {self.read_char();}, // advance without push
                Some(&ch) => {self.advance_and_push(&mut lit, ch);},
                None => {
                    self.error(self.offset, "raw string literal not terminated");
                }
            };
        };
        Token::STRING(lit)
    }

    fn scan_comment(&mut self, ch: char) -> Token {
        let mut lit = String::new();
        lit.push(self.read_char().unwrap());
        lit.push(self.read_char().unwrap());
        if ch == '/' {  // //
            loop {
                match self.read_char() {
                    Some('\n') | None => break,
                    Some(c) => {
                        if c != '\r' {
                            lit.push(c);
                        }
                    },
                }
            }
            lit.push('\n');
            Token::COMMENT(lit)
        } else {        // /*
            loop {
                match self.read_char() {
                    Some('*') => {
                        match self.peek_char() {
                            Some('/') => {
                                lit.push_str("*/");
                                self.read_char();
                                break;
                            },
                            _ => {lit.push('*')},
                        }
                    },
                    Some(c) => {
                        if c != '\r' {
                            lit.push(c);
                        }
                    },
                    None => {
                        break;
                    },
                }
            }
            Token::COMMENT(lit)
        }
    }

    fn scan_string_char_lit(&mut self, lit: &mut String, quote: char) -> (bool, usize) {
        lit.push(self.read_char().unwrap());
        let mut n = 0;
        loop {
            match self.peek_char(){
                Some(&ch) if ch == quote => {
                    self.advance_and_push(lit, quote);
                    break;
                }
                Some('\n') | None => {
                    self.error(self.offset, "string/char literal not terminated");
                    return (false, 0);
                },
                Some('\\') => {
                    if !self.scan_escape(lit, quote) {
                        return (false, 0);
                    }
                    n += 1;
                }
                Some(&ch) => {
                    self.advance_and_push(lit, ch);
                    n += 1;
                }
            }
        }
        (true, n)
    }

    fn scan_escape(&mut self, lit: &mut String, quote: char) -> bool {
        lit.push(self.read_char().unwrap());

        let mut n:isize;
        let base:u32;
        let max:u32;
        match self.peek_char() {
            Some(&ch) => match ch {
                'a' | 'b' | 'f' | 'n' | 'r' | 't' | 'v' | '\\'  => {
                    self.advance_and_push(lit, ch);
                    return true;
                },
                c if c == quote => {
                    self.advance_and_push(lit, c);
                    return true;
                }
                '0' | '1' | '2' | '3' | '4' | '5' | '6' | '7' => {
                    n = 3; base = 8; max = 255;
                }
                'x' => {
                    self.advance_and_push(lit, 'x');
                    n = 2; base = 16; max = 255;
                },
                'u' => {
                    self.advance_and_push(lit, 'u');
                    n = 4; base = 16; max = std::char::MAX as u32;
                },
                'U' => {
                    self.advance_and_push(lit, 'U');
                    n = 8; base = 16; max = std::char::MAX  as u32;
                },
                _ => {
                    self.error(self.offset, "unknown escape sequence");
                    return false;
                },
            }
            None => { 
                self.error(self.offset, "escape sequence not terminated");
                return false;
            }
        }

        let mut x:u32 = 0;
        while n > 0 {
            match self.peek_char() {
                Some(&ch) => {
                    let d = digit_val(ch);
                    if d >= base {
                        self.error(self.offset, "illegal character in escape sequence");
                        return false;
                    }
                    self.advance_and_push(lit, ch);
                    x = x*base + d;
                },
                None => {
                    self.error(self.offset, "escape sequence not terminated");
                    return false;
                }
            }
            n -= 1;
        }
        if x > max || 0xD800 <= x && x < 0xE000 {
            self.error(self.offset, "escape sequence is invalid Unicode code point");
            return false;
        }
        true
    }

    fn scan_switch2<'b>(&mut self, t1: &'b Token, t2: &'b Token) -> &'b Token {
        self.read_char();
        match self.peek_char() {
            Some(&'=') => { self.read_char(); t2 },
            _ => t1,
        }
    }

    fn scan_switch3<'b>(&mut self, t1: &'b Token, t2: &'b Token, 
        ch: char, t3: &'b Token) -> &'b Token {
        self.read_char();
        match self.peek_char() {
            Some(&'=') => { self.read_char(); t2 },
            Some(&c) if c == ch => { self.read_char(); t3 },
            _ => t1,
        }
    }

    fn scan_switch4<'b>(&mut self, t1: &'b Token, t2: &'b Token, 
        ch: char, t3: &'b Token, t4: &'b Token) -> &'b Token {
        self.read_char();
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

    // returns true if line ends with comment:
    fn comment_to_end(&self) -> bool {
        let mut iter = self.src.clone(); // don't touch the main iter
        iter.next(); // eat the first '/'
        match iter.next() {
            // //-style comment always ends a line
            Some('/') => return true,
            Some('*') => {
                loop {
                    match iter.next() {
                        Some('\n') => return true,
                        Some('*') => {
                            match iter.peek() {
                                Some(&'/') => {
                                    iter.next();
                                    break;
                                } ,
                                _ => {},
                            };
                        },
                        Some(_) => {},
                        None => return true, 
                    }
                }
                loop { //skip whitespaces
                    match iter.peek() {
                        Some(' ') | Some('\t') | Some('r') => {iter.next();},
                        _ => break,
                    }
                }
                match iter.peek() {
                    Some('\n') | None => return true,
                    _ => {},
                }
            }
            _ => panic!("should not call into this function")
        }
        return false;
    }

    fn advance_and_push(&mut self, literal: &mut String, ch: char) {
        self.read_char();
        literal.push(ch);
    }

    pub fn file_mut(&mut self) -> &mut position::File {
        self.file
    }

    pub fn file(&self) -> &position::File {
        self.file
    }
}

fn digit_val(ch: char) -> u32 {
    match ch {
        c if c >= '0' && c <= '9' => ch as u32 - '0' as u32,
        c if c >= 'a' && c <= 'f' => ch as u32 - 'a' as u32,
        c if c >= 'A' && c <= 'F' => ch as u32 - 'A' as u32,
        _ => 16
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
    use super::position::{/*File, FileSet, */SharedFileSet};

    fn error_handler(pos: position::Position, msg: &str) {
        print!("scan error at {}: {}\n", pos, msg);
    }

    #[test] 
    fn test_scanner(){
        let fs = SharedFileSet::new();
        let mut fsm = fs.borrow_mut();
        let f = fsm.add_file(fs.weak(), "testfile1.gs", 0, 1000);

        //let src = " \"|a string \\t nttttt|\" 'a' 'aa' '\t' `d\\n\r\r\r\naaa` 3.14e6 2.33e-10 025 028 0xaa 0x break\n 333++ > >= ! abc if else 0 ... . .. .23";
        let src = r#"

        break // ass
        break /*lala
        \la */
        abc
        123
        "slkfdskflsd"
        `d\\n\r\r\r\naaa`
        'a' 'aa' '\t'
        .25
        break
        /*dff"#;
        //let src = ". 123";
        print!("src {}\n", src);

        let err = Rc::new(RefCell::new(errors::ErrorList::new()));
        let mut scanner = Scanner::new(f, src, err);
        loop {
            let tok = scanner.scan();
            let pos = scanner.position();
            print!("Token:{:?} --{}-- Pos:{}\n", tok, tok, pos);
            if tok == Token::EOF {
                break
            }
        }
    }
}