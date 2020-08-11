#![allow(non_camel_case_types)]
use std::fmt;

pub const LOWEST_PREC: usize = 0; // non-operators
pub const UNARY_PREC: usize = 6;
pub const HIGHEST_PREC: usize = 7;

#[derive(Hash, Eq, PartialEq, Clone)]
pub enum Token {
	// Special tokens
	NONE,
	ILLEGAL(TokenData),
	EOF,
	COMMENT(TokenData),

	// Identifiers and basic type literals
	IDENT(TokenData),  // main
	INT(TokenData),    // 12345
	FLOAT(TokenData),  // 123.45
	IMAG(TokenData),   // 123.45i
	CHAR(TokenData),   // 'a'
	STRING(TokenData), // "abc"
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

	RPAREN,               // )
	RBRACK,               // ]
	RBRACE,               // }
	SEMICOLON(TokenData), // ; true if SEMICOLON is NOT inserted by scanner
	COLON,                // :

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
			Token::NONE => (TokenType::Other, "NONE"),
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
			Token::LBRACE => (TokenType::Operator, "{"),
			Token::COMMA => (TokenType::Operator, ","),
			Token::PERIOD => (TokenType::Operator, "."),
			Token::RPAREN => (TokenType::Operator, ")"),
			Token::RBRACK => (TokenType::Operator, "]"),
			Token::RBRACE => (TokenType::Operator, "}"),
			Token::SEMICOLON(_) => (TokenType::Operator, ";"),
			Token::COLON => (TokenType::Operator, ":"),
			Token::BREAK => (TokenType::Keyword, "break"),
			Token::CASE => (TokenType::Keyword, "case"),
			Token::CHAN => (TokenType::Keyword, "chan"),
			Token::CONST => (TokenType::Keyword, "const"),
			Token::CONTINUE => (TokenType::Keyword, "continue"),
			Token::DEFAULT => (TokenType::Keyword, "default"),
			Token::DEFER => (TokenType::Keyword, "defer"),
			Token::ELSE => (TokenType::Keyword, "else"),
			Token::FALLTHROUGH => (TokenType::Keyword, "fallthrough"),
			Token::FOR => (TokenType::Keyword, "for"),
			Token::FUNC => (TokenType::Keyword, "func"),
			Token::GO => (TokenType::Keyword, "go"),
			Token::GOTO => (TokenType::Keyword, "goto"),
			Token::IF => (TokenType::Keyword, "if"),
			Token::IMPORT => (TokenType::Keyword, "import"),
			Token::INTERFACE => (TokenType::Keyword, "interface"),
			Token::MAP => (TokenType::Keyword, "map"),
			Token::PACKAGE => (TokenType::Keyword, "package"),
			Token::RANGE => (TokenType::Keyword, "range"),
			Token::RETURN => (TokenType::Keyword, "return"),
			Token::SELECT => (TokenType::Keyword, "select"),
			Token::STRUCT => (TokenType::Keyword, "struct"),
			Token::SWITCH => (TokenType::Keyword, "switch"),
			Token::TYPE => (TokenType::Keyword, "type"),
			Token::VAR => (TokenType::Keyword, "var"),
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
			_ => Token::IDENT(ident.into()),
		}
	}

	pub fn int1() -> Token {
		Token::INT("1".to_string().into())
	}

	pub fn precedence(&self) -> usize {
		match self {
			Token::LOR => 1,
			Token::LAND => 2,
			Token::EQL | Token::NEQ | Token::LSS | Token::LEQ | Token::GTR | Token::GEQ => 3,
			Token::ADD | Token::SUB | Token::OR | Token::XOR => 4,
			Token::MUL
			| Token::QUO
			| Token::REM
			| Token::SHL
			| Token::SHR
			| Token::AND
			| Token::AND_NOT => 5,
			_ => LOWEST_PREC,
		}
	}

	pub fn text(&self) -> &str {
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
			Token::INT(l) => l.as_str(),
			Token::FLOAT(l) => l.as_str(),
			Token::IMAG(l) => l.as_str(),
			Token::CHAR(l) => l.as_str(),
			Token::STRING(l) => l.as_str(),
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
		let text = self.text();
		match self {
			Token::IDENT(l)
			| Token::INT(l)
			| Token::FLOAT(l)
			| Token::IMAG(l)
			| Token::CHAR(l)
			| Token::STRING(l) => f.write_str(l.as_str()),
			_ => write!(f, "{}", text),
		}
	}
}

impl fmt::Debug for Token {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		let text = self.text();
		match self {
			Token::IDENT(l)
			| Token::INT(l)
			| Token::FLOAT(l)
			| Token::IMAG(l)
			| Token::CHAR(l)
			| Token::STRING(l) => write!(f, "{} {}", text, l.as_str()),
			Token::SEMICOLON(real) if !*real.as_bool() => write!(f, "\"{}(inserted)\"", text),
			token if token.is_operator() || token.is_keyword() => write!(f, "\"{}\"", text),
			_ => write!(f, "{}", text),
		}
	}
}

#[derive(Hash, Eq, PartialEq, Clone, Debug)]
enum RawTokenData {
	Bool(bool),
	Str(String),
	StrStr(String, String),
	StrChar(String, char),
}

#[derive(Hash, Eq, PartialEq, Clone, Debug)]
pub struct TokenData(Box<RawTokenData>);

impl From<bool> for TokenData {
	fn from(b: bool) -> Self {
		TokenData(Box::new(RawTokenData::Bool(b)))
	}
}

impl From<String> for TokenData {
	fn from(s: String) -> Self {
		TokenData(Box::new(RawTokenData::Str(s)))
	}
}

impl From<(String, String)> for TokenData {
	fn from(ss: (String, String)) -> Self {
		TokenData(Box::new(RawTokenData::StrStr(ss.0, ss.1)))
	}
}

impl From<(String, char)> for TokenData {
	fn from(ss: (String, char)) -> Self {
		TokenData(Box::new(RawTokenData::StrChar(ss.0, ss.1)))
	}
}

impl AsRef<bool> for TokenData {
	fn as_ref(&self) -> &bool {
		self.as_bool()
	}
}

impl AsRef<String> for TokenData {
	fn as_ref(&self) -> &String {
		self.as_str()
	}
}

impl AsMut<String> for TokenData {
	fn as_mut(&mut self) -> &mut String {
		self.as_str_mut()
	}
}

impl TokenData {
	pub fn as_bool(&self) -> &bool {
		match self.0.as_ref() {
			RawTokenData::Bool(b) => b,
			_ => unreachable!(),
		}
	}

	pub fn as_str(&self) -> &String {
		match self.0.as_ref() {
			RawTokenData::Str(s) => s,
			RawTokenData::StrStr(s, _) => s,
			RawTokenData::StrChar(s, _) => s,
			_ => unreachable!(),
		}
	}

	pub fn as_str_mut(&mut self) -> &mut String {
		match self.0.as_mut() {
			RawTokenData::Str(s) => s,
			RawTokenData::StrStr(s, _) => s,
			RawTokenData::StrChar(s, _) => s,
			_ => unreachable!(),
		}
	}

	pub fn as_str_str(&self) -> (&String, &String) {
		match self.0.as_ref() {
			RawTokenData::StrStr(s1, s2) => (s1, s2),
			_ => unreachable!(),
		}
	}

	pub fn as_str_char(&self) -> (&String, &char) {
		match self.0.as_ref() {
			RawTokenData::StrChar(s, c) => (s, c),
			_ => unreachable!(),
		}
	}
}

#[cfg(test)]
mod test {
	use super::*;

	#[test]
	fn token_test() {
		print!(
			"testxxxxx \n{}\n{}\n{}\n{}\n. ",
			Token::ILLEGAL("asd".to_string().into()),
			Token::SWITCH,
			Token::IDENT("some_var".to_string().into()),
			Token::FLOAT("3.14".to_string().into()),
		);
	}
}
