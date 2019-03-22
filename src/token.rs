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
			Token::ILLEGAL(_) => (TokenType::Other, "ILLEGAL"),
			Token::EOF => (TokenType::Other, "EOF"),
			Token::COMMENT(_) => (TokenType::Other, "COMMENT"),
			Token::IDENT(_) => (TokenType::Literal, "IDENT"),
			Token::INT(_) => (TokenType::Literal, "INT"),
			Token::FLOAT(_) => (TokenType::Literal, "FLOAT"),
			Token::IMAG(_) => (TokenType::Literal, "IMAG"),
			Token::CHAR(_) => (TokenType::Literal, "CHAR"),
			Token::STRING(_) => (TokenType::Literal, "STRING"),
			Token::ADD => (TokenType::Operator, "+"), 
			Token::SUB => (TokenType::Operator, "-"), 
			Token::MUL => (TokenType::Operator, "*"), 
			Token::QUO => (TokenType::Operator, "/"), 
			Token::REM => (TokenType::Operator, "%"), 
			Token::AND => (TokenType::Operator, "&"),
			Token::OR => (TokenType::Operator, "|"),
			Token::XOR => (TokenType::Operator, "^"),
			Token::SHL => (TokenType::Operator, "<<"),
			Token::SHR => (TokenType::Operator, ">>"),
			Token::AND_NOT => (TokenType::Operator, "&^"), 
			Token::ADD_ASSIGN => (TokenType::Operator, "+="),
			Token::SUB_ASSIGN => (TokenType::Operator, "-="),
			Token::MUL_ASSIGN => (TokenType::Operator, "*="), 
			Token::QUO_ASSIGN => (TokenType::Operator, "/="), 
			Token::REM_ASSIGN => (TokenType::Operator, "%="), 
			Token::AND_ASSIGN => (TokenType::Operator, "&="),  
			Token::OR_ASSIGN => (TokenType::Operator, "|="), 
			Token::XOR_ASSIGN => (TokenType::Operator, "^="), 
			Token::SHL_ASSIGN => (TokenType::Operator, "<<="), 
			Token::SHR_ASSIGN => (TokenType::Operator, ">>="), 
			Token::AND_NOT_ASSIGN => (TokenType::Operator, "&^="),
			Token::LAND => (TokenType::Operator, "&&"),
			Token::LOR => (TokenType::Operator, "||"),
			Token::ARROW => (TokenType::Operator, "<-"),
			Token::INC => (TokenType::Operator, "++"),
			Token::DEC => (TokenType::Operator, "--"),
			Token::EQL => (TokenType::Operator, "=="),
			Token::LSS => (TokenType::Operator, "<"),
			Token::GTR => (TokenType::Operator, ">"),
			Token::ASSIGN => (TokenType::Operator, "="),
			Token::NOT => (TokenType::Operator, "!"),
			Token::NEQ => (TokenType::Operator, "!="),
			Token::LEQ => (TokenType::Operator, "<="),
			Token::GEQ => (TokenType::Operator, ">="),
			Token::DEFINE => (TokenType::Operator, ":="),
			Token::ELLIPSIS => (TokenType::Operator, "..."),
			Token::LPAREN => (TokenType::Operator, "("),
			Token::LBRACK => (TokenType::Operator, "["),
			Token::LBRACE => (TokenType::Operator, "{{"),
			Token::COMMA => (TokenType::Operator, ","),
			Token::PERIOD => (TokenType::Operator, "."),
			Token::RPAREN => (TokenType::Operator, ")"),
			Token::RBRACK => (TokenType::Operator, "]"),
			Token::RBRACE => (TokenType::Operator, "}}"),
			Token::SEMICOLON(_) => (TokenType::Operator, ";"),
			Token::COLON => (TokenType::Operator, ":"),
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

	pub fn get_literal(&self) -> &str {
		match self {
			Token::INT(l) => l,
			Token::FLOAT(l) => l,
			Token::IMAG(l) => l,
			Token::CHAR(l) => l,
			Token::STRING(l) => l,
			_ => "",
		}
	}

	pub fn is_stmt_start(&self) -> bool {
		match self {
			Token::BREAK => true,
			Token::CONST => true,
			Token::CONTINUE => true,
			Token::DEFER => true,
			Token::FALLTHROUGH => true,
			Token::FOR => true,
			Token::GO => true,
			Token::GOTO => true,
			Token::IF => true,
			Token::RETURN => true,
			Token::SELECT => true,
			Token::SWITCH => true,
			Token::TYPE => true,
			Token::VAR => true,
			_ => false,
		}
	}

	pub fn is_decl_start(&self) -> bool {
		match self {
			Token::CONST => true,
			Token::TYPE => true,
			Token::VAR => true,
			_ => false,
		}
	}

	pub fn is_expr_end(&self) -> bool {
		match self {
			Token::COMMA => true,
			Token::COLON => true,
			Token::SEMICOLON(_) => true,
			Token::RPAREN => true,
			Token::RBRACK => true,
			Token::RBRACE => true,
			_ => false,
		}
	}

}

impl fmt::Display for Token {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		let text = self.token_text();
		match self {
			Token::IDENT(l) => write!(f, "{} {}", text, l),
			Token::INT(l) => write!(f, "{} {}", text, l),
			Token::FLOAT(l) => write!(f, "{} {}", text, l),
			Token::IMAG(l) => write!(f, "{} {}", text, l),
			Token::CHAR(l) => write!(f, "{} {}", text, l),
			Token::STRING(l) => write!(f, "{} {}", text, l),
			Token::SEMICOLON(real) if !real => 
				write!(f, "\"{}(inserted)\"", text),
			token if token.is_operator() || token.is_keyword() => 
				write!(f, "\"{}\"", text),
			_ => write!(f, "{}", text),
		}
	}
}

#[cfg(test)]
mod test {
	use super::*;

	#[test]
	fn token_test () {
		print!("testxxxxx \n{}\n{}\n{}\n{}\n. ", 
			Token::ILLEGAL("asd".to_string()),
			Token::SWITCH,
			Token::IDENT("some_var".to_string()),
			Token::FLOAT("3.14".to_string()),
			);
	}
}