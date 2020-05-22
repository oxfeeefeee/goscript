#![allow(dead_code)]
use goscript_parser::token::Token;
use num_bigint::BigInt;
use num_traits::cast::FromPrimitive;
use num_traits::Num;
use std::borrow::Cow;
use std::fmt;

/// [FGOSF-] constant implements Values representing untyped
/// Go constants and their corresponding operations.
///
/// A special Unknown value may be used when a value
/// is unknown due to an error. Operations on unknown
/// values produce unknown values unless specified
/// otherwise. [-FGOSF]
///
/// Because BigFloat library is not available at the moment(2020/5)
/// float numbers arbitrary precision is not supported for now
/// float numbers is simply represented as f64
///
/// todo: This is against the Go specs.

/// All the values involved in the evaluation
pub enum Value {
    Unknown,
    Bool(bool),
    Str(String),
    Int(BigInt),
    Float(f64),
    Complex(f64, f64),
}

impl fmt::Display for Value {
    /// [FGOSF] fmt returns a short, quoted (human-readable) form of the value.
    /// For numeric values, the result may be an approximation;
    /// for String values the result may be a shortened string.
    /// Use ExactString for a string representing a value exactly.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Unknown => write!(f, "unknown"),
            Value::Bool(b) => b.fmt(f),
            Value::Str(s) => write!(f, "{}", short_quote_str(s, 72)),
            Value::Int(s) => s.fmt(f),
            Value::Float(s) => s.fmt(f),
            Value::Complex(r, i) => write!(f, "({} + {}i)", r, i.to_string()),
        }
    }
}

impl Value {
    pub fn with_bool(b: bool) -> Value {
        Value::Bool(b)
    }

    pub fn with_str(s: String) -> Value {
        Value::Str(s)
    }

    pub fn with_i64(i: i64) -> Value {
        Value::Int(BigInt::from_i64(i).unwrap())
    }

    pub fn with_u64(u: u64) -> Value {
        Value::Int(BigInt::from_u64(u).unwrap())
    }

    pub fn with_f64(f: f64) -> Value {
        Value::Float(f)
    }

    pub fn with_literal(tok: &Token) -> Value {
        match tok {
            Token::INT(ilit) => int_from_literal(ilit.as_str()),
            Token::FLOAT(flit) => float_from_literal(flit.as_str()),
            Token::IMAG(imlit) => {
                if let Value::Float(f) =
                    float_from_literal(&imlit.as_str()[..(imlit.as_str().len() - 2)])
                {
                    Value::Complex(0.0, f)
                } else {
                    Value::Unknown
                }
            }
            Token::CHAR(clit) => {
                let (_, ch) = clit.as_str_char();
                Value::with_i64(*ch as i64)
            }
            Token::STRING(slit) => {
                let (_, s) = slit.as_str_str();
                Value::with_str(s.clone())
            }
            _ => Value::Unknown,
        }
    }

    fn string(&self) -> String {
        match self {
            Value::Str(s) => quote_str(s),
            _ => self.to_string(),
        }
    }
}

// ----------------------------------------------------------------------------
// utilities

pub fn quote_str(s: &str) -> String {
    //todo: really works the same as the Go version? does it matter?
    s.escape_default().collect()
}

pub fn short_quote_str(s: &str, max: usize) -> String {
    let result = s.escape_default().collect();
    shorten_with_ellipsis(result, max)
}

pub fn int_from_literal(lit: &str) -> Value {
    let result = if lit.starts_with("0x") {
        BigInt::from_str_radix(&lit[2..], 16)
    } else if lit.starts_with("0o") {
        BigInt::from_str_radix(&lit[2..], 10)
    } else if lit.starts_with("0b") {
        BigInt::from_str_radix(&lit[2..], 2)
    } else {
        BigInt::from_str_radix(lit, 10)
    };
    match result {
        Ok(i) => Value::Int(i),
        Err(_) => Value::Unknown,
    }
}

pub fn float_from_literal(lit: &str) -> Value {
    match lit.parse::<f64>() {
        Ok(f) => Value::with_f64(f),
        Err(_) => Value::Unknown,
    }
}

fn shorten_with_ellipsis(s: String, max: usize) -> String {
    if s.len() <= max {
        s
    } else {
        let mut buf: Vec<char> = s.chars().collect();
        buf = buf[0..(buf.len() - 3)].to_vec();
        buf.append(&mut "...".to_owned().chars().collect());
        buf.into_iter().collect()
    }
}

#[cfg(test)]
mod test {
    #[test]
    fn test_str_unquote() {
        let s = "\\111";
        dbg!(s);
    }
}
