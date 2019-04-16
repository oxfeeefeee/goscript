#![allow(dead_code)]
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::cell::{RefCell, Ref};
use std::rc::{Rc,Weak};
use super::proto::Closure;

#[derive(Clone, Debug)]
pub enum GosValue {
    Nil,
    Bool(bool),
    Int(i64),
    Float(f64),
    Interface(Box<GosValue>),
    Str(Rc<String>),
    WeakStr(Weak<String>),
    Closure(Rc<Closure>),
    WeakClosure(Rc<Closure>),
    Slice(Rc<RefCell<SliceObj>>),
    WeakSlice(Weak<RefCell<SliceObj>>),
    Map(Rc<RefCell<HashMap<GosValue, GosValue>>>),
    WeakMap(Weak<RefCell<HashMap<GosValue, GosValue>>>),
    Struct(Rc<RefCell<Vec<GosValue>>>),
    WeakStruct(Weak<RefCell<Vec<GosValue>>>),
    Channel(Rc<RefCell<ChannelObj>>),
    WeakChannel(Weak<RefCell<ChannelObj>>),
}

impl PartialEq for GosValue {
    fn eq(&self, other: &GosValue) -> bool {
        let refx = self.try_upgrade();
        let refy = other.try_upgrade();
        let x = if self.is_weak() {&refx} else {self};
        let y = if other.is_weak() {&refy} else {other};
        match (x, y) {
            (GosValue::Nil, GosValue::Nil) => true,
            (GosValue::Bool(x), GosValue::Bool(y)) => x == y,
            (GosValue::Int(x), GosValue::Int(y)) => x == y,
            (GosValue::Float(x), GosValue::Float(y)) => f64_eq(&x, &y),
            (GosValue::Str(x), GosValue::Str(y)) => x == y,
            (GosValue::Slice(x), GosValue::Slice(y)) => x == y,
            (GosValue::Map(x), GosValue::Map(y)) => x == y,
            (GosValue::Struct(x), GosValue::Struct(y)) => x == y,
            (GosValue::Interface(x), GosValue::Interface(y)) => x == y,
            (GosValue::Closure(x), GosValue::Closure(y)) => Rc::ptr_eq(x, y),
            (GosValue::Channel(x), GosValue::Channel(y)) => x == y,
            _ => false,
        }
    }
}

impl Eq for GosValue {}

impl Hash for GosValue {
    fn hash<H: Hasher>(&self, state: &mut H) {
        let refx = self.try_upgrade();
        let x = if self.is_weak() {&refx} else {self};
        match x {
            GosValue::Nil => {0.hash(state);}
            GosValue::Bool(b) => {b.hash(state);}
            GosValue::Int(i) => {i.hash(state);}
            GosValue::Float(f) => {f64_hash(&f, state)}
            GosValue::Str(s) => {s.as_ref().hash(state);}
            GosValue::Slice(s) => {s.as_ref().borrow().hash(state);}
            GosValue::Map(_) => {unreachable!()}
            GosValue::Struct(s) => {
                for item in s.as_ref().borrow().iter() {
                    item.hash(state);
                }}
            GosValue::Interface(i) => {i.as_ref().hash(state);}
            GosValue::Closure(_) => {unreachable!()}
            GosValue::Channel(s) => {s.as_ref().borrow().hash(state);}
            _ => unreachable!()
        }
    }
}

impl GosValue {
    pub fn new_str(s: String) -> GosValue {
        GosValue::Str(Rc::new(s))
    }

    pub fn new_slice(s: Vec<GosValue>) -> GosValue {
        let len = s.len();
        GosValue::Slice(Rc::new(RefCell::new(SliceObj{
            data: Rc::new(RefCell::new(s)),
            begin: 0,
            end: len,
        })))
    }

    pub fn new_map(m: HashMap<GosValue, GosValue>) -> GosValue {
        GosValue::Map(Rc::new(RefCell::new(m)))
    }

    pub fn new_struct(s: Vec<GosValue>) -> GosValue {
        GosValue::Struct(Rc::new(RefCell::new(s)))
    }

    pub fn is_ref(&self) -> bool {
        match self {
            GosValue::Str(_) => true,
            GosValue::Closure(_) => true,
            GosValue::Slice(_) => true,
            GosValue::Map(_) => true,
            GosValue::Struct(_) => true,
            GosValue::Channel(_) => true,
            _ => false,
        }
    }

    pub fn is_weak(&self) -> bool {
        match self {
            GosValue::WeakStr(_) => true,
            GosValue::WeakClosure(_) => true,
            GosValue::WeakSlice(_) => true,
            GosValue::WeakMap(_) => true,
            GosValue::WeakStruct(_) => true,
            GosValue::WeakChannel(_) => true,
            _ => false,
        }
    }

    pub fn try_upgrade(&self) -> GosValue {
        if self.is_weak() {
            self.upgrade()
        } else {
            GosValue::Nil
        }
    }

    pub fn try_downgrade(&self) -> GosValue {
        if self.is_ref() {
            self.downgrade()
        } else {
            GosValue::Nil
        }
    }

    // panic if self is not weak ref
    pub fn upgrade(&self) -> GosValue {
        match self {
            GosValue::WeakSlice(weak) => match weak.upgrade() {
                Some(rf) => GosValue::Slice(rf),
                None => GosValue::Nil,
            },
            GosValue::WeakMap(weak) => match weak.upgrade() {
                Some(rf) => GosValue::Map(rf),
                None => GosValue::Nil,
            },
            GosValue::WeakStruct(weak) => match weak.upgrade() {
                Some(rf) => GosValue::Struct(rf),
                None => GosValue::Nil,
            },
            GosValue::WeakChannel(weak) => match weak.upgrade() {
                Some(rf) => GosValue::Channel(rf),
                None => GosValue::Nil,
            },
            _ => unreachable!()
        }
    }

    // panic if self is not ref
    pub fn downgrade(&self) -> GosValue {
         match self {
            GosValue::Slice(s) => GosValue::WeakSlice(Rc::downgrade(s)),
            GosValue::Map(m) => GosValue::WeakMap(Rc::downgrade(m)),
            GosValue::Struct(s) => GosValue::WeakStruct(Rc::downgrade(s)),
            GosValue::Channel(c) => GosValue::WeakChannel(Rc::downgrade(c)),
            _ => unreachable!()
        }
    }

    #[inline]
    pub fn get_int(&self) -> i64 {
        if let GosValue::Int(i) = self {*i} else {unreachable!();}
    }
}

#[derive(Clone, Debug)]
pub struct SliceObj {
    pub data: Rc<RefCell<Vec<GosValue>>>,
    pub begin: usize,
    pub end: usize,
}

impl SliceObj {
    pub fn len(&self) -> usize {
        self.end - self.begin
    }

    pub fn iter(&self) -> SliceObjIter {
        SliceObjIter{
            slice: self,
            cur: self.begin,
        }
    }

    pub fn data_ref(&self) -> Ref<Vec<GosValue>> {
        self.data.as_ref().borrow()
    }

    pub fn get_item(&self, i: usize) -> Ref<GosValue> {
        Ref::map(self.data_ref(), |x| &x[i+self.begin])
    }

    pub fn set_item(&self, i: usize, val: GosValue) {
        self.data.as_ref().borrow_mut()[i] = val;
    }
}

impl PartialEq for SliceObj {
    fn eq(&self, other: &SliceObj) -> bool {
        if self.len() != other.len() {
            return false
        }
        for (i, val) in self.iter().enumerate() {
            if *val != *other.get_item(i) {
                return false
            }
        }
        return true
    }
}

impl Hash for SliceObj {
    fn hash<H: Hasher>(&self, state: &mut H) {
        if self.len() == 0 {
            "empty".hash(state);
        } else {
            for v in self.iter() {
                v.hash(state)
            }
        }
    }
}

pub struct SliceObjIter<'a> {
    slice: &'a SliceObj,
    cur: usize
}

impl<'a> Iterator for SliceObjIter<'a> {
    type Item = Ref<'a, GosValue>;

    fn next(&mut self) -> Option<Ref<'a, GosValue>> {
        if self.cur < self.slice.end {
            self.cur += 1;
            Some(self.slice.get_item(self.cur - 1))
        } else {
            None
        }
    }
}

#[derive(Hash, Eq, PartialEq, Clone, Debug)]
pub struct ChannelObj {
}

fn f64_eq(x: &f64, y: &f64) -> bool {
    match (x, y) {
        (a, b) if a.is_nan() && b.is_nan() => true,
        (a, _) if a.is_nan() => false,
        (_, b) if b.is_nan() => false,
        (a, b) => a == b,
    }
}

fn f64_hash<H: Hasher>(f :&f64, state: &mut H) {
    match f {
        x if x.is_nan() => {"NAN".hash(state);},
        x => {x.to_bits().hash(state);},
    }
}


#[cfg(test)]
mod test {
use std::collections::hash_map::DefaultHasher;
use std::mem;
use super::*;
    
    fn calculate_hash<T: Hash>(t: &T) -> u64 {
        let mut s = DefaultHasher::new();
        t.hash(&mut s);
        s.finish()
    } 

    #[test]
	fn test_types() {
        let t1: Vec<GosValue> = vec!
            [GosValue::new_str("Norway".to_string()), GosValue::Int(100),
            GosValue::new_str("Denmark".to_string()), GosValue::Int(10),
            ];

        let t2: Vec<GosValue> = vec!
            [GosValue::new_str("Norway".to_string()), GosValue::Int(100),
            GosValue::new_str("Denmark".to_string()), GosValue::Int(10),
            ];
        
        let a = GosValue::new_slice(t1);
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
    }
}