#![allow(dead_code)]
use super::types::{
    ChannelKey, ClosureKey, FunctionKey, InterfaceKey, MapKey, Objects, SliceKey, StringKey,
    StructKey,
};
use std::collections::HashMap;
use std::hash::{Hash, Hasher};

#[derive(Clone, Copy, Debug)]
pub enum GosValue {
    Nil,
    Bool(bool),
    Int(i64),
    Float(f64),
    Interface(InterfaceKey),
    Str(StringKey),
    Closure(ClosureKey),
    Slice(SliceKey),
    Map(MapKey),
    Struct(StructKey),
    Channel(ChannelKey),
    Function(FunctionKey),
}

impl PartialEq for GosValue {
    fn eq(&self, other: &GosValue) -> bool {
        match (self, other) {
            (GosValue::Nil, GosValue::Nil) => true,
            (GosValue::Bool(x), GosValue::Bool(y)) => x == y,
            (GosValue::Int(x), GosValue::Int(y)) => x == y,
            (GosValue::Float(x), GosValue::Float(y)) => f64_eq(&x, &y),
            (GosValue::Str(x), GosValue::Str(y)) => x == y,
            (GosValue::Slice(x), GosValue::Slice(y)) => x == y,
            _ => false,
        }
    }
}

impl Default for GosValue {
    fn default() -> Self {
        GosValue::Nil
    }
}

impl Eq for GosValue {}

impl Hash for GosValue {
    fn hash<H: Hasher>(&self, state: &mut H) {
        match self {
            GosValue::Nil => {
                0.hash(state);
            }
            GosValue::Bool(b) => {
                b.hash(state);
            }
            GosValue::Int(i) => {
                i.hash(state);
            }
            GosValue::Float(f) => f64_hash(&f, state),
            /*GosValue::Str(s) => {s.data.hash(state);}
            GosValue::Slice(s) => {s.as_ref().borrow().hash(state);}
            GosValue::Map(_) => {unreachable!()}
            GosValue::Struct(s) => {
                for item in s.as_ref().borrow().iter() {
                    item.hash(state);
                }}
            GosValue::Interface(i) => {i.as_ref().hash(state);}
            GosValue::Closure(_) => {unreachable!()}
            GosValue::Channel(s) => {s.as_ref().borrow().hash(state);}*/
            _ => unreachable!(),
        }
    }
}

impl GosValue {
    pub fn new_str(s: String, o: &mut Objects) -> GosValue {
        o.new_string(s)
    }

    #[inline]
    pub fn get_int(&self) -> i64 {
        if let GosValue::Int(i) = self {
            *i
        } else {
            unreachable!();
        }
    }

    #[inline]
    pub fn get_bool(&self) -> bool {
        if let GosValue::Bool(b) = self {
            *b
        } else {
            unreachable!();
        }
    }

    #[inline]
    pub fn get_function(&self) -> &FunctionKey {
        if let GosValue::Function(f) = self {
            f
        } else {
            unreachable!();
        }
    }
}

#[derive(Hash, Eq, PartialEq, Clone, Debug)]
pub struct ChannelObj {}

fn f64_eq(x: &f64, y: &f64) -> bool {
    match (x, y) {
        (a, b) if a.is_nan() && b.is_nan() => true,
        (a, _) if a.is_nan() => false,
        (_, b) if b.is_nan() => false,
        (a, b) => a == b,
    }
}

fn f64_hash<H: Hasher>(f: &f64, state: &mut H) {
    match f {
        x if x.is_nan() => {
            "NAN".hash(state);
        }
        x => {
            x.to_bits().hash(state);
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use std::collections::hash_map::DefaultHasher;
    use std::mem;
    fn calculate_hash<T: Hash>(t: &T) -> u64 {
        let mut s = DefaultHasher::new();
        t.hash(&mut s);
        s.finish()
    }

    #[test]
    fn test_types() {
        let mut o = Objects::new();
        let t1: Vec<GosValue> = vec![
            GosValue::new_str("Norway".to_string(), &mut o),
            GosValue::Int(100),
            GosValue::new_str("Denmark".to_string(), &mut o),
            GosValue::Int(10),
        ];

        let t2: Vec<GosValue> = vec![
            GosValue::new_str("Norway".to_string(), &mut o),
            GosValue::Int(100),
            GosValue::new_str("Denmark".to_string(), &mut o),
            GosValue::Int(10),
        ];
        /*let a = GosValue::new_slice(t1);
        let b = GosValue::new_slice(t2);
        let c = GosValue::new_slice(vec![a.clone(), b.clone(), GosValue::Int(999)]);
        let d = GosValue::new_slice(vec![a.clone(), b.clone(), GosValue::Int(999)]);

        //let c = b.clone();

        println!("types {}-{}-{}\n",
            calculate_hash(&c),
            calculate_hash(&d),
            mem::size_of::<GosValue>());

        assert!((a == b) == (calculate_hash(&a) == calculate_hash(&b)));
        assert!((c == d) == (calculate_hash(&c) == calculate_hash(&d)));
        assert!(GosValue::Nil == GosValue::Nil);
        assert!(GosValue::Nil != a);
        assert!(GosValue::Int(1) == GosValue::Int(1));
        assert!(GosValue::Int(1) != GosValue::Int(2));
        assert!(GosValue::Float(1.0) == GosValue::Float(1.0));
        assert!(GosValue::Float(std::f64::NAN) == GosValue::Float(std::f64::NAN));
        assert!(GosValue::Float(std::f64::NAN) != GosValue::Float(1.0));
        assert!(GosValue::Float(0.0) == GosValue::Float(-0.0));
        assert!(GosValue::new_str("aaa".to_string()) == GosValue::new_str("aaa".to_string()));
        let s1 = GosValue::new_str("aaa".to_string());
        let s2 = s1.clone();
        assert!(s1 == s2);

        //let i = GosValue::Interface(Box::new(GosValue::Nil));
        */
    }
}
