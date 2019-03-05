use std::fmt;

#[derive(Hash, Eq, PartialEq, Clone, Debug)]
pub enum Token {
	// Special tokens
	ILLEGAL,
	EOF,
	COMMENT,

	// Identifiers and basic type literals
	IDENT(String),   // main
	INT(String, i64),        // 12345
	FLOAT(String),      // 123.45
	IMAG(String),    // 123.45i
	CHAR(String, char),       // 'a'
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
	SEMICOLON, // ;
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

impl Token {
	pub fn token_text(&self) -> &str {
		match self {
			Token::ILLEGAL => "ILLEGAL",
			Token::EOF => "EOF",
			Token::COMMENT => "COMMENT",
			Token::IDENT(indent) => indent,
			Token::INT(istr, _) => istr,
			Token::FLOAT(f) => f,
			Token::IMAG(im) => im,
			Token::CHAR(chstr, _) => chstr,
			Token::STRING(s) => s,
			Token::ADD => "+", 
			Token::SUB => "-", 
			Token::MUL => "*", 
			Token::QUO => "/", 
			Token::REM => "%", 
			Token::AND => "&",
			Token::OR => "|",
			Token::XOR => "^",
			Token::SHL => "<<",
			Token::SHR => ">>",
			Token::AND_NOT => "&^", 
			Token::ADD_ASSIGN => "+=",
			Token::SUB_ASSIGN => "-=",
			Token::MUL_ASSIGN => "*=", 
			Token::QUO_ASSIGN => "/=", 
			Token::REM_ASSIGN => "%=", 
			Token::AND_ASSIGN => "&=",  
			Token::OR_ASSIGN => "|=", 
			Token::XOR_ASSIGN => "^=", 
			Token::SHL_ASSIGN => "<<=", 
			Token::SHR_ASSIGN => ">>=", 
			Token::AND_NOT_ASSIGN => "&^=",
			Token::LAND => "&&",
			Token::LOR => "||",
			Token::ARROW => "<-",
			Token::INC => "++",
			Token::DEC => "--",
			Token::EQL => "==",
			Token::LSS => "<",
			Token::GTR => ">",
			Token::ASSIGN => "=",
			Token::NOT => "!",
			Token::NEQ => "!=",
			Token::LEQ => "<=",
			Token::GEQ => ">=",
			Token::DEFINE => ":=",
			Token::ELLIPSIS => "...",
			Token::LPAREN => "(",
			Token::LBRACK => "[",
			Token::LBRACE => "{{",
			Token::COMMA => ",",
			Token::PERIOD => ".",
			Token::RPAREN => ")",
			Token::RBRACK => "]",
			Token::RBRACE => "}}",
			Token::SEMICOLON => ";",
			Token::COLON => ":",
			Token::BREAK => "break",
			Token::CASE => "case",
			Token::CHAN => "chan",
			Token::CONST => "const",
			Token::CONTINUE => "continue",
			Token::DEFAULT => "default",
			Token::DEFER => "defer",
			Token::ELSE => "else",
			Token::FALLTHROUGH => "fallthrough",
			Token::FOR => "for",
			Token::FUNC => "func",
			Token::GO => "go",
			Token::GOTO => "goto",
			Token::IF => "if",
			Token::IMPORT => "import",
			Token::INTERFACE => "interface",
			Token::MAP => "map",
			Token::PACKAGE => "package",
			Token::RANGE => "range",
			Token::RETURN => "return",
			Token::SELECT => "select",
			Token::STRUCT => "struct",
			Token::SWITCH => "switch",
			Token::TYPE => "type",
			Token::VAR => "var",
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
}

impl fmt::Display for Token {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		let n = self.token_text();
		write!(f, "{}", n)
	}
}

#[cfg(test)]
mod test {
	use super::*;

	#[test]
	fn token_test () {
		print!("testxxxxx {}. ", Token::ILLEGAL);
	}
}