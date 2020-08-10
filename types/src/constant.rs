#![allow(dead_code)]
use super::typ::{BasicDetail, BasicInfo, BasicType};
use goscript_parser::token::Token;
use num_bigint::{BigInt, Sign};
use num_rational::BigRational;
use num_traits::cast::FromPrimitive;
use num_traits::cast::ToPrimitive;
use num_traits::sign::Signed;
use num_traits::Num;
use ordered_float;
use std::borrow::Borrow;
use std::borrow::Cow;
use std::fmt;

type F32 = ordered_float::OrderedFloat<f32>;
type F64 = ordered_float::OrderedFloat<f64>;

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
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Value {
    Unknown,
    Bool(bool),
    Str(String),
    Int(BigInt),
    Rat(BigRational),
    Float(F64),
    Complex(Box<Value>, Box<Value>),
}

impl fmt::Display for Value {
    /// For numeric values, the result may be an approximation;
    /// for String values the result may be a shortened string.
    /// Use ExactString for a string representing a value exactly.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Unknown => write!(f, "unknown"),
            Value::Bool(b) => {
                f.write_str("bool: ")?;
                b.fmt(f)
            }
            Value::Str(s) => {
                f.write_str("string: ")?;
                write!(f, "{}", short_quote_str(s, 72))
            }
            Value::Int(s) => {
                f.write_str("int: ")?;
                s.fmt(f)
            }
            Value::Rat(r) => {
                f.write_str("rat: ")?;
                r.fmt(f)
            }
            Value::Float(s) => {
                f.write_str("float: ")?;
                s.fmt(f)
            }
            Value::Complex(r, i) => {
                f.write_str("complex: ")?;
                write!(f, "({} + {}i)", r, i)
            }
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
        Value::Float(f.into())
    }

    pub fn with_literal(tok: &Token) -> Value {
        match tok {
            Token::INT(ilit) => int_from_literal(ilit.as_str()),
            Token::FLOAT(flit) => float_from_literal(flit.as_str()),
            Token::IMAG(imlit) => {
                let s = imlit.as_str();
                let v = float_from_literal(&s[..(s.len() - 1)]);
                if let Value::Float(_) = &v {
                    Value::Complex(Box::new(Value::with_f64(0.0)), Box::new(v))
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

    pub fn is_int(&self) -> bool {
        match self {
            Value::Int(_) => true,
            Value::Rat(r) => r.is_integer(),
            _ => false,
        }
    }

    pub fn representable(&self, base: &BasicDetail, rounded: Option<&mut Value>) -> bool {
        if let Value::Unknown = self {
            return true; // avoid follow-up errors
        }

        let float_representable =
            |val: &Value, btype: BasicType, rounded: Option<&mut Value>| -> bool {
                match val.to_float() {
                    Value::Float(f) => match btype {
                        BasicType::Float64 => true,
                        BasicType::Float32 => {
                            if f.to_f32().is_some() {
                                true
                            } else {
                                if let Some(r) = rounded {
                                    *r = Value::Float(((*f as f32) as f64).into());
                                }
                                false
                            }
                        }
                        BasicType::UntypedFloat => true,
                        _ => unreachable!(),
                    },
                    _ => false,
                }
            };

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
            BasicInfo::IsFloat => float_representable(self, base.typ(), rounded),
            BasicInfo::IsComplex => {
                let ty = match base.typ() {
                    BasicType::Complex64 => BasicType::Float32,
                    BasicType::Complex128 => BasicType::Float64,
                    BasicType::UntypedComplex => BasicType::UntypedFloat,
                    _ => unreachable!(),
                };
                match self.to_complex() {
                    Value::Complex(r, i) => {
                        let (rrounded, irounded): (Option<&mut Value>, Option<&mut Value>) =
                            match rounded {
                                Some(val) => {
                                    *val = Value::Complex(
                                        Box::new(Value::with_f64(0.0)),
                                        Box::new(Value::with_f64(0.0)),
                                    );
                                    if let Value::Complex(r, i) = &mut *val {
                                        (Some(r.as_mut()), Some(i.as_mut()))
                                    } else {
                                        unreachable!()
                                    }
                                }
                                None => (None, None),
                            };
                        let rok = float_representable(&r, ty, rrounded);
                        let iok = float_representable(&i, ty, irounded);
                        rok && iok
                    }
                    _ => false,
                }
            }
            BasicInfo::IsBoolean => match self {
                Value::Bool(_) => true,
                _ => false,
            },
            BasicInfo::IsString => match self {
                Value::Str(_) => true,
                _ => false,
            },
            _ => false,
        }
    }

    pub fn to_int(&self) -> Cow<Value> {
        let f64_to_int = |x| -> Cow<Value> {
            match BigRational::from_f64(x) {
                Some(v) => {
                    if v.is_integer() {
                        Cow::Owned(Value::Int(v.to_integer()))
                    } else {
                        Cow::Owned(Value::Unknown)
                    }
                }
                None => Cow::Owned(Value::Unknown),
            }
        };
        match self {
            Value::Int(_) => Cow::Borrowed(self),
            Value::Rat(r) => {
                if r.is_integer() {
                    Cow::Owned(Value::Int(r.to_integer()))
                } else {
                    Cow::Owned(Value::Unknown)
                }
            }
            Value::Float(f) => f64_to_int(**f),
            Value::Complex(r, i) => {
                let (ival, ok) = i.to_int().int_as_i64();
                if ok && ival == 0 {
                    r.to_int()
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
            Value::Rat(r) => rat_to_f64(r),
            Value::Float(f) => Some(**f),
            Value::Complex(r, i) => {
                let (ival, ok) = i.to_float().num_as_f64();
                if ok && ival == 0.0 {
                    let (rval, ok) = r.to_float().num_as_f64();
                    if ok {
                        Some(*rval)
                    } else {
                        None
                    }
                } else {
                    None
                }
            }
            _ => None,
        };
        v.map_or(Value::Unknown, |x| Value::Float(x.into()))
    }

    pub fn to_complex(&self) -> Value {
        match self {
            Value::Int(_) | Value::Rat(_) | Value::Float(_) => {
                Value::Complex(Box::new(self.clone()), Box::new(Value::with_f64(0.0)))
            }
            Value::Complex(_, _) => self.clone(),
            _ => Value::Unknown,
        }
    }

    /// real returns the real part of x, which must be a numeric or unknown value.
    /// If x is Unknown, the result is Unknown.
    pub fn real(&self) -> Value {
        match self {
            Value::Int(_) | Value::Float(_) | Value::Rat(_) | Value::Unknown => self.clone(),
            Value::Complex(r, _) => *r.clone(),
            _ => panic!(format!("{} not numeric", self)),
        }
    }

    /// imag returns the imaginary part of x, which must be a numeric or unknown value.
    /// If x is Unknown, the result is Unknown.
    pub fn imag(&self) -> Value {
        match self {
            Value::Int(_) | Value::Float(_) | Value::Rat(_) => Value::with_f64(0.0),
            Value::Complex(_, i) => *i.clone(),
            Value::Unknown => Value::Unknown,
            _ => panic!(format!("{} not numeric", self)),
        }
    }

    /// sign returns -1, 0, or 1 depending on whether x < 0, x == 0, or x > 0;
    /// x must be numeric or Unknown. For complex values x, the sign is 0 if x == 0,
    /// otherwise it is != 0. If x is Unknown, the result is 1.
    pub fn sign(&self) -> isize {
        match self {
            Value::Int(i) => match i.sign() {
                Sign::Plus => 1,
                Sign::Minus => -1,
                Sign::NoSign => 0,
            },
            Value::Rat(r) => {
                if r.is_positive() {
                    1
                } else if r.is_negative() {
                    -1
                } else {
                    0
                }
            }
            Value::Float(v) => {
                let f: f64 = **v;
                if f > 0.0 {
                    1
                } else if f < 0.0 {
                    -1
                } else {
                    0
                }
            }
            Value::Complex(r, i) => r.sign() | i.sign(),
            Value::Unknown => 1, // avoid spurious division by zero errors
            _ => panic!(format!("{} not numeric", self)),
        }
    }

    /// binary_op returns the result of the binary expression x op y.
    /// The operation must be defined for the operands. If one of the
    /// operands is Unknown, the result is Unknown.
    /// binary_op doesn't handle comparisons or shifts; use compare
    /// or shift instead.
    ///
    /// To force integer division of Int operands, use op == Token::QUO_ASSIGN
    /// instead of Token::QUO; the result is guaranteed to be Int in this case.
    /// Division by zero leads to a run-time panic.
    pub fn binary_op(x: &Value, op: &Token, y: &Value) -> Value {
        let add = |x, y| Value::binary_op(x, &Token::ADD, y);
        let sub = |x, y| Value::binary_op(x, &Token::SUB, y);
        let mul = |x, y| Value::binary_op(x, &Token::MUL, y);
        let div = |x, y| Value::binary_op(x, &Token::QUO, y);
        let bx = |x| Box::new(x);

        let (x, y) = Value::match_type(Cow::Borrowed(x), Cow::Borrowed(y));
        match (&*x, &*y) {
            (Value::Unknown, Value::Unknown) => Value::Unknown,
            (Value::Bool(a), Value::Bool(b)) => match op {
                Token::LAND => Value::Bool(*a && *b),
                Token::LOR => Value::Bool(*a || *b),
                _ => unreachable!(),
            },
            (Value::Int(a), Value::Int(b)) => {
                match op {
                    Token::ADD => Value::Int(a + b),
                    Token::SUB => Value::Int(a - b),
                    Token::MUL => Value::Int(a * b),
                    Token::QUO => Value::Rat(BigRational::new(a.clone(), b.clone())),
                    Token::QUO_ASSIGN => Value::Int(a / b), // force integer division
                    Token::REM => Value::Int(a % b),
                    Token::AND => Value::Int(a & b),
                    Token::OR => Value::Int(a | b),
                    Token::XOR => Value::Int(a ^ b),
                    Token::AND_NOT => Value::Int(a & !b),
                    _ => unreachable!(),
                }
            }
            (Value::Rat(a), Value::Rat(b)) => match op {
                Token::ADD => Value::Rat(a + b),
                Token::SUB => Value::Rat(a - b),
                Token::MUL => Value::Rat(a * b),
                Token::QUO => Value::Rat(a / b),
                _ => unreachable!(),
            },
            (Value::Float(a), Value::Float(b)) => match op {
                Token::ADD => Value::Float(*a + *b),
                Token::SUB => Value::Float(*a - *b),
                Token::MUL => Value::Float(*a * *b),
                Token::QUO => Value::Float(*a / *b),
                _ => unreachable!(),
            },
            (Value::Complex(ar, ai), Value::Complex(br, bi)) => match op {
                Token::ADD => Value::Complex(bx(add(ar, br)), bx(add(ai, bi))),
                Token::SUB => Value::Complex(bx(sub(ar, br)), bx(sub(ai, bi))),
                Token::MUL => {
                    let (a, b, c, d) = (ar, ai, br, bi);
                    let ac = mul(&a, &c);
                    let bd = mul(&b, &d);
                    let bc = mul(&b, &c);
                    let ad = mul(&a, &d);
                    Value::Complex(bx(sub(&ac, &bd)), bx(add(&bc, &ad)))
                }
                Token::QUO => {
                    // (ac+bd)/s + i(bc-ad)/s, with s = cc + dd
                    let (a, b, c, d) = (ar, ai, br, bi);
                    let cc = mul(&c, &c);
                    let dd = mul(&d, &d);
                    let s = add(&cc, &dd);
                    let ac = mul(&a, &c);
                    let bd = mul(&b, &d);
                    let acbd = add(&ac, &bd);
                    let bc = mul(&b, &c);
                    let ad = mul(&a, &d);
                    let bcad = sub(&bc, &ad);
                    Value::Complex(bx(div(&acbd, &s)), bx(div(&bcad, &s)))
                }
                _ => unreachable!(),
            },
            (Value::Str(a), Value::Str(b)) => match op {
                Token::ADD => Value::Str(format!("{}{}", a, b)),
                _ => unreachable!(),
            },
            _ => unreachable!(),
        }
    }

    /// unary_op returns the result of the unary expression op y.
    /// The operation must be defined for the operand.
    /// If prec > 0 it specifies the ^ (xor) result size in bits.
    /// If y is Unknown, the result is Unknown.
    pub fn unary_op(op: &Token, y: &Value, prec: usize) -> Value {
        match op {
            Token::ADD => match y {
                Value::Str(_) => unreachable!(),
                _ => y.clone(),
            },
            Token::SUB => match y {
                Value::Unknown => Value::Unknown,
                Value::Int(i) => Value::Int(-i),
                Value::Rat(r) => Value::Rat(-r),
                Value::Float(f) => Value::Float(-(*f)),
                Value::Complex(r, i) => Value::Complex(
                    Box::new(Value::unary_op(op, r, 0)),
                    Box::new(Value::unary_op(op, i, 0)),
                ),
                _ => unreachable!(),
            },
            Token::XOR => match y {
                Value::Unknown => Value::Unknown,
                Value::Int(i) => {
                    let mut v = !i;
                    if prec > 0 {
                        v = v & (!(BigInt::from_i64(-1).unwrap() << prec * 8));
                    }
                    Value::Int(v)
                }
                _ => unreachable!(),
            },
            Token::NOT => match y {
                Value::Unknown => Value::Unknown,
                Value::Bool(b) => Value::Bool(!b),
                _ => unreachable!(),
            },
            _ => unreachable!(),
        }
    }

    /// compare returns the result of the comparison x op y.
    /// The comparison must be defined for the operands.
    /// If one of the operands is Unknown, the result is
    /// false.
    pub fn compare(x: &Value, op: &Token, y: &Value) -> bool {
        let (x, y) = Value::match_type(Cow::Borrowed(x), Cow::Borrowed(y));
        match (&*x, &*y) {
            (Value::Unknown, _) | (_, Value::Unknown) => false,
            (Value::Bool(a), Value::Bool(b)) => match op {
                Token::EQL => a == b,
                Token::NEQ => a != b,
                _ => unreachable!(),
            },
            (Value::Int(a), Value::Int(b)) => match op {
                Token::EQL => a == b,
                Token::NEQ => a != b,
                Token::LSS => a < b,
                Token::LEQ => a <= b,
                Token::GTR => a > b,
                Token::GEQ => a >= b,
                _ => unreachable!(),
            },
            (Value::Rat(a), Value::Rat(b)) => match op {
                Token::EQL => a == b,
                Token::NEQ => a != b,
                Token::LSS => a < b,
                Token::LEQ => a <= b,
                Token::GTR => a > b,
                Token::GEQ => a >= b,
                _ => unreachable!(),
            },
            (Value::Float(a), Value::Float(b)) => match op {
                Token::EQL => a == b,
                Token::NEQ => a != b,
                Token::LSS => a < b,
                Token::LEQ => a <= b,
                Token::GTR => a > b,
                Token::GEQ => a >= b,
                _ => unreachable!(),
            },
            (Value::Complex(ar, ai), Value::Complex(br, bi)) => {
                let r = Value::compare(ar, op, br);
                let i = Value::compare(ai, op, bi);
                match op {
                    Token::EQL => r && i,
                    Token::NEQ => !r || !i,
                    _ => unreachable!(),
                }
            }
            (Value::Str(a), Value::Str(b)) => match op {
                Token::EQL => a == b,
                Token::NEQ => a != b,
                Token::LSS => a < b,
                Token::LEQ => a <= b,
                Token::GTR => a > b,
                Token::GEQ => a >= b,
                _ => unreachable!(),
            },
            _ => unreachable!(),
        }
    }

    // shift returns the result of the shift expression x op s
    // with op == Token::SHL or Token::SHR (<< or >>). x must be
    // an Int or an Unknown. If x is Unknown, the result is Unknown.
    pub fn shift(x: &Value, op: &Token, s: usize) -> Value {
        match x {
            Value::Unknown => Value::Unknown,
            Value::Int(i) => match op {
                Token::SHL => Value::Int(i << s),
                Token::SHR => Value::Int(i >> s),
                _ => unreachable!(),
            },
            _ => unreachable!(),
        }
    }

    pub fn str_as_string(&self) -> String {
        match self {
            Value::Str(s) => quote_str(s),
            Value::Unknown => "".to_string(),
            _ => panic!("not a string"),
        }
    }

    /// int_as_u64 returns the Go uint64 value and whether the result is exact;
    pub fn int_as_u64(&self) -> (u64, bool) {
        match self {
            Value::Int(i) => match i.to_u64() {
                Some(v) => (v, true),
                _ => (std::u64::MAX, false),
            },
            Value::Unknown => (0, false),
            _ => panic!("not an integer"),
        }
    }

    /// int_as_i64 returns the Go int64 value and whether the result is exact;
    pub fn int_as_i64(&self) -> (i64, bool) {
        match self {
            Value::Int(i) => match i.to_i64() {
                Some(v) => (v, true),
                _ => (
                    if self.sign() > 0 {
                        std::i64::MAX
                    } else {
                        std::i64::MIN
                    },
                    false,
                ),
            },
            Value::Unknown => (0, false),
            _ => panic!("not an integer"),
        }
    }

    /// num_as_f64 returns the nearest Go float64 value of x and whether the result is exact;
    /// x must be numeric or an Unknown, but not Complex. For values too small (too close to 0)
    /// to represent as float64, num_as_f64 silently underflows to 0. The result sign always
    /// matches the sign of x, even for 0.
    /// If x is Unknown, the result is (0, false).
    pub fn num_as_f64(&self) -> (F64, bool) {
        match self {
            Value::Int(_) | Value::Rat(_) => {
                let vf = self.to_float();
                if vf == Value::Unknown {
                    (
                        if self.sign() > 0 {
                            std::f64::MAX.into()
                        } else {
                            std::f64::MIN.into()
                        },
                        false,
                    )
                } else {
                    vf.num_as_f64()
                }
            }
            Value::Float(f) => (*f, true),
            Value::Unknown => (0.0.into(), false),
            _ => panic!("not a number"),
        }
    }

    /// num_as_f32 is like num_as_f64 but for float32 instead of float64.
    pub fn num_as_f32(&self) -> (F32, bool) {
        match self {
            Value::Int(_) | Value::Rat(_) => {
                let vf = self.to_float();
                if vf == Value::Unknown {
                    (
                        if self.sign() > 0 {
                            std::f32::MAX.into()
                        } else {
                            std::f32::MIN.into()
                        },
                        false,
                    )
                } else {
                    vf.num_as_f32()
                }
            }
            Value::Float(v) => {
                let min: f64 = std::f32::MIN as f64;
                let max: f64 = std::f32::MAX as f64;
                let f: f64 = v.into_inner();
                if f > min && f < max {
                    ((f as f32).into(), true)
                } else if f < min {
                    (std::f32::MIN.into(), false)
                } else {
                    (std::f32::MAX.into(), false)
                }
            }
            Value::Unknown => (0.0.into(), false),
            _ => panic!("not a number"),
        }
    }

    fn ord(&self) -> usize {
        match self {
            Value::Unknown => 0,
            Value::Bool(_) | Value::Str(_) => 1,
            Value::Int(_) => 2,
            Value::Rat(_) => 3,
            Value::Float(_) => 4,
            Value::Complex(_, _) => 5,
        }
    }

    /// match_type returns the matching representation (same type) with the
    /// smallest complexity for two values x and y. If one of them is
    /// numeric, both of them must be numeric. If one of them is Unknown
    /// both results are Unknown
    fn match_type<'a>(x: Cow<'a, Value>, y: Cow<'a, Value>) -> (Cow<'a, Value>, Cow<'a, Value>) {
        if x.ord() > y.ord() {
            let (y, x) = Value::match_type(y, x);
            return (x, y);
        }
        match &*x {
            Value::Bool(_) | Value::Str(_) | Value::Complex(_, _) => (x, y),
            Value::Int(iv) => match &*y {
                Value::Int(_) => (x, y),
                Value::Rat(_) => (
                    Cow::Owned(Value::Rat(BigRational::new(iv.clone(), 1.into()))),
                    y,
                ),
                Value::Float(_) => match iv.to_f64() {
                    Some(f) => (Cow::Owned(Value::Float(f.into())), y),
                    None => (Cow::Owned(Value::Unknown), Cow::Owned(Value::Unknown)),
                },
                Value::Complex(_, _) => (
                    Cow::Owned(Value::Complex(
                        Box::new(x.into_owned()),
                        Box::new(Value::with_f64(0.0)),
                    )),
                    y,
                ),
                Value::Unknown => (x.clone(), x),
                _ => unreachable!(),
            },
            Value::Rat(rv) => match &*y {
                Value::Rat(_) => (x, y),
                Value::Float(_) => match rat_to_f64(rv) {
                    Some(f) => (Cow::Owned(Value::Float(f.into())), y),
                    None => (Cow::Owned(Value::Unknown), Cow::Owned(Value::Unknown)),
                },
                Value::Complex(_, _) => (
                    Cow::Owned(Value::Complex(
                        Box::new(x.into_owned()),
                        Box::new(Value::with_f64(0.0)),
                    )),
                    y,
                ),
                Value::Unknown => (x.clone(), x),
                _ => unreachable!(),
            },
            Value::Float(_) => match &*y {
                Value::Float(_) => (x, y),
                Value::Complex(_, _) => (
                    Cow::Owned(Value::Complex(
                        Box::new(x.into_owned()),
                        Box::new(Value::with_f64(0.0)),
                    )),
                    y,
                ),
                Value::Unknown => (x.clone(), x),
                _ => unreachable!(),
            },
            Value::Unknown => (x.clone(), x),
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

fn rat_to_f64(r: &BigRational) -> Option<f64> {
    match (r.numer().to_f64(), r.denom().to_f64()) {
        (Some(n), Some(d)) => Some(n / d),
        _ => None,
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
