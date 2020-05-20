use super::opcode::OpIndex;
use super::types::{GosTypeData, GosValue, TypeKey, VMObjects};
use std::cell::{Cell, Ref, RefCell, RefMut};
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

    pub fn clone_data(&self) -> Rc<String> {
        self.data.clone()
    }

    pub fn into_string(self) -> String {
        Rc::try_unwrap(self.data).unwrap()
    }

    pub fn len(&self) -> usize {
        self.data.as_ref().len()
    }

    pub fn get_byte(&self, i: usize) -> u8 {
        self.data.as_ref().as_bytes()[i]
    }

    pub fn slice(&self, begin: Option<usize>, end: Option<usize>) -> StringVal {
        let self_len = self.len();
        let bi = begin.unwrap_or(0);
        let ei = end.unwrap_or(self_len);
        assert!(bi < self_len);
        assert!(bi <= ei && ei <= self_len);
        // todo: efficient slice-like string
        let sub = self.data.as_ref()[bi..ei].to_owned();
        StringVal {
            dark: false,
            data: Rc::new(sub),
        }
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
        // todo: technically we should pin 'objs'
        unsafe { self.objs.as_ref().unwrap() }
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

pub type GosHashMap = HashMap<HashKey, Cell<GosValue>>;

#[derive(Clone)]
pub struct MapVal {
    pub dark: bool,
    objs: *const VMObjects,
    default_val: Cell<GosValue>,
    // Why not Rc or no wrapper at all?
    // - Sometimes we need to copy it out of VMObjects, so we need a wapper
    // - If the underlying data is not read-only, you cannot simply use Rc
    map: Rc<RefCell<GosHashMap>>,
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
            default_val: Cell::new(default_val),
            map: Rc::new(RefCell::new(HashMap::new())),
        }
    }

    /// deep_clone creates a new MapVal with duplicated content of 'self.map'
    pub fn deep_clone(&self) -> MapVal {
        MapVal {
            dark: false,
            objs: self.objs,
            default_val: self.default_val.clone(),
            map: Rc::new(RefCell::new(self.map.borrow().clone())),
        }
    }

    pub fn insert(&mut self, key: GosValue, val: GosValue) -> Option<GosValue> {
        let hk = self.hash_key(key);
        self.map
            .borrow_mut()
            .insert(hk, Cell::new(val))
            .map(|x| x.into_inner())
    }

    pub fn get(&self, key: &GosValue) -> GosValue {
        let hk = HashKey {
            val: *key,
            objs: self.objs,
        };
        let mref = self.map.borrow();
        let cell = match mref.get(&hk) {
            Some(v) => v,
            None => &self.default_val,
        };
        cell.clone().into_inner()
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
            self.map.borrow_mut().insert(hk, self.default_val.clone());
        }
    }

    pub fn len(&self) -> usize {
        self.map.borrow().len()
    }

    pub fn borrow_data_mut(&self) -> RefMut<GosHashMap> {
        self.map.borrow_mut()
    }

    pub fn borrow_data(&self) -> Ref<GosHashMap> {
        self.map.borrow()
    }

    pub fn clone_data(&self) -> Rc<RefCell<GosHashMap>> {
        self.map.clone()
    }
}

// ----------------------------------------------------------------------------
// SliceVal

pub type GosVec = Vec<Cell<GosValue>>;

#[derive(Clone, Debug)]
pub struct SliceVal {
    pub dark: bool,
    begin: usize,
    end: usize,
    soft_cap: usize, // <= self.vec.capacity()
    vec: Rc<RefCell<GosVec>>,
}

impl<'a> SliceVal {
    pub fn new(len: usize, cap: usize, default_val: &GosValue) -> SliceVal {
        assert!(cap >= len);
        let mut val = SliceVal {
            dark: false,
            begin: 0,
            end: 0,
            soft_cap: cap,
            vec: Rc::new(RefCell::new(Vec::with_capacity(cap))),
        };
        for _ in 0..len {
            val.push(*default_val);
        }
        val
    }

    pub fn with_data(val: Vec<GosValue>) -> SliceVal {
        SliceVal {
            dark: false,
            begin: 0,
            end: val.len(),
            soft_cap: val.len(),
            vec: Rc::new(RefCell::new(
                val.into_iter().map(|x| Cell::new(x)).collect(),
            )),
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

    pub fn borrow(&self) -> SliceRef {
        SliceRef::new(self)
    }

    pub fn borrow_data_mut(&self) -> std::cell::RefMut<GosVec> {
        self.vec.borrow_mut()
    }

    pub fn borrow_data(&self) -> std::cell::Ref<GosVec> {
        self.vec.borrow()
    }

    pub fn push(&mut self, val: GosValue) {
        self.try_grow_vec(self.len() + 1);
        self.vec.borrow_mut().push(Cell::new(val));
        self.end += 1;
    }

    pub fn append(&mut self, vals: &mut GosVec) {
        let new_len = self.len() + vals.len();
        self.try_grow_vec(new_len);
        self.vec.borrow_mut().append(vals);
        self.end = self.begin + new_len;
    }

    pub fn get(&self, i: usize) -> Option<GosValue> {
        if let Some(val) = self.vec.borrow().get(self.begin + i) {
            Some(val.clone().into_inner())
        } else {
            None
        }
    }

    pub fn set(&self, i: usize, val: GosValue) {
        self.vec.borrow_mut()[self.begin + i] = Cell::new(val);
    }

    pub fn slice(&self, begin: Option<usize>, end: Option<usize>, max: Option<usize>) -> SliceVal {
        let self_len = self.len();
        let self_cap = self.cap();
        let bi = begin.unwrap_or(0);
        let ei = end.unwrap_or(self_len);
        let mi = max.unwrap_or(self_cap);
        assert!(bi < self_len);
        assert!(bi <= ei && ei <= self_len);
        assert!(ei <= mi && mi <= self_cap);
        SliceVal {
            dark: false,
            begin: self.begin + bi,
            end: self.begin + ei,
            soft_cap: self.begin + mi,
            vec: self.vec.clone(),
        }
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

/// SliceRef works like 'Ref', it means to be used as a temporary warpper
/// to help accessing inner vec
pub struct SliceRef<'a> {
    vec_ref: Ref<'a, GosVec>,
    begin: usize,
    end: usize,
}

pub type SliceIter<'a> = std::iter::Take<std::iter::Skip<std::slice::Iter<'a, Cell<GosValue>>>>;

pub type SliceEnumIter<'a> =
    std::iter::Enumerate<std::iter::Take<std::iter::Skip<std::slice::Iter<'a, Cell<GosValue>>>>>;

impl<'a> SliceRef<'a> {
    pub fn new(s: &SliceVal) -> SliceRef {
        SliceRef {
            vec_ref: s.vec.borrow(),
            begin: s.begin,
            end: s.end,
        }
    }

    pub fn iter(&self) -> SliceIter {
        self.vec_ref
            .iter()
            .skip(self.begin)
            .take(self.end - self.begin)
    }

    pub fn enumerate(&self) -> SliceEnumIter {
        self.vec_ref
            .iter()
            .skip(self.begin)
            .take(self.end - self.begin)
            .enumerate()
    }

    pub fn get(&self, i: usize) -> Option<&Cell<GosValue>> {
        self.vec_ref.get(self.begin + i)
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
        dbg!(mem::size_of::<RefCell<GosValue>>());

        let mut h: HashMap<isize, isize> = HashMap::new();
        h.insert(0, 1);
        let mut h2 = h.clone();
        h2.insert(0, 3);
        dbg!(h[&0]);
        dbg!(h2[&0]);
    }
}
