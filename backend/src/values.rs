use super::opcode::OpIndex;
use super::types::{gos_eq, GosTypeData, GosValue, Objects, TypeKey};
use std::cell::RefCell;
use std::cmp::Ordering;
use std::collections::HashMap;
use std::fmt;
use std::hash::{Hash, Hasher};
use std::iter::FromIterator;
use std::rc::Rc;

// ----------------------------------------------------------------------------
// InterfaceVal

#[derive(Clone, Debug)]
pub struct InterfaceVal {}

// ----------------------------------------------------------------------------
// StringVal

#[derive(Clone, Debug)]
pub struct StringVal {
    pub dark: bool,
    data: Rc<String>,
}

impl StringVal {
    pub fn with_str(s: String) -> StringVal {
        StringVal {
            dark: false,
            data: Rc::new(s),
        }
    }

    pub fn data_as_ref(&self) -> &String {
        self.data.as_ref()
    }

    pub fn into_string(self) -> String {
        Rc::try_unwrap(self.data).unwrap()
    }

    pub fn deep_clone(&self) -> StringVal {
        StringVal::with_str(self.data_as_ref().clone())
    }
}

impl PartialEq for StringVal {
    fn eq(&self, other: &StringVal) -> bool {
        self.data == other.data
    }
}

impl Eq for StringVal {}

impl PartialOrd for StringVal {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for StringVal {
    fn cmp(&self, other: &Self) -> Ordering {
        self.data_as_ref().cmp(other.data_as_ref())
    }
}

// ----------------------------------------------------------------------------
// MapVal

#[derive(Clone)]
pub struct HashKey {
    pub val: GosValue,
    pub objs: &'static Objects,
}

impl Eq for HashKey {}

impl PartialEq for HashKey {
    fn eq(&self, other: &HashKey) -> bool {
        gos_eq(&self.val, &other.val, self.objs)
    }
}

impl fmt::Debug for HashKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.val.fmt(f)
    }
}

impl Hash for HashKey {
    fn hash<H: Hasher>(&self, state: &mut H) {
        match &self.val {
            GosValue::Nil => 0.hash(state),
            GosValue::Bool(b) => b.hash(state),
            GosValue::Int(i) => i.hash(state),
            GosValue::Float64(f) => f.to_bits().hash(state),
            GosValue::Str(s) => self.objs.strings[*s].data.hash(state),
            /*
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

#[derive(Clone)]
pub struct MapVal {
    pub dark: bool,
    objs: &'static Objects,
    default_val: GosValue,
    data: HashMap<HashKey, GosValue>,
}

impl fmt::Debug for MapVal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MapVal")
            .field("dark", &self.dark)
            .field("default_val", &self.default_val)
            .field("data", &self.data)
            .finish()
    }
}

impl MapVal {
    pub fn new(objs: &'static Objects, default_val: GosValue) -> MapVal {
        MapVal {
            dark: false,
            objs: objs,
            default_val: default_val,
            data: HashMap::new(),
        }
    }

    pub fn insert(&mut self, key: GosValue, val: GosValue) -> Option<GosValue> {
        let hk = HashKey {
            val: key,
            objs: self.objs,
        };
        self.data.insert(hk, val)
    }

    pub fn get(&self, key: &GosValue) -> &GosValue {
        let hk = HashKey {
            val: *key,
            objs: self.objs,
        };
        match self.data.get(&hk) {
            Some(v) => v,
            None => &self.default_val,
        }
    }

    pub fn get_mut(&mut self, key: &GosValue) -> &mut GosValue {
        let hk = HashKey {
            val: *key,
            objs: self.objs,
        };
        let result = self.data.get_mut(&hk);
        if result.is_some() {
            result.unwrap();
        }
        self.insert(*key, self.default_val);
        self.data.get_mut(&hk).unwrap()
    }

    pub fn len(&self) -> usize {
        self.data.len()
    }
}

// ----------------------------------------------------------------------------
// StructVal

#[derive(Clone, Debug)]
pub struct StructVal {
    pub dark: bool,
    pub typ: TypeKey,
    pub fields: Vec<GosValue>,
}

impl StructVal {
    pub fn field_method_index(&self, name: &String, objs: &Objects) -> OpIndex {
        let t = &objs.types[self.typ];
        if let GosTypeData::Struct(_, map) = &t.data {
            map[name] as OpIndex
        } else {
            unreachable!()
        }
    }
}

// ----------------------------------------------------------------------------
// ChannelVal

#[derive(Clone, Debug)]
pub struct ChannelVal {
    pub dark: bool,
}

// ----------------------------------------------------------------------------
// SliceVal

#[derive(Clone, Debug)]
pub struct SliceVal {
    pub dark: bool,
    begin: usize,
    end: usize,
    soft_cap: usize, // <= self.vec.capacity()
    vec: Rc<RefCell<Vec<GosValue>>>,
}

impl<'a> SliceVal {
    pub fn new(cap: usize) -> SliceVal {
        SliceVal {
            dark: false,
            begin: 0,
            end: 0,
            soft_cap: cap,
            vec: Rc::new(RefCell::new(Vec::with_capacity(cap))),
        }
    }

    pub fn with_data(val: Vec<GosValue>) -> SliceVal {
        SliceVal {
            dark: false,
            begin: 0,
            end: 0,
            soft_cap: val.len(),
            vec: Rc::new(RefCell::new(val)),
        }
    }

    pub fn deep_clone(&self) -> SliceVal {
        let vec = Vec::from_iter(self.vec.borrow()[self.begin..self.end].iter().cloned());
        SliceVal {
            dark: false,
            begin: 0,
            end: self.cap(),
            soft_cap: self.cap(),
            vec: Rc::new(RefCell::new(vec)),
        }
    }

    pub fn len(&self) -> usize {
        self.end - self.begin
    }

    pub fn cap(&self) -> usize {
        self.soft_cap - self.begin
    }

    pub fn borrow_data_mut(&self) -> std::cell::RefMut<Vec<GosValue>> {
        self.vec.borrow_mut()
    }

    pub fn borrow_data(&self) -> std::cell::Ref<Vec<GosValue>> {
        self.vec.borrow()
    }

    pub fn push(&mut self, val: GosValue) {
        self.try_grow_vec(self.len() + 1);
        self.vec.borrow_mut().push(val);
        self.end += 1;
    }

    pub fn append(&mut self, vals: &mut Vec<GosValue>) {
        let new_len = self.len() + vals.len();
        self.try_grow_vec(new_len);
        self.vec.borrow_mut().append(vals);
        self.end = self.begin + new_len;
    }

    pub fn iter(&'a self) -> SliceValIter<'a> {
        SliceValIter {
            slice: self,
            cur: self.begin,
        }
    }

    pub fn get(&self, i: usize) -> Option<GosValue> {
        if let Some(val) = self.vec.borrow().get(i) {
            Some(val.clone())
        } else {
            None
        }
    }

    pub fn set(&self, i: usize, val: GosValue) {
        self.vec.borrow_mut()[i] = val;
    }

    fn try_grow_vec(&mut self, len: usize) {
        let mut cap = self.cap();
        assert!(cap >= self.len());
        if cap >= len {
            return;
        }
        while cap < len {
            if cap < 1024 {
                cap *= 2
            } else {
                cap = (cap as f32 * 1.25) as usize
            }
        }
        let data_len = self.len();
        let mut vec = Vec::from_iter(self.vec.borrow()[self.begin..self.end].iter().cloned());
        vec.reserve_exact(cap - vec.len());
        self.vec = Rc::new(RefCell::new(vec));
        self.begin = 0;
        self.end = data_len;
        self.soft_cap = cap;
    }
}

pub struct SliceValIter<'a> {
    slice: &'a SliceVal,
    cur: usize,
}

impl<'a> Iterator for SliceValIter<'a> {
    type Item = GosValue;

    fn next(&mut self) -> Option<GosValue> {
        if self.cur < self.slice.end {
            self.cur += 1;
            Some(self.slice.get(self.cur - 1).unwrap())
        } else {
            None
        }
    }
}
