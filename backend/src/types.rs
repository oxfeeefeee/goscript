#![allow(dead_code)]
#![macro_use]
use super::value::GosValue;
use slotmap::{new_key_type, DenseSlotMap};
use std::cell::{Ref, RefCell};
use std::collections::HashMap;
use std::hash::{Hash, Hasher};

pub use super::codegen::{FunctionVal, PackageVal};

const DEFAULT_CAPACITY: usize = 128;

new_key_type! { pub struct InterfaceKey; }
new_key_type! { pub struct ClosureKey; }
new_key_type! { pub struct StringKey; }
new_key_type! { pub struct SliceKey; }
new_key_type! { pub struct MapKey; }
new_key_type! { pub struct StructKey; }
new_key_type! { pub struct ChannelKey; }
new_key_type! { pub struct VecKey; }
new_key_type! { pub struct UpValueKey; }
new_key_type! { pub struct FunctionKey; }
new_key_type! { pub struct PackageKey; }

#[derive(Clone, Debug)]
pub struct Objects {
    pub interfaces: DenseSlotMap<InterfaceKey, InterfaceVal>,
    pub closures: DenseSlotMap<ClosureKey, ClosureVal>,
    pub strings: DenseSlotMap<StringKey, StringVal>,
    pub slices: DenseSlotMap<SliceKey, SliceVal>,
    pub maps: DenseSlotMap<MapKey, MapVal>,
    pub structs: DenseSlotMap<StructKey, StructVal>,
    pub channels: DenseSlotMap<ChannelKey, ChannelVal>,
    pub vecs: DenseSlotMap<VecKey, VecVal>,
    pub upvalues: DenseSlotMap<UpValueKey, UpValueVal>,
    pub functions: DenseSlotMap<FunctionKey, FunctionVal>,
    pub packages: DenseSlotMap<PackageKey, PackageVal>,
}

impl Objects {
    pub fn new() -> Objects {
        Objects {
            interfaces: DenseSlotMap::with_capacity_and_key(DEFAULT_CAPACITY),
            closures: DenseSlotMap::with_capacity_and_key(DEFAULT_CAPACITY),
            strings: DenseSlotMap::with_capacity_and_key(DEFAULT_CAPACITY),
            slices: DenseSlotMap::with_capacity_and_key(DEFAULT_CAPACITY),
            maps: DenseSlotMap::with_capacity_and_key(DEFAULT_CAPACITY),
            structs: DenseSlotMap::with_capacity_and_key(DEFAULT_CAPACITY),
            channels: DenseSlotMap::with_capacity_and_key(DEFAULT_CAPACITY),
            vecs: DenseSlotMap::with_capacity_and_key(DEFAULT_CAPACITY),
            upvalues: DenseSlotMap::with_capacity_and_key(DEFAULT_CAPACITY),
            functions: DenseSlotMap::with_capacity_and_key(DEFAULT_CAPACITY),
            packages: DenseSlotMap::with_capacity_and_key(DEFAULT_CAPACITY),
        }
    }

    pub fn new_string(&mut self, s: String) -> GosValue {
        GosValue::Str(self.strings.insert(StringVal {
            dark: false,
            data: s,
        }))
    }
}

#[derive(Clone, Debug)]
pub struct InterfaceVal {}

#[derive(Clone, Debug)]
pub struct UpValueVal {}

#[derive(Clone, Debug)]
pub struct ClosureVal {
    pub func: FunctionKey,
}

impl ClosureVal {
    pub fn new(key: FunctionKey) -> ClosureVal {
        ClosureVal { func: key }
    }
}

// ----------------------------------------------------------------------------
// StringVal

#[derive(Clone, Debug)]
pub struct StringVal {
    pub dark: bool,
    pub data: String,
}

impl PartialEq for StringVal {
    fn eq(&self, other: &StringVal) -> bool {
        self.data == other.data
    }
}

// ----------------------------------------------------------------------------
// VecVal

#[derive(Clone, Debug)]
pub struct VecVal {
    // Not visible to users
    pub dark: bool,
    pub data: RefCell<Vec<GosValue>>,
}

// ----------------------------------------------------------------------------
// MapVal

#[derive(Clone, Debug)]
pub struct MapVal {
    pub dark: bool,
    pub data: HashMap<GosValue, GosValue>,
}

// ----------------------------------------------------------------------------
// StructVal

#[derive(Copy, Clone, Debug)]
pub struct StructVal {
    pub dark: bool,
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
    pub begin: usize,
    pub end: usize,
    pub vec_key: VecKey,
}

impl<'a, 'o> SliceVal {
    pub fn len(&self) -> usize {
        self.end - self.begin
    }

    pub fn iter(&'a self, o: &'o Objects) -> SliceValIter<'a, 'o> {
        SliceValIter {
            slice: self,
            objects: o,
            cur: self.begin,
        }
    }

    pub fn get_item(&self, o: &'o Objects, i: usize) -> Option<GosValue> {
        if let Some(vec) = o.vecs.get(self.vec_key) {
            if let Some(val) = vec.data.borrow().get(i) {
                Some(val.clone())
            } else {
                None
            }
        } else {
            None
        }
    }

    pub fn set_item(&self, o: &'o Objects, i: usize, val: &GosValue) {
        if let Some(vec) = o.vecs.get(self.vec_key) {
            vec.data.borrow_mut()[i] = *val;
        } else {
            unreachable!();
        }
    }
}

pub struct SliceValIter<'a, 'o: 'a> {
    slice: &'a SliceVal,
    objects: &'o Objects,
    cur: usize,
}

impl<'a, 'o> Iterator for SliceValIter<'a, 'o> {
    type Item = GosValue;

    fn next(&mut self) -> Option<GosValue> {
        if self.cur < self.slice.end {
            self.cur += 1;
            Some(self.slice.get_item(self.objects, self.cur - 1).unwrap())
        } else {
            None
        }
    }
}
