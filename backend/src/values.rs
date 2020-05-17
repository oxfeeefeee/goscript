use super::opcode::OpIndex;
use super::types::{GosTypeData, GosValue, TypeKey, VMObjects};
use std::cell::{Ref, RefCell, RefMut};
use std::cmp::Ordering;
use std::collections::HashMap;
use std::fmt;
use std::hash::{Hash, Hasher};
use std::iter::FromIterator;
use std::rc::Rc;

// ----------------------------------------------------------------------------
// StringVal

#[derive(Clone, Debug)]
pub struct StringVal {
    pub dark: bool,
    // strings are readony, so unlike slices and maps, Rc is enough
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
    // needed for accessing managed data like GosValue::Str
    pub objs: *const VMObjects,
}

impl HashKey {
    fn objs(&self) -> &VMObjects {
        // There is no way around it, if you what the Hash trait implemented
        // because managed data like GosValue::Str needs a ref to VMObjects
        unsafe { &*self.objs }
    }
}

impl Eq for HashKey {}

impl PartialEq for HashKey {
    fn eq(&self, other: &HashKey) -> bool {
        self.val.eq(&other.val, self.objs())
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
            GosValue::Str(s) => self.objs().strings[*s].data.hash(state),
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
    objs: *const VMObjects,
    default_val: GosValue,
    // Why not Rc or no wrapper at all?
    // - Sometimes we need to copy it out of VMObjects, so we need a wapper
    // - If the underlying data is not read-only, you cannot simply use Rc
    map: Rc<RefCell<HashMap<HashKey, GosValue>>>,
}

impl fmt::Debug for MapVal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MapVal")
            .field("dark", &self.dark)
            .field("default_val", &self.default_val)
            .field("map", &self.map)
            .finish()
    }
}

impl MapVal {
    pub fn new(objs: &VMObjects, default_val: GosValue) -> MapVal {
        MapVal {
            dark: false,
            objs: objs,
            default_val: default_val,
            map: Rc::new(RefCell::new(HashMap::new())),
        }
    }

    /// deep_clone creates a new MapVal with duplicated content of 'self.map'
    pub fn deep_clone(&self) -> MapVal {
        MapVal {
            dark: false,
            objs: self.objs,
            default_val: self.default_val,
            map: Rc::new(RefCell::new(self.map.borrow().clone())),
        }
    }

    pub fn insert(&mut self, key: GosValue, val: GosValue) -> Option<GosValue> {
        let hk = self.hash_key(key);
        self.map.borrow_mut().insert(hk, val)
    }

    pub fn get(&self, key: &GosValue) -> GosValue {
        let hk = HashKey {
            val: *key,
            objs: self.objs,
        };
        match self.map.borrow().get(&hk) {
            Some(v) => *v,
            None => self.default_val,
        }
    }

    pub fn hash_key(&self, key: GosValue) -> HashKey {
        HashKey {
            val: key,
            objs: self.objs,
        }
    }

    /// touch_key makes sure there is a value for the 'key', a default value is set if
    /// the value is empty
    pub fn touch_key(&self, key: &GosValue) {
        let hk = HashKey {
            val: *key,
            objs: self.objs,
        };
        if self.map.borrow().get(&hk).is_none() {
            self.map.borrow_mut().insert(hk, self.default_val);
        }
    }

    pub fn len(&self) -> usize {
        self.map.borrow().len()
    }

    pub fn borrow_data_mut(&self) -> RefMut<HashMap<HashKey, GosValue>> {
        self.map.borrow_mut()
    }

    pub fn borrow_data(&self) -> Ref<HashMap<HashKey, GosValue>> {
        self.map.borrow()
    }
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

    /// deep_clone creates a new SliceVal with duplicated content of 'self.vec'
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

    pub fn iter(&self) -> SliceValIter {
        SliceValIter {
            begin: self.begin,
            end: self.end,
            vec: self.vec.clone(),
            cur: 0,
        }
    }

    pub fn get(&self, i: usize) -> Option<GosValue> {
        if let Some(val) = self.vec.borrow().get(self.begin + i) {
            Some(val.clone())
        } else {
            None
        }
    }

    pub fn set(&self, i: usize, val: GosValue) {
        self.vec.borrow_mut()[self.begin + i] = val;
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

#[derive(Clone, Debug)]
pub struct SliceValIter {
    begin: usize,
    end: usize,
    vec: Rc<RefCell<Vec<GosValue>>>,
    cur: usize,
}

impl<'a> Iterator for SliceValIter {
    type Item = (GosValue, GosValue);

    fn next(&mut self) -> Option<(GosValue, GosValue)> {
        let p = self.cur + self.begin;
        if p < self.end {
            Some((
                GosValue::Int(self.cur as isize),
                *self.vec.borrow().get(p).unwrap(),
            ))
        } else {
            None
        }
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
    pub fn field_method_index(&self, name: &String, objs: &VMObjects) -> OpIndex {
        let t = &objs.types[self.typ];
        if let GosTypeData::Struct(_, map) = &t.data {
            map[name] as OpIndex
        } else {
            unreachable!()
        }
    }
}

// ----------------------------------------------------------------------------
// InterfaceVal

#[derive(Clone, Debug)]
pub struct InterfaceVal {}

// ----------------------------------------------------------------------------
// ChannelVal

#[derive(Clone, Debug)]
pub struct ChannelVal {
    pub dark: bool,
}

#[cfg(test)]
mod test {
    use super::*;
    use std::mem;

    #[test]
    fn test_size() {
        dbg!(mem::size_of::<HashMap<HashKey, GosValue>>());
        dbg!(mem::size_of::<String>());
        dbg!(mem::size_of::<SliceVal>());
        dbg!(mem::size_of::<SliceValIter>());

        let mut h: HashMap<isize, isize> = HashMap::new();
        h.insert(0, 1);
        let mut h2 = h.clone();
        h2.insert(0, 3);
        dbg!(h[&0]);
        dbg!(h2[&0]);
    }
}
