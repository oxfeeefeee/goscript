#![allow(dead_code)]
use super::typ::{BasicDetail, BasicInfo, BasicType};
use goscript_parser::token::Token;
use num_bigint::BigInt;
use num_traits::cast::FromPrimitive;
use num_traits::cast::ToPrimitive;
use num_traits::Num;
use std::borrow::Borrow;
use std::borrow::Cow;
use std::fmt;

/// constant implements Values representing untyped
/// Go constants and their corresponding operations.
///
/// A special Unknown value may be used when a value
/// is unknown due to an error. Operations on unknown
/// values produce unknown values unless specified
/// otherwise.
///
/// Because BigFloat library is not available at the moment(2020/5)
/// float numbers arbitrary precision is not supported for now
/// float numbers is simply represented as f64
/// todo: This is against the Go specs.

/// All the values involved in the evaluation
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Unknown,
    Bool(bool),
    Str(String),
    Int(BigInt),
    Float(f64),
    Complex(f64, f64),
}

impl fmt::Display for Value {
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

    pub fn representable(&self, base: &BasicDetail, rounded: Option<&mut Value>) -> bool {
        if let Value::Unknown = self {
            return true; // avoid follow-up errors
        }
        match base.info() {
            BasicInfo::IsInteger => match self.to_int().borrow() {
                Value::Int(ival) => {
                    if let Some(r) = rounded {
                        *r = Value::Int(ival.clone())
                    }
                    match base.typ() {
                        BasicType::Int => ival.to_isize().is_some(),
                        BasicType::Int8 => ival.to_i8().is_some(),
                        BasicType::Int16 => ival.to_i16().is_some(),
                        BasicType::Int32 => ival.to_i32().is_some(),
                        BasicType::Int64 => ival.to_i64().is_some(),
                        BasicType::Uint | BasicType::Uintptr => ival.to_usize().is_some(),
                        BasicType::Uint8 | BasicType::Byte => ival.to_u8().is_some(),
                        BasicType::Uint16 => ival.to_u16().is_some(),
                        BasicType::Uint32 | BasicType::Rune => ival.to_u32().is_some(),
                        BasicType::Uint64 => ival.to_u64().is_some(),
                        BasicType::UntypedInt => true,
                        _ => unreachable!(),
                    }
                }
                _ => false,
            },
            BasicInfo::IsFloat => match self.to_float() {
                Value::Float(f) => match base.typ() {
                    BasicType::Float64 => true,
                    BasicType::Float32 => {
                        if f.to_f32().is_some() {
                            true
                        } else {
                            if let Some(r) = rounded {
                                *r = Value::Float((f as f32).into());
                            }
                            false
                        }
                    }
                    BasicType::UntypedFloat => true,
                    _ => unreachable!(),
                },
                _ => false,
            },
            BasicInfo::IsComplex => match self.to_complex() {
                Value::Complex(re, im) => match base.typ() {
                    BasicType::Complex128 => true,
                    BasicType::Complex64 => {
                        if re.to_f32().is_some() && im.to_f32().is_some() {
                            true
                        } else {
                            if let Some(r) = rounded {
                                *r = Value::Complex((re as f32).into(), (im as f32).into());
                            }
                            false
                        }
                    }
                    BasicType::UntypedComplex => true,
                    _ => unreachable!(),
                },
                _ => false,
            },
            BasicInfo::IsBoolean => base.info() == BasicInfo::IsBoolean,
            BasicInfo::IsString => base.info() == BasicInfo::IsString,
            _ => false,
        }
    }

    pub fn to_int(&self) -> Cow<Value> {
        let f64_to_int = |x| -> Cow<Value> {
            match BigInt::from_f64(x) {
                Some(v) => Cow::Owned(Value::Int(v)),
                None => Cow::Owned(Value::Unknown),
            }
        };
        match self {
            Value::Int(_) => Cow::Borrowed(self),
            Value::Float(f) => f64_to_int(*f),
            Value::Complex(r, i) => {
                if *i == 0.0 {
                    f64_to_int(*r)
                } else {
                    Cow::Owned(Value::Unknown)
                }
            }
            _ => Cow::Owned(Value::Unknown),
        }
    }

    pub fn to_float(&self) -> Value {
        let v = match self {
            Value::Int(i) => i.to_f64(),
            Value::Float(f) => Some(*f),
            Value::Complex(r, i) => {
                if *i == 0.0 {
                    Some(*r)
                } else {
                    None
                }
            }
            _ => None,
        };
        v.map_or(Value::Unknown, |x| Value::Float(x))
    }

    pub fn to_complex(&self) -> Value {
        let v = match self {
            Value::Int(i) => i.to_f64().map(|x| (x, 0.0)),
            Value::Float(f) => Some((*f, 0.0)),
            Value::Complex(r, i) => Some((*r, *i)),
            _ => None,
        };
        v.map_or(Value::Unknown, |(r, i)| Value::Complex(r, i))
    }

    pub fn as_string(&self) -> String {
        match self {
            Value::Str(s) => quote_str(s),
            _ => self.to_string(),
        }
    }

    /// int_as_u64 returns the Go uint64 value and whether the result is exact;
    pub fn int_as_u64(&self) -> (u64, bool) {
        unimplemented!()
    }

    /// int_as_i64 returns the Go int64 value and whether the result is exact;
    pub fn int_as_i64(&self) -> (i64, bool) {
        unimplemented!()
    }

    /// real returns the real part of x, which must be a numeric or unknown value.
    /// If x is Unknown, the result is Unknown.
    pub fn real(&self) -> Value {
        unimplemented!()
    }

    /// imag returns the imaginary part of x, which must be a numeric or unknown value.
    /// If x is Unknown, the result is Unknown.
    pub fn imag(&self) -> Value {
        unimplemented!()
    }

    /// Sign returns -1, 0, or 1 depending on whether x < 0, x == 0, or x > 0;
    /// x must be numeric or Unknown. For complex values x, the sign is 0 if x == 0,
    /// otherwise it is != 0. If x is Unknown, the result is 1.
    pub fn sign(&self) -> isize {
        unimplemented!()
    }

    /// BinaryOp returns the result of the binary expression x op y.
    /// The operation must be defined for the operands. If one of the
    /// operands is Unknown, the result is Unknown.
    /// BinaryOp doesn't handle comparisons or shifts; use Compare
    /// or Shift instead.
    ///
    /// To force integer division of Int operands, use op == token.QUO_ASSIGN
    /// instead of token.QUO; the result is guaranteed to be Int in this case.
    /// Division by zero leads to a run-time panic.
    pub fn binary_op(x: &Value, op: &Token, y: &Value) -> Value {
        unimplemented!()
    }

    // UnaryOp returns the result of the unary expression op y.
    // The operation must be defined for the operand.
    // If prec > 0 it specifies the ^ (xor) result size in bits.
    // If y is Unknown, the result is Unknown.
    //
    pub fn unary_op(op: &Token, y: &Value, prec: usize) -> Value {
        unimplemented!()
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
