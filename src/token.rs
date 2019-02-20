use std::fmt;
use std::collections::HashMap;

#[derive(Hash, Eq, PartialEq, Clone, Debug)]
pub enum Token {
	// Special tokens
	ILLEGAL,
	EOF,
	COMMENT,

	// Identifiers and basic type literals
	IDENT(String),   // main
	INT(i64),        // 12345
	FLOAT(f64),      // 123.45
	IMAG(String),    // 123.45i
	CHAR(i32),       // 'a'
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

impl fmt::Display for Token {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		match self {
			
		}
	}
}