#![allow(non_camel_case_types)]
use std::fmt;

#[derive(Hash, Eq, PartialEq, Clone, Debug)]
pub enum Token {
	// Special tokens
	ILLEGAL(String),
	EOF,
	COMMENT(String),

	// Identifiers and basic type literals
	IDENT(String),   // main
	INT(String),        // 12345
	FLOAT(String),      // 123.45
	IMAG(String),    // 123.45i
	CHAR(String),       // 'a'
	STRING(String),  // "abc"
	
	// Operator
	ADD, // +
	SUB, // -
	MUL, // *
	QUO, // /
	REM, // %

	AND,     // &
	OR,      // |
	XOR,     // ^
	SHL,     // <<
	SHR,     // >>
	AND_NOT, // &^

	ADD_ASSIGN, // +=
	SUB_ASSIGN, // -=
	MUL_ASSIGN, // *=
	QUO_ASSIGN, // /=
	REM_ASSIGN, // %=

	AND_ASSIGN,     // &=
	OR_ASSIGN,      // |=
	XOR_ASSIGN,     // ^=
	SHL_ASSIGN,     // <<=
	SHR_ASSIGN,     // >>=
	AND_NOT_ASSIGN, // &^=

	LAND,  // &&
	LOR,   // ||
	ARROW, // <-
	INC,   // ++
	DEC,   // --

	EQL,    // ==
	LSS,    // <
	GTR,    // >
	ASSIGN, // =
	NOT,    // !

	NEQ,      // !=
	LEQ,      // <=
	GEQ,      // >=
	DEFINE,   // :=
	ELLIPSIS, // ...

	LPAREN, // (
	LBRACK, // [
	LBRACE, // {
	COMMA,  // ,
	PERIOD, // .

	RPAREN,    // )
	RBRACK,    // ]
	RBRACE,    // }
	SEMICOLON(bool), // ; true if SEMICOLON is NOT inserted by scanner
	COLON,     // :

	// Keywords
	BREAK,
	CASE,
	CHAN,
	CONST,
	CONTINUE,

	DEFAULT,
	DEFER,
	ELSE,
	FALLTHROUGH,
	FOR,

	FUNC,
	GO,
	GOTO,
	IF,
	IMPORT,

	INTERFACE,
	MAP,
	PACKAGE,
	RANGE,
	RETURN,

	SELECT,
	STRUCT,
	SWITCH,
	TYPE,
	VAR,
}

pub enum TokenType {
	Literal,
	Operator,
	Keyword,
	Other,
}

impl Token {
	pub fn token_property(&self) -> (TokenType, &str) {
		match self {
			Token::ILLEGAL(s) => (TokenType::Other, s),
			Token::EOF => (TokenType::Other, "EOF"),
			Token::COMMENT(s) => (TokenType::Other, s),
			Token::IDENT(indent) => (TokenType::Literal, indent),
			Token::INT(istr) => (TokenType::Literal, istr),
			Token::FLOAT(f) => (TokenType::Literal, f),
			Token::IMAG(im) => (TokenType::Literal, im),
			Token::CHAR(chstr) => (TokenType::Literal, chstr),
			Token::STRING(s) => (TokenType::Literal, s),
			Token::ADD => (TokenType::Operator, "+"), 
			Token::SUB => (TokenType::Literal, "-"), 
			Token::MUL => (TokenType::Literal, "*"), 
			Token::QUO => (TokenType::Literal, "/"), 
			Token::REM => (TokenType::Literal, "%"), 
			Token::AND => (TokenType::Literal, "&"),
			Token::OR => (TokenType::Literal, "|"),
			Token::XOR => (TokenType::Literal, "^"),
			Token::SHL => (TokenType::Literal, "<<"),
			Token::SHR => (TokenType::Literal, ">>"),
			Token::AND_NOT => (TokenType::Literal, "&^"), 
			Token::ADD_ASSIGN => (TokenType::Literal, "+="),
			Token::SUB_ASSIGN => (TokenType::Literal, "-="),
			Token::MUL_ASSIGN => (TokenType::Literal, "*="), 
			Token::QUO_ASSIGN => (TokenType::Literal, "/="), 
			Token::REM_ASSIGN => (TokenType::Literal, "%="), 
			Token::AND_ASSIGN => (TokenType::Literal, "&="),  
			Token::OR_ASSIGN => (TokenType::Literal, "|="), 
			Token::XOR_ASSIGN => (TokenType::Literal, "^="), 
			Token::SHL_ASSIGN => (TokenType::Literal, "<<="), 
			Token::SHR_ASSIGN => (TokenType::Literal, ">>="), 
			Token::AND_NOT_ASSIGN => (TokenType::Literal, "&^="),
			Token::LAND => (TokenType::Literal, "&&"),
			Token::LOR => (TokenType::Literal, "||"),
			Token::ARROW => (TokenType::Literal, "<-"),
			Token::INC => (TokenType::Literal, "++"),
			Token::DEC => (TokenType::Literal, "--"),
			Token::EQL => (TokenType::Literal, "=="),
			Token::LSS => (TokenType::Literal, "<"),
			Token::GTR => (TokenType::Literal, ">"),
			Token::ASSIGN => (TokenType::Literal, "="),
			Token::NOT => (TokenType::Literal, "!"),
			Token::NEQ => (TokenType::Literal, "!="),
			Token::LEQ => (TokenType::Literal, "<="),
			Token::GEQ => (TokenType::Literal, ">="),
			Token::DEFINE => (TokenType::Literal, ":="),
			Token::ELLIPSIS => (TokenType::Literal, "..."),
			Token::LPAREN => (TokenType::Literal, "("),
			Token::LBRACK => (TokenType::Literal, "["),
			Token::LBRACE => (TokenType::Literal, "{{"),
			Token::COMMA => (TokenType::Literal, ","),
			Token::PERIOD => (TokenType::Literal, "."),
			Token::RPAREN => (TokenType::Literal, ")"),
			Token::RBRACK => (TokenType::Literal, "]"),
			Token::RBRACE => (TokenType::Literal, "}}"),
			Token::SEMICOLON(_) => (TokenType::Literal, ";"),
			Token::COLON => (TokenType::Literal, ":"),
			Token::BREAK => (TokenType::Keyword, "break"),
			Token::CASE => (TokenType::Keyword,"case"),
			Token::CHAN => (TokenType::Keyword,"chan"),
			Token::CONST => (TokenType::Keyword,"const"),
			Token::CONTINUE => (TokenType::Keyword,"continue"),
			Token::DEFAULT => (TokenType::Keyword,"default"),
			Token::DEFER => (TokenType::Keyword,"defer"),
			Token::ELSE => (TokenType::Keyword,"else"),
			Token::FALLTHROUGH => (TokenType::Keyword,"fallthrough"),
			Token::FOR => (TokenType::Keyword,"for"),
			Token::FUNC => (TokenType::Keyword,"func"),
			Token::GO => (TokenType::Keyword,"go"),
			Token::GOTO => (TokenType::Keyword,"goto"),
			Token::IF => (TokenType::Keyword,"if"),
			Token::IMPORT => (TokenType::Keyword,"import"),
			Token::INTERFACE => (TokenType::Keyword,"interface"),
			Token::MAP => (TokenType::Keyword,"map"),
			Token::PACKAGE => (TokenType::Keyword,"package"),
			Token::RANGE => (TokenType::Keyword,"range"),
			Token::RETURN => (TokenType::Keyword,"return"),
			Token::SELECT => (TokenType::Keyword,"select"),
			Token::STRUCT => (TokenType::Keyword,"struct"),
			Token::SWITCH => (TokenType::Keyword,"switch"),
			Token::TYPE => (TokenType::Keyword,"type"),
			Token::VAR => (TokenType::Keyword,"var"),
		}
	}

	pub fn ident_token(ident: String) -> Token {
		match ident.as_str() {
			"break" => Token::BREAK,
			"case" => Token::CASE,
			"chan" => Token::CHAN,
			"const" => Token::CONST,
			"continue" => Token::CONTINUE,
			"default" => Token::DEFAULT,
			"defer" => Token::DEFER,
			"else" => Token::ELSE,
			"fallthrough" => Token::FALLTHROUGH,
			"for" => Token::FOR,
			"func" => Token::FUNC,
			"go" => Token::GO,
			"goto" => Token::GOTO,
			"if" => Token::IF,
			"import" => Token::IMPORT,
			"interface" => Token::INTERFACE,
			"map" => Token::MAP,
			"package" => Token::PACKAGE,
			"range" => Token::RANGE,
			"return" => Token::RETURN,
			"select" => Token::SELECT,
			"struct" => Token::STRUCT,
			"switch" => Token::SWITCH,
			"type" => Token::TYPE,
			"var" => Token::VAR,
			_ => Token::IDENT(ident),
		}
	}

	pub fn token_text(&self) -> &str {
		let (_, t) = self.token_property();
		t
	}

	pub fn is_literal(&self) -> bool {
		match self.token_property().0 {
			TokenType::Literal => true,
			_ => false,
		}
	}

	pub fn is_operator(&self) -> bool {
		match self.token_property().0 {
			TokenType::Operator => true,
			_ => false,
		}
	}

	pub fn is_keyword(&self) -> bool {
		match self.token_property().0 {
			TokenType::Keyword => true,
			_ => false,
		}
	}
}

impl fmt::Display for Token {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		write!(f, "{}", self.token_text())
	}
}

#[cfg(test)]
mod test {
	use super::*;

	#[test]
	fn token_test () {
		print!("testxxxxx {}. ", Token::ILLEGAL(String::new()));
	}
}