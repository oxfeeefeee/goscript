#![allow(dead_code)]
/// [FGOSF] constant implements Values representing untyped
/// Go constants and their corresponding operations.
///
/// A special Unknown value may be used when a value
/// is unknown due to an error. Operations on unknown
/// values produce unknown values unless specified
/// otherwise.
///
/// Because BigFloat is not available at the moment(2020/5)
/// When BigRational is not able to hold the float value,
/// a f64 is used, and the returned value will be marked as
/// nonprecise.
/// todo: This is against the Go specs.
use super::util::{quote_str, short_quote_str};
use goscript_parser::token::Token;
use num_bigint::BigInt;
use num_rational::BigRational;
use num_traits::cast::FromPrimitive;
use std::fmt;

enum IntVal {
    I64(i64),
    Big(BigInt),
}

impl fmt::Display for IntVal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            IntVal::I64(i) => i.fmt(f),
            IntVal::Big(b) => b.fmt(f),
        }
    }
}

enum FloatVal {
    F64(f64),
    Big(BigRational),
    Nonprecise(f64),
}

impl fmt::Display for FloatVal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FloatVal::F64(fl) => fl.fmt(f),
            FloatVal::Big(b) => b.fmt(f),
            FloatVal::Nonprecise(fl) => write!(f, "{}(onprecise)", fl),
        }
    }
}

/// All the values involved in the evaluation
pub enum Value {
    Unknown,
    Bool(bool),
    Str(String),
    Int(IntVal),
    Float(FloatVal),
    Complex(FloatVal, FloatVal),
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
        Value::Int(IntVal::I64(i))
    }

    pub fn with_u64(u: u64) -> Value {
        let ival = if u < 1 << 63 {
            IntVal::I64(u as i64)
        } else {
            IntVal::Big(BigInt::from_u64(u).unwrap())
        };
        Value::Int(ival)
    }

    pub fn with_f64(f: f64) -> Value {
        if f.is_infinite() || f.is_nan() {
            Value::Unknown
        } else if f == 0.0 {
            // convert -0 to 0
            Value::with_i64(0)
        } else {
            Value::Float(FloatVal::Big(BigRational::from_f64(f).unwrap()))
        }
    }

    pub fn with_literal(lit: &String, tok: &Token) -> Value {
        Value::Unknown
    }

    /// [FGOSF] ExactString returns an exact, quoted (human-readable) form of the value.
    /// If the Value is of Kind String, use StringVal to obtain the unquoted string.
    fn string(&self) -> String {
        match self {
            Value::Str(s) => quote_str(s),
            _ => self.to_string(),
        }
    }
}
