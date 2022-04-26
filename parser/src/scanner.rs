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

use super::errors;
use super::position;
use super::token::Token;
use std::iter::Peekable;
use std::path::Path;
use std::str::Chars;

type ErrorHandler = fn(pos: position::Position, msg: &str);

pub struct Scanner<'a> {
    file: &'a mut position::File, // source file handle
    dir: String,                  // directory portion of file.Name()
    src: Peekable<Chars<'a>>,     // source
    errors: &'a errors::ErrorList,

    offset: usize,      // character offset
    line_offset: usize, // current line offset
    semi1: bool,        // insert semicolon if current char is \n
    semi2: bool,        // insert semicolon if followed by \n
}

impl<'a> Scanner<'a> {
    pub fn new(
        file: &'a mut position::File,
        src: &'a str,
        err: &'a errors::ErrorList,
    ) -> Scanner<'a> {
        let dir = Path::new(file.name())
            .parent()
            .unwrap()
            .to_string_lossy()
            .into_owned();
        Scanner {
            file: file,
            dir: dir,
            src: src.chars().peekable(),
            errors: err,
            offset: 0,
            line_offset: 0,
            semi1: false,
            semi2: false,
        }
    }

    fn error(&self, msg: &str) {
        errors::FilePosErrors::new(self.file, self.errors).add_str(self.offset, msg, false);
    }

    fn position(&self) -> position::Position {
        self.file.position(self.pos())
    }

    pub fn pos(&self) -> position::Pos {
        self.file().pos(self.offset)
    }

    // Read the next Unicode char
    #[allow(dead_code)]
    pub fn scan(&mut self) -> (Token, position::Pos) {
        self.semi1 = self.semi2;
        self.semi2 = false;
        self.skip_whitespace();
        let pos = self.file().pos(self.offset);
        let token = match self.peek_char() {
            Some(&ch) if is_letter(ch) => {
                let t = self.scan_identifier();
                match t {
                    Token::BREAK | Token::CONTINUE | Token::FALLTHROUGH | Token::RETURN => {
                        self.semi2 = true
                    }
                    Token::IDENT(_) => self.semi2 = true,
                    _ => {}
                }
                t
            }
            Some(&ch) if is_decimal(ch) => {
                self.semi2 = true;
                self.scan_number(ch)
            }
            // we only reach here if s.semi1 was
            // set in the first place and exited early
            // from skip_whitespace()
            Some('\n') => {
                self.semi1 = false;
                Token::SEMICOLON(false.into())
            }
            Some('"') => {
                self.semi2 = true;
                self.scan_string()
            }
            Some('\'') => {
                self.semi2 = true;
                self.scan_char()
            }
            Some('`') => {
                self.semi2 = true;
                self.scan_raw_string()
            }
            Some(':') => self.scan_switch2(&Token::COLON, &Token::DEFINE).clone(),
            Some('.') => {
                match self.get_char2nd() {
                    Some(ch) if is_decimal(ch) => {
                        self.semi2 = true;
                        self.scan_number('.')
                    }
                    Some('.') => {
                        self.read_char();
                        self.read_char();
                        match self.peek_char() {
                            Some('.') => {
                                self.read_char();
                                Token::ELLIPSIS
                            }
                            _ => {
                                self.semi2 = self.semi1; // preserve insert semi info
                                Token::ILLEGAL("..".to_owned().into())
                            }
                        }
                    }
                    _ => {
                        self.read_char();
                        Token::PERIOD
                    }
                }
            }
            Some(',') => self.scan_token(Token::COMMA, false),
            Some(';') => self.scan_token(Token::SEMICOLON(true.into()), false),
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
                let t = self.scan_switch3(&Token::SUB, &Token::SUB_ASSIGN, '-', &Token::DEC);
                self.semi2 = *t == Token::DEC;
                t.clone()
            }
            Some('*') => self.scan_switch2(&Token::MUL, &Token::MUL_ASSIGN).clone(),
            Some('/') => {
                let ch = self.get_char2nd();
                match ch {
                    Some('/') | Some('*') => {
                        if self.semi1 && self.comment_to_end() {
                            self.semi1 = false;
                            Token::SEMICOLON(false.into())
                        } else {
                            self.semi2 = self.semi1; // preserve insert semi info
                            self.scan_comment(ch.unwrap())
                        }
                    }
                    _ => self.scan_switch2(&Token::QUO, &Token::QUO_ASSIGN).clone(),
                }
            }
            Some('%') => self.scan_switch2(&Token::REM, &Token::REM_ASSIGN).clone(),
            Some('^') => self.scan_switch2(&Token::XOR, &Token::XOR_ASSIGN).clone(),
            Some('<') => match self.get_char2nd() {
                Some('-') => {
                    self.read_char();
                    self.scan_token(Token::ARROW, false)
                }
                _ => self
                    .scan_switch4(
                        &Token::LSS,
                        &Token::LEQ,
                        '<',
                        &Token::SHL,
                        &Token::SHL_ASSIGN,
                    )
                    .clone(),
            },
            Some('>') => self
                .scan_switch4(
                    &Token::GTR,
                    &Token::GEQ,
                    '>',
                    &Token::SHR,
                    &Token::SHR_ASSIGN,
                )
                .clone(),
            Some('=') => self.scan_switch2(&Token::ASSIGN, &Token::EQL).clone(),
            Some('!') => self.scan_switch2(&Token::NOT, &Token::NEQ).clone(),
            Some('&') => match self.get_char2nd() {
                Some('^') => {
                    self.read_char();
                    self.scan_switch2(&Token::AND_NOT, &Token::AND_NOT_ASSIGN)
                        .clone()
                }
                _ => self
                    .scan_switch3(&Token::AND, &Token::AND_ASSIGN, '&', &Token::LAND)
                    .clone(),
            },
            Some('|') => self
                .scan_switch3(&Token::OR, &Token::OR_ASSIGN, '|', &Token::LOR)
                .clone(),
            Some(&c) => {
                self.semi2 = self.semi1; // preserve insert semi info
                self.read_char();
                Token::ILLEGAL(c.to_string().into())
            }
            None => {
                if self.semi1 {
                    self.semi1 = false;
                    Token::SEMICOLON(false.into())
                } else {
                    Token::EOF
                }
            }
        };
        (token, pos)
    }

    fn scan_identifier(&mut self) -> Token {
        let mut s = String::new();
        loop {
            match self.peek_char() {
                Some(&ch) if is_letter(ch) || is_decimal(ch) => {
                    self.advance_and_push(&mut s, ch);
                }
                _ => break,
            }
        }
        Token::ident_token(s)
    }

    fn scan_number(&mut self, ch: char) -> Token {
        let mut tok = self.scan_number_without_i(ch);
        // Handles the 'i' at the end
        match self.peek_char() {
            Some('i') => match tok {
                Token::INT(mut lit) | Token::FLOAT(mut lit) => {
                    self.advance_and_push(lit.as_mut(), 'i');
                    tok = Token::IMAG(lit);
                }
                _ => {}
            },
            _ => {}
        }
        tok
    }

    fn scan_number_without_i(&mut self, ch: char) -> Token {
        let mut literal = String::new();
        if ch == '.' {
            //.34
            self.scan_fraction_and_finish(literal)
        } else if ch == '0' {
            self.advance_and_push(&mut literal, '0');
            match self.peek_char() {
                // hexadecimal int
                Some('x') | Some('X') => self.prefixed_int(literal, IntPrefix::Hex),
                // octal int (explicit)
                Some('o') | Some('O') => self.prefixed_int(literal, IntPrefix::Octal(false)),
                // binary int
                Some('b') | Some('B') => self.prefixed_int(literal, IntPrefix::Binary),
                // octal int (bare)
                Some(ch) if is_decimal(*ch) => self.prefixed_int(literal, IntPrefix::Octal(true)),
                _ => self.decimal_int_or_float(literal),
            }
        } else {
            self.decimal_int_or_float(literal)
        }
    }

    fn prefixed_int(&mut self, mut literal: String, prefix: IntPrefix) -> Token {
        if prefix.is_bare() {
            literal.push(prefix.char());
        } else {
            self.advance_and_push(&mut literal, prefix.char());
        }

        let result = self.scan_digits(&mut literal, prefix.is_valid(), true);
        match result {
            Ok(count) => {
                if count == 0 {
                    self.error(prefix.err_msg());
                    return Token::ILLEGAL(literal.into());
                }
            }
            Err(e) => {
                self.error(e);
                return Token::ILLEGAL(literal.into());
            }
        }
        if let Some(c) = self.peek_char() {
            if matches!(prefix, IntPrefix::Binary | IntPrefix::Octal(_)) && is_decimal(*c) {
                self.error(prefix.err_msg());
                return Token::ILLEGAL(literal.into());
            }
        }
        Token::INT(literal.into())
    }

    fn decimal_int_or_float(&mut self, mut literal: String) -> Token {
        // decimal int or float 3 / 3.14
        if let Err(e) = self.scan_digits(&mut literal, is_decimal, true) {
            self.error(e);
            return Token::ILLEGAL(literal.into());
        }
        match self.peek_char() {
            Some('e') | Some('E') => self.scan_exponent_and_finish(literal),
            Some('.') => self.scan_fraction_and_finish(literal),
            _ => Token::INT(literal.into()),
        }
    }

    fn scan_token(&mut self, t: Token, semi: bool) -> Token {
        self.read_char();
        self.semi2 = semi;
        t
    }

    fn scan_char(&mut self) -> Token {
        let mut lit = String::new();
        if let Some(unquoted) = self.scan_string_char_lit(&mut lit, '\'') {
            Token::CHAR((lit, unquoted.chars().next().unwrap()).into())
        } else {
            Token::ILLEGAL(lit.into())
        }
    }

    fn scan_string(&mut self) -> Token {
        let mut lit = String::new();
        if let Some(unquoted) = self.scan_string_char_lit(&mut lit, '"') {
            Token::STRING((lit, unquoted).into())
        } else {
            Token::ILLEGAL(lit.into())
        }
    }

    fn scan_raw_string(&mut self) -> Token {
        let mut lit = self.read_char().unwrap().to_string();
        let mut unquoted = String::with_capacity(lit.len());
        loop {
            match self.peek_char() {
                Some('`') => {
                    self.advance_and_push(&mut lit, '`');
                    break;
                }
                Some('\r') => {
                    // advance without push
                    self.read_char();
                }
                Some(&ch) => {
                    self.advance_and_push(&mut lit, ch);
                    unquoted.push(ch);
                }
                None => {
                    self.error("raw string literal not terminated");
                    break;
                }
            };
        }
        Token::STRING((lit, unquoted).into())
    }

    fn scan_comment(&mut self, ch: char) -> Token {
        let mut lit = String::new();
        lit.push(self.read_char().unwrap());
        lit.push(self.read_char().unwrap());
        if ch == '/' {
            // //
            loop {
                match self.read_char() {
                    Some('\n') | None => break,
                    Some(c) => {
                        if c != '\r' {
                            lit.push(c);
                        }
                    }
                }
            }
            lit.push('\n');
            Token::COMMENT(lit.into())
        } else {
            // /*
            loop {
                match self.read_char() {
                    Some('*') => match self.peek_char() {
                        Some('/') => {
                            lit.push_str("*/");
                            self.read_char();
                            break;
                        }
                        _ => lit.push('*'),
                    },
                    Some(c) => {
                        if c != '\r' {
                            lit.push(c);
                        }
                    }
                    None => {
                        self.error("comment not terminated");
                        break;
                    }
                }
            }
            Token::COMMENT(lit.into())
        }
    }

    fn scan_string_char_lit(&mut self, lit: &mut String, quote: char) -> Option<String> {
        lit.push(self.read_char().unwrap());
        let mut unquoted = String::with_capacity(lit.len());
        loop {
            match self.peek_char() {
                Some(&ch) if ch == quote => {
                    self.advance_and_push(lit, quote);
                    break;
                }
                Some('\n') | None => {
                    self.error("string/char literal not terminated");
                    return None;
                }
                Some('\\') => {
                    let result = self.scan_escape(lit, quote);
                    if result.is_none() {
                        return None;
                    } else {
                        unquoted.push(result.unwrap());
                    }
                }
                Some(&ch) => {
                    self.advance_and_push(lit, ch);
                    unquoted.push(ch);
                }
            }
        }
        Some(unquoted)
    }

    fn scan_escape(&mut self, lit: &mut String, quote: char) -> Option<char> {
        lit.push(self.read_char().unwrap());

        let mut n: isize;
        let base: u32;
        let max: u32;
        match self.peek_char() {
            Some(&ch) => match ch {
                'a' => {
                    self.advance_and_push(lit, ch);
                    return Some('\u{0007}');
                }
                'b' => {
                    self.advance_and_push(lit, ch);
                    return Some('\u{0008}');
                }
                'f' => {
                    self.advance_and_push(lit, ch);
                    return Some('\u{000c}');
                }
                'n' => {
                    self.advance_and_push(lit, ch);
                    return Some('\n');
                }
                'r' => {
                    self.advance_and_push(lit, ch);
                    return Some('\r');
                }
                't' => {
                    self.advance_and_push(lit, ch);
                    return Some('\t');
                }
                'v' => {
                    self.advance_and_push(lit, ch);
                    return Some('\u{000b}');
                }
                '\\' => {
                    self.advance_and_push(lit, ch);
                    return Some('\u{005c}');
                }
                c if c == quote => {
                    self.advance_and_push(lit, c);
                    return Some(c);
                }
                '0' | '1' | '2' | '3' | '4' | '5' | '6' | '7' => {
                    n = 3;
                    base = 8;
                    max = 255;
                }
                'x' => {
                    self.advance_and_push(lit, 'x');
                    n = 2;
                    base = 16;
                    max = 255;
                }
                'u' => {
                    self.advance_and_push(lit, 'u');
                    n = 4;
                    base = 16;
                    max = std::char::MAX as u32;
                }
                'U' => {
                    self.advance_and_push(lit, 'U');
                    n = 8;
                    base = 16;
                    max = std::char::MAX as u32;
                }
                _ => {
                    self.error("unknown escape sequence");
                    return None;
                }
            },
            None => {
                self.error("escape sequence not terminated");
                return None;
            }
        }

        let mut x: u32 = 0;
        while n > 0 {
            match self.peek_char() {
                Some(&ch) => {
                    let d = digit_val(ch);
                    if d >= base {
                        self.error("illegal character in escape sequence");
                        return None;
                    }
                    self.advance_and_push(lit, ch);
                    x = x * base + d;
                }
                None => {
                    self.error("escape sequence not terminated");
                    return None;
                }
            }
            n -= 1;
        }
        if x <= max {
            let result = std::char::from_u32(x);
            if result.is_none() {
                self.error("escape sequence is invalid Unicode code point");
            }
            result
        } else {
            None
        }
    }

    fn scan_switch2<'b>(&mut self, t1: &'b Token, t2: &'b Token) -> &'b Token {
        self.read_char();
        match self.peek_char() {
            Some(&'=') => {
                self.read_char();
                t2
            }
            _ => t1,
        }
    }

    fn scan_switch3<'b>(
        &mut self,
        t1: &'b Token,
        t2: &'b Token,
        ch: char,
        t3: &'b Token,
    ) -> &'b Token {
        self.read_char();
        match self.peek_char() {
            Some(&'=') => {
                self.read_char();
                t2
            }
            Some(&c) if c == ch => {
                self.read_char();
                t3
            }
            _ => t1,
        }
    }

    fn scan_switch4<'b>(
        &mut self,
        t1: &'b Token,
        t2: &'b Token,
        ch: char,
        t3: &'b Token,
        t4: &'b Token,
    ) -> &'b Token {
        self.read_char();
        match self.peek_char() {
            Some(&'=') => {
                self.read_char();
                t2
            }
            Some(&c) if c == ch => {
                self.read_char();
                match self.peek_char() {
                    Some(&'=') => {
                        self.read_char();
                        t4
                    }
                    _ => t3,
                }
            }
            _ => t1,
        }
    }

    fn scan_fraction_and_finish(&mut self, mut lit: String) -> Token {
        self.advance_and_push(&mut lit, '.');
        if let Err(e) = self.scan_digits(&mut lit, is_decimal, false) {
            self.error(e);
            return Token::ILLEGAL(lit.into());
        }
        match self.peek_char() {
            Some('e') | Some('E') => self.scan_exponent_and_finish(lit),
            _ => Token::FLOAT(lit.into()),
        }
    }

    fn scan_exponent_and_finish(&mut self, mut lit: String) -> Token {
        self.advance_and_push(&mut lit, 'e');
        match self.peek_char() {
            Some(&ch) if ch == '+' || ch == '-' => {
                self.advance_and_push(&mut lit, ch);
            }
            _ => {}
        }
        match self.scan_digits(&mut lit, is_decimal, false) {
            Ok(count) => {
                if count == 0 {
                    self.error("illegal floating-point exponent");
                    Token::ILLEGAL(lit.into())
                } else {
                    Token::FLOAT(lit.into())
                }
            }
            Err(e) => {
                self.error(e);
                Token::ILLEGAL(lit.into())
            }
        }
    }

    fn scan_digits<'s, 'r>(
        &'s mut self,
        digits: &mut String,
        ch_valid: fn(char) -> bool,
        allow_pre_underscore: bool,
    ) -> Result<usize, &'r str> {
        let msg = "_ must separate successive digits";
        if !allow_pre_underscore {
            if let Some(ch) = self.peek_char() {
                if *ch == '_' {
                    return Err(msg);
                }
            }
        }
        let mut count = 0;
        let mut previous_is_underscore = false;
        loop {
            match self.peek_char() {
                Some(&ch) if ch_valid(ch) => {
                    self.advance_and_push(digits, ch);
                    count += 1;
                    previous_is_underscore = false;
                }
                Some('_') => {
                    if !previous_is_underscore {
                        // accept '_' but do not increase count
                        self.advance_and_push(digits, '_');
                        previous_is_underscore = true;
                    } else {
                        return Err(msg);
                    }
                }
                _ => break,
            }
        }
        if !previous_is_underscore {
            Ok(count)
        } else {
            return Err(msg);
        }
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
                    self.file.add_line(self.offset + 1);
                }
                self.offset += 1;
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
                                }
                                _ => {}
                            };
                        }
                        Some(_) => {}
                        None => return true,
                    }
                }
                loop {
                    //skip whitespaces
                    match iter.peek() {
                        Some(' ') | Some('\t') | Some('r') => {
                            iter.next();
                        }
                        _ => break,
                    }
                }
                match iter.peek() {
                    Some('\n') | None => return true,
                    _ => {}
                }
            }
            _ => panic!("should not call into this function"),
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
        c if c >= 'a' && c <= 'f' => ch as u32 - 'a' as u32 + 10,
        c if c >= 'A' && c <= 'F' => ch as u32 - 'A' as u32 + 10,
        _ => 16,
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

fn is_binary(ch: char) -> bool {
    ch == '0' || ch == '1'
}

fn is_hex(ch: char) -> bool {
    (ch >= '0' && ch <= '9') || (ch.to_ascii_lowercase() >= 'a' && ch.to_ascii_lowercase() <= 'f')
}

enum IntPrefix {
    Binary,
    Octal(bool),
    Hex,
}

impl IntPrefix {
    fn is_valid(&self) -> fn(char) -> bool {
        match self {
            Self::Binary => is_binary,
            Self::Octal(_) => is_octal,
            Self::Hex => is_hex,
        }
    }

    fn char(&self) -> char {
        match self {
            Self::Binary => 'b',
            Self::Octal(_) => 'o',
            Self::Hex => 'x',
        }
    }

    fn is_bare(&self) -> bool {
        matches!(self, Self::Octal(true))
    }

    fn err_msg(&self) -> &str {
        match self {
            Self::Binary => "illegal binary number",
            Self::Octal(_) => "illegal octal number",
            Self::Hex => "illegal hexadecimal number",
        }
    }
}

#[cfg(test)]
mod test {
    use super::position::FileSet;
    use super::*;

    fn error_handler(pos: position::Position, msg: &str) {
        print!("scan error at {}: {}\n", pos, msg);
    }

    #[test]
    fn test_scanner() {
        let mut fs = FileSet::new();
        let f = fs.add_file("testfile1.gs".to_owned(), None, 1000);

        //let src = " \"|a string \\t nttttt|\" 'a' 'aa' '\t' `d\\n\r\r\r\naaa` 3.14e6 2.33e-10 025 028 0xaa 0x break\n 333++ > >= ! abc if else 0 ... . .. .23";
        let src = r#"
        1234.
        -1e0+0e0
        0o010078
        0o0100_7
        _333
        333_
        3_3
        3__3
        0b000111
        0b000112
        5e+1
        0.5e-1
        07 08 
        break // ass
        break /*lala
        \la */
        abc
        123
        "slkfdskfl\nsd"
        `d\\n\r\r\r\naaa`
        'a' 'aa' '\t'
        "aaa\nbbb"
        .25
        break
        /*dff"#;
        print!("src {}\n", src);

        let err = errors::ErrorList::new();
        let mut scanner = Scanner::new(f, src, &err);
        loop {
            let (tok, pos) = scanner.scan();
            print!("Token:{:?} --{}-- Pos:{}\n", tok, tok, pos);
            if tok == Token::EOF {
                break;
            }
        }
        print!("\n<- {} ->\n", err);
    }
}
