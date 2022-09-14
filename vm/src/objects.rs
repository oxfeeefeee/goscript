// Copyright 2022 The Goscript Authors. All rights reserved.
// Use of this source code is governed by a BSD-style
// license that can be found in the LICENSE file.

#![macro_use]
use crate::channel::Channel;
use crate::ffi::Ffi;
use crate::gc::GcContainer;
use crate::instruction::{Instruction, OpIndex, ValueType};
use crate::metadata::*;
use crate::stack::Stack;
use crate::value::*;
use goscript_parser::objects::*;
use goscript_parser::piggy_key_type;
use std::any::Any;
use std::borrow::Cow;
use std::cell::{Cell, Ref, RefCell, RefMut};
use std::collections::HashMap;
use std::fmt::{self, Debug, Display, Write};
use std::hash::{Hash, Hasher};
use std::marker::PhantomData;
use std::ops::Range;
use std::rc::{Rc, Weak};
use std::{panic, ptr, str};

piggy_key_type! {
    pub struct MetadataKey;
    pub struct FunctionKey;
    pub struct PackageKey;
}

pub type MetadataObjs = PiggyVec<MetadataKey, MetadataType>;
pub type FunctionObjs = PiggyVec<FunctionKey, FunctionVal>;
pub type PackageObjs = PiggyVec<PackageKey, PackageVal>;

#[derive(Debug)]
pub struct VMObjects {
    pub metas: MetadataObjs,
    pub functions: FunctionObjs,
    pub packages: PackageObjs,
    pub s_meta: StaticMeta,
}

impl VMObjects {
    pub fn new() -> VMObjects {
        const CAP: usize = 16;
        let mut metas = PiggyVec::with_capacity(CAP);
        let md = StaticMeta::new(&mut metas);
        VMObjects {
            metas,
            functions: PiggyVec::with_capacity(CAP),
            packages: PiggyVec::with_capacity(CAP),
            s_meta: md,
        }
    }
}

// ----------------------------------------------------------------------------
// MapObj

pub type GosHashMap = HashMap<GosValue, GosValue>;

pub type GosHashMapIter<'a> = std::collections::hash_map::Iter<'a, GosValue, GosValue>;

#[derive(Debug)]
pub struct MapObj {
    map: RefCell<GosHashMap>,
}

impl MapObj {
    pub fn new() -> MapObj {
        MapObj {
            map: RefCell::new(HashMap::new()),
        }
    }

    #[inline]
    pub fn insert(&self, key: GosValue, val: GosValue) -> Option<GosValue> {
        self.borrow_data_mut().insert(key, val)
    }

    #[inline]
    pub fn get(&self, key: &GosValue) -> Option<GosValue> {
        let borrow = self.borrow_data();
        let val = borrow.get(key);
        match val {
            Some(v) => Some(v.clone()),
            None => None,
        }
    }

    #[inline]
    pub fn delete(&self, key: &GosValue) {
        let mut mref = self.borrow_data_mut();
        mref.remove(key);
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.borrow_data().len()
    }

    #[inline]
    pub fn borrow_data_mut(&self) -> RefMut<GosHashMap> {
        self.map.borrow_mut()
    }

    #[inline]
    pub fn borrow_data(&self) -> Ref<GosHashMap> {
        self.map.borrow()
    }

    #[inline]
    pub fn clone_inner(&self) -> RefCell<GosHashMap> {
        self.map.clone()
    }
}

impl Clone for MapObj {
    fn clone(&self) -> Self {
        MapObj {
            map: self.map.clone(),
        }
    }
}

impl PartialEq for MapObj {
    fn eq(&self, _other: &MapObj) -> bool {
        unreachable!()
    }
}

impl Eq for MapObj {}

impl Display for MapObj {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("map[")?;
        for (i, kv) in self.map.borrow().iter().enumerate() {
            if i > 0 {
                f.write_char(' ')?;
            }
            let v: &GosValue = &kv.1;
            write!(f, "{}:{}", kv.0, v)?
        }
        f.write_char(']')
    }
}

// ----------------------------------------------------------------------------
// ArrayObj

/// Element is used to store GosValue in Typed containers to save memomry
pub trait Element: Clone + Hash + Debug {
    fn from_value(val: GosValue) -> Self;

    fn into_value(self, t: ValueType) -> GosValue;

    fn set_value(&self, val: &GosValue);

    fn need_gc() -> bool {
        false
    }

    fn copy_or_clone_slice(dst: &mut [Self], src: &[Self]) {
        dst.clone_from_slice(src)
    }
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct GosElem {
    cell: RefCell<GosValue>,
}

impl GosElem {
    /// for gc
    pub fn ref_sub_one(&self) {
        self.cell.borrow().ref_sub_one();
    }

    /// for gc
    pub fn mark_dirty(&self, queue: &mut RCQueue) {
        self.cell.borrow().mark_dirty(queue);
    }
}

impl std::fmt::Display for GosElem {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        std::fmt::Display::fmt(&self.cell.borrow(), f)
    }
}

impl Hash for GosElem {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.cell.borrow().hash(state)
    }
}

impl Element for GosElem {
    #[inline]
    fn from_value(val: GosValue) -> Self {
        GosElem {
            cell: RefCell::new(val),
        }
    }

    #[inline]
    fn into_value(self, _t: ValueType) -> GosValue {
        self.cell.into_inner()
    }

    #[inline]
    fn set_value(&self, val: &GosValue) {
        self.cell.replace(val.clone());
    }

    #[inline]
    fn need_gc() -> bool {
        true
    }
}

/// Cell is much cheaper than RefCell, used to store basic types
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct CellElem<T>
where
    T: Copy + PartialEq,
{
    pub cell: Cell<T>,
}

impl<T> CellElem<T>
where
    T: Copy + PartialEq,
{
    pub fn into_inner(self) -> T {
        self.cell.into_inner()
    }

    fn clone_slice(dst: &mut [CellElem<T>], src: &[CellElem<T>]) {
        let d: &mut [T] = unsafe { std::mem::transmute(dst) };
        let s: &[T] = unsafe { std::mem::transmute(src) };
        d.copy_from_slice(s)
    }
}

pub type Elem8 = CellElem<u8>;
pub type Elem16 = CellElem<u16>;
pub type Elem32 = CellElem<u32>;
pub type Elem64 = CellElem<u64>;

/// This can be used when any version of Slice/Array returns the same thing
/// kind of unsafe
pub type AnyElem = CellElem<u8>;

impl<T> Hash for CellElem<T>
where
    T: Copy + PartialEq + Hash,
{
    fn hash<H: Hasher>(&self, state: &mut H) {
        let d = self.cell.get();
        d.hash(state)
    }
}

impl Element for CellElem<u8> {
    #[inline]
    fn from_value(val: GosValue) -> Self {
        CellElem {
            cell: Cell::new(*val.as_uint8()),
        }
    }

    #[inline]
    fn into_value(self, t: ValueType) -> GosValue {
        let data = ValueData::new_uint8(self.cell.get());
        GosValue::new(t, data)
    }

    #[inline]
    fn set_value(&self, val: &GosValue) {
        self.cell.set(*val.as_uint8());
    }

    #[inline]
    fn copy_or_clone_slice(dst: &mut [Self], src: &[Self]) {
        CellElem::<u8>::clone_slice(dst, src)
    }
}

impl Element for Elem16 {
    #[inline]
    fn from_value(val: GosValue) -> Self {
        CellElem {
            cell: Cell::new(*val.as_uint16()),
        }
    }

    #[inline]
    fn into_value(self, t: ValueType) -> GosValue {
        let data = ValueData::new_uint16(self.cell.get());
        GosValue::new(t, data)
    }

    #[inline]
    fn set_value(&self, val: &GosValue) {
        self.cell.set(*val.as_uint16());
    }

    #[inline]
    fn copy_or_clone_slice(dst: &mut [Self], src: &[Self]) {
        CellElem::<u16>::clone_slice(dst, src)
    }
}

impl Element for Elem32 {
    #[inline]
    fn from_value(val: GosValue) -> Self {
        CellElem {
            cell: Cell::new(*val.as_uint32()),
        }
    }

    #[inline]
    fn into_value(self, t: ValueType) -> GosValue {
        let data = ValueData::new_uint32(self.cell.get());
        GosValue::new(t, data)
    }

    #[inline]
    fn set_value(&self, val: &GosValue) {
        self.cell.set(*val.as_uint32());
    }

    #[inline]
    fn copy_or_clone_slice(dst: &mut [Self], src: &[Self]) {
        CellElem::<u32>::clone_slice(dst, src)
    }
}

impl Element for Elem64 {
    #[inline]
    fn from_value(val: GosValue) -> Self {
        CellElem {
            cell: Cell::new(*val.as_uint64()),
        }
    }

    #[inline]
    fn into_value(self, t: ValueType) -> GosValue {
        let data = ValueData::new_uint64(self.cell.get());
        GosValue::new(t, data)
    }

    #[inline]
    fn set_value(&self, val: &GosValue) {
        self.cell.set(*val.as_uint64());
    }

    #[inline]
    fn copy_or_clone_slice(dst: &mut [Self], src: &[Self]) {
        CellElem::<u64>::clone_slice(dst, src)
    }
}

pub struct ArrayObj<T> {
    vec: RefCell<Vec<T>>,
}

pub type GosArrayObj = ArrayObj<GosElem>;

impl<T> ArrayObj<T>
where
    T: Element,
{
    pub(crate) fn with_size(
        size: usize,
        cap: usize,
        val: &GosValue,
        gcos: &GcContainer,
    ) -> ArrayObj<T> {
        assert!(cap >= size);
        let mut v = Vec::with_capacity(cap);
        for _ in 0..size {
            v.push(T::from_value(val.copy_semantic(gcos)))
        }
        ArrayObj {
            vec: RefCell::new(v),
        }
    }

    pub fn with_data(data: Vec<GosValue>) -> ArrayObj<T> {
        ArrayObj {
            vec: RefCell::new(data.into_iter().map(|x| T::from_value(x)).collect()),
        }
    }

    pub fn with_raw_data(data: Vec<T>) -> ArrayObj<T> {
        ArrayObj {
            vec: RefCell::new(data),
        }
    }

    #[inline(always)]
    pub fn len(&self) -> usize {
        self.borrow_data().len()
    }

    #[inline(always)]
    pub fn borrow_data_mut(&self) -> std::cell::RefMut<Vec<T>> {
        self.vec.borrow_mut()
    }

    #[inline(always)]
    pub fn borrow_data(&self) -> std::cell::Ref<Vec<T>> {
        self.vec.borrow()
    }

    #[inline]
    pub fn as_rust_slice(&self) -> Ref<[T]> {
        Ref::map(self.borrow_data(), |x| &x[..])
    }

    #[inline]
    pub fn as_rust_slice_mut(&self) -> RefMut<[T]> {
        RefMut::map(self.borrow_data_mut(), |x| &mut x[..])
    }

    #[inline(always)]
    pub fn index_elem(&self, i: usize) -> T {
        self.borrow_data()[i].clone()
    }

    #[inline(always)]
    pub fn get(&self, i: usize, t: ValueType) -> RuntimeResult<GosValue> {
        if i >= self.len() {
            return Err(format!("index {} out of range", i).to_owned());
        }
        Ok(self.borrow_data()[i].clone().into_value(t))
    }

    #[inline(always)]
    pub fn set(&self, i: usize, val: &GosValue) -> RuntimeResult<()> {
        if i >= self.len() {
            return Err(format!("index {} out of range", i).to_owned());
        }
        Ok(self.borrow_data()[i].set_value(&val))
    }

    #[inline]
    pub fn size_of_data(&self) -> usize {
        std::mem::size_of::<T>() * self.len()
    }

    pub fn display_fmt(&self, t: ValueType, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_char('[')?;
        for (i, e) in self.vec.borrow().iter().enumerate() {
            if i > 0 {
                f.write_char(' ')?;
            }
            write!(f, "{}", e.clone().into_value(t))?
        }
        f.write_char(']')
    }

    pub fn debug_fmt(&self, t: ValueType, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_char('[')?;
        for (i, e) in self.vec.borrow().iter().enumerate() {
            if i > 0 {
                f.write_char(' ')?;
            }
            write!(f, "{:#?}", e.clone().into_value(t))?
        }
        f.write_char(']')
    }
}

impl<T> Hash for ArrayObj<T>
where
    T: Element,
{
    fn hash<H: Hasher>(&self, state: &mut H) {
        for e in self.borrow_data().iter() {
            e.hash(state);
        }
    }
}

impl<T> Eq for ArrayObj<T> where T: Element + PartialEq {}

impl<T> PartialEq for ArrayObj<T>
where
    T: Element + PartialEq,
{
    fn eq(&self, b: &ArrayObj<T>) -> bool {
        if self.borrow_data().len() != b.borrow_data().len() {
            return false;
        }
        let bdata = b.borrow_data();
        for (i, e) in self.borrow_data().iter().enumerate() {
            if e != bdata.get(i).unwrap() {
                return false;
            }
        }
        true
    }
}

impl<T> Clone for ArrayObj<T>
where
    T: Element + PartialEq,
{
    fn clone(&self) -> Self {
        ArrayObj {
            vec: RefCell::new(self.borrow_data().iter().map(|x| x.clone()).collect()),
        }
    }
}

// ----------------------------------------------------------------------------
// SliceObj

#[derive(Clone)]
pub struct SliceObj<T> {
    array: GosValue,
    begin: Cell<usize>,
    end: Cell<usize>,
    // This is not capacity, but rather the max index that can be sliced.
    cap_end: Cell<usize>,
    phantom: PhantomData<T>,
}

pub type GosSliceObj = SliceObj<GosElem>;

impl<T> SliceObj<T>
where
    T: Element,
{
    pub fn with_array(arr: GosValue, begin: isize, end: isize) -> RuntimeResult<SliceObj<T>> {
        let len = arr.as_array::<T>().0.len();
        let (bi, ei, cap) = SliceObj::<T>::check_indices(0, len, len, begin, end, -1)?;
        Ok(SliceObj {
            begin: Cell::from(bi),
            end: Cell::from(ei),
            cap_end: Cell::from(cap),
            array: arr,
            phantom: PhantomData,
        })
    }

    /// Get a reference to the slice obj's array.
    #[must_use]
    #[inline]
    pub fn array(&self) -> &GosValue {
        &self.array
    }

    #[inline(always)]
    pub fn array_obj(&self) -> &ArrayObj<T> {
        &self.array.as_array::<T>().0
    }

    #[inline(always)]
    pub fn begin(&self) -> usize {
        self.begin.get()
    }

    #[inline(always)]
    pub fn end(&self) -> usize {
        self.end.get()
    }

    #[inline(always)]
    pub fn len(&self) -> usize {
        self.end() - self.begin()
    }

    #[inline(always)]
    pub fn cap(&self) -> usize {
        self.cap_end.get() - self.begin()
    }

    #[inline(always)]
    pub fn range(&self) -> Range<usize> {
        self.begin.get()..self.end.get()
    }

    #[inline]
    pub fn as_rust_slice(&self) -> Ref<[T]> {
        Ref::map(self.borrow_all_data(), |x| {
            &x[self.begin.get()..self.end.get()]
        })
    }

    #[inline]
    pub fn as_rust_slice_mut(&self) -> RefMut<[T]> {
        RefMut::map(self.borrow_all_data_mut(), |x| {
            &mut x[self.begin.get()..self.end.get()]
        })
    }

    #[inline]
    pub unsafe fn as_raw_slice<U>(&self) -> Ref<[U]> {
        assert!(std::mem::size_of::<U>() == std::mem::size_of::<T>());
        std::mem::transmute(self.as_rust_slice())
    }

    #[inline]
    pub unsafe fn as_raw_slice_mut<U>(&self) -> RefMut<[U]> {
        assert!(std::mem::size_of::<U>() == std::mem::size_of::<T>());
        std::mem::transmute(self.as_rust_slice_mut())
    }

    #[inline]
    pub fn sharing_with(&self, other: &SliceObj<T>) -> bool {
        self.array.as_addr() == other.array.as_addr()
    }

    /// get_array_equivalent returns the underlying array and mapped index
    #[inline]
    pub fn get_array_equivalent(&self, i: usize) -> (&GosValue, usize) {
        (self.array(), self.begin() + i)
    }

    #[inline(always)]
    pub fn index_elem(&self, i: usize) -> T {
        self.array_obj().index_elem(self.begin() + i)
    }

    #[inline(always)]
    pub fn get(&self, i: usize, t: ValueType) -> RuntimeResult<GosValue> {
        self.array_obj().get(self.begin() + i, t)
    }

    #[inline(always)]
    pub fn set(&self, i: usize, val: &GosValue) -> RuntimeResult<()> {
        self.array_obj().set(i, val)
    }

    #[inline]
    pub fn push(&mut self, val: GosValue) {
        let mut data = self.borrow_all_data_mut();
        if data.len() == self.len() {
            data.push(T::from_value(val))
        } else {
            data[self.end()] = T::from_value(val);
        }
        drop(data);
        *self.end.get_mut() += 1;
        if self.cap_end.get() < self.end.get() {
            *self.cap_end.get_mut() += 1;
        }
    }

    #[inline]
    pub fn append(&mut self, other: &SliceObj<T>) {
        let mut data = self.borrow_all_data_mut();
        let new_end = self.end() + other.len();
        let after_end_len = data.len() - self.end();
        let sharing = self.sharing_with(other);
        if after_end_len <= other.len() {
            data.truncate(self.end());
            if !sharing {
                data.extend_from_slice(&other.as_rust_slice());
            } else {
                data.extend_from_within(other.range());
            }
        } else {
            if !sharing {
                T::copy_or_clone_slice(&mut data[self.end()..new_end], &other.as_rust_slice());
            } else {
                let cloned = data[other.range()].to_vec();
                T::copy_or_clone_slice(&mut data[self.end()..new_end], &cloned);
            }
        }
        drop(data);
        *self.end.get_mut() = new_end;
        if self.cap_end.get() < self.end.get() {
            *self.cap_end.get_mut() = self.end.get();
        }
    }

    #[inline]
    pub fn copy_from(&self, other: &SliceObj<T>) -> usize {
        let other_range = other.range();
        let (left, right) = match self.len() >= other.len() {
            true => (self.begin()..self.begin() + other.len(), other_range),
            false => (
                self.begin()..self.end(),
                other_range.start..other_range.start + self.len(),
            ),
        };
        let len = left.len();
        let sharing = self.sharing_with(other);
        let data = &mut self.borrow_all_data_mut();
        if !sharing {
            T::copy_or_clone_slice(&mut data[left], &other.borrow_all_data()[right]);
        } else {
            let cloned = data[right].to_vec();
            T::copy_or_clone_slice(&mut data[left], &cloned);
        }
        len
    }

    #[inline]
    pub fn slice(&self, begin: isize, end: isize, max: isize) -> RuntimeResult<SliceObj<T>> {
        let (bi, ei, cap) = SliceObj::<T>::check_indices(
            self.begin(),
            self.len(),
            self.cap_end.get(),
            begin,
            end,
            max,
        )?;
        Ok(SliceObj {
            begin: Cell::from(bi),
            end: Cell::from(ei),
            cap_end: Cell::from(cap),
            array: self.array.clone(),
            phantom: PhantomData,
        })
    }

    #[inline]
    pub fn get_vec(&self, t: ValueType) -> Vec<GosValue> {
        self.as_rust_slice()
            .iter()
            .map(|x: &T| x.clone().into_value(t))
            .collect()
    }

    #[inline]
    pub fn swap(&self, i: usize, j: usize) -> RuntimeResult<()> {
        let len = self.len();
        if i >= len {
            Err(format!("index {} out of range", i))
        } else if j >= len {
            Err(format!("index {} out of range", j))
        } else {
            self.borrow_all_data_mut()
                .swap(i + self.begin(), j + self.begin());
            Ok(())
        }
    }

    #[inline]
    pub fn borrow_all_data_mut(&self) -> std::cell::RefMut<Vec<T>> {
        self.array_obj().borrow_data_mut()
    }

    #[inline]
    fn borrow_all_data(&self) -> std::cell::Ref<Vec<T>> {
        self.array_obj().borrow_data()
    }

    #[inline]
    fn check_indices(
        this_begin: usize,
        this_len: usize,
        this_cap: usize,
        begin: isize,
        end: isize,
        max: isize,
    ) -> RuntimeResult<(usize, usize, usize)> {
        let bi = this_begin + begin as usize;
        if bi > this_cap {
            return Err(format!("index {} out of range", begin).to_owned());
        }

        let cap = if max < 0 {
            this_cap
        } else {
            let val = max as usize;
            if val > this_cap {
                return Err(format!("index {} out of range", max).to_owned());
            }
            val
        };

        let ei = if end < 0 {
            this_len
        } else {
            let val = this_begin + end as usize;
            if val < bi || val > cap {
                return Err(format!("index {} out of range", end).to_owned());
            }
            val
        };

        Ok((bi, ei, cap))
    }

    pub fn display_fmt(&self, t: ValueType, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_char('[')?;
        for (i, e) in self.as_rust_slice().iter().enumerate() {
            if i > 0 {
                f.write_char(' ')?;
            }
            write!(f, "{}", e.clone().into_value(t))?
        }
        f.write_char(']')
    }

    pub fn debug_fmt(&self, t: ValueType, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_char('[')?;
        for (i, e) in self.as_rust_slice().iter().enumerate() {
            if i > 0 {
                f.write_char(' ')?;
            }
            write!(f, "{:#?}", e.clone().into_value(t))?
        }
        f.write_char(']')
    }
}

impl<T> PartialEq for SliceObj<T> {
    fn eq(&self, _other: &SliceObj<T>) -> bool {
        unreachable!() //false
    }
}

impl<T> Eq for SliceObj<T> {}

pub type SliceIter<'a, T> = std::slice::Iter<'a, T>;

pub type SliceEnumIter<'a, T> = std::iter::Enumerate<SliceIter<'a, T>>;

// ----------------------------------------------------------------------------
// StringObj

pub type StringIter<'a> = std::str::Chars<'a>;

pub type StringEnumIter<'a> = std::iter::Enumerate<StringIter<'a>>;

pub type StringObj = SliceObj<Elem8>;

pub struct StrUtil;

impl StrUtil {
    #[inline]
    pub fn with_str(s: &str) -> StringObj {
        let buf: Vec<Elem8> = unsafe { std::mem::transmute(s.as_bytes().to_vec()) };
        StrUtil::buf_into_string(buf)
    }

    /// It's safe because strings are readonly
    /// https://stackoverflow.com/questions/50431702/is-it-safe-and-defined-behavior-to-transmute-between-a-t-and-an-unsafecellt
    /// https://doc.rust-lang.org/src/core/str/converts.rs.html#173
    #[inline]
    pub fn as_str(this: &StringObj) -> Ref<str> {
        unsafe { std::mem::transmute(this.as_rust_slice()) }
    }

    #[inline]
    pub fn index(this: &StringObj, i: usize) -> RuntimeResult<GosValue> {
        this.get(i, ValueType::Uint8)
    }

    #[inline]
    pub fn index_elem(this: &StringObj, i: usize) -> u8 {
        this.index_elem(i).into_inner()
    }

    #[inline]
    pub fn add(a: &StringObj, b: &StringObj) -> StringObj {
        let mut buf = a.as_rust_slice().to_vec();
        buf.extend_from_slice(&b.as_rust_slice());
        StrUtil::buf_into_string(buf)
    }

    #[inline]
    fn buf_into_string(buf: Vec<Elem8>) -> StringObj {
        let arr = GosValue::new_non_gc_array(ArrayObj::with_raw_data(buf), ValueType::Uint8);
        SliceObj::with_array(arr, 0, -1).unwrap()
    }
}

// ----------------------------------------------------------------------------
// StructObj

#[derive(Clone, Debug)]
pub struct StructObj {
    fields: RefCell<Vec<GosValue>>,
}

impl StructObj {
    pub fn new(fields: Vec<GosValue>) -> StructObj {
        StructObj {
            fields: RefCell::new(fields),
        }
    }

    #[inline]
    pub fn borrow_fields(&self) -> Ref<Vec<GosValue>> {
        self.fields.borrow()
    }

    #[inline]
    pub fn borrow_fields_mut(&self) -> RefMut<Vec<GosValue>> {
        self.fields.borrow_mut()
    }
}

impl Eq for StructObj {}

impl PartialEq for StructObj {
    #[inline]
    fn eq(&self, other: &StructObj) -> bool {
        let other_fields = other.borrow_fields();
        for (i, f) in self.borrow_fields().iter().enumerate() {
            if f != &other_fields[i] {
                return false;
            }
        }
        return true;
    }
}

impl Hash for StructObj {
    #[inline]
    fn hash<H: Hasher>(&self, state: &mut H) {
        for f in self.borrow_fields().iter() {
            f.hash(state)
        }
    }
}

impl Display for StructObj {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_char('{')?;
        for (i, fld) in self.borrow_fields().iter().enumerate() {
            if i > 0 {
                f.write_char(' ')?;
            }
            write!(f, "{}", fld)?
        }
        f.write_char('}')
    }
}

// ----------------------------------------------------------------------------
// InterfaceObj

#[derive(Clone, Debug)]
pub struct UnderlyingFfi {
    pub ffi_obj: Rc<dyn Ffi>,
    pub meta: Meta,
}

impl UnderlyingFfi {
    pub fn new(obj: Rc<dyn Ffi>, meta: Meta) -> UnderlyingFfi {
        UnderlyingFfi {
            ffi_obj: obj,
            meta: meta,
        }
    }
}

/// Info about how to invoke a method of the underlying value
/// of an interface
#[derive(Clone, Debug)]
pub enum IfaceBinding {
    Struct(Rc<RefCell<MethodDesc>>, Option<Vec<OpIndex>>),
    Iface(usize, Option<Vec<OpIndex>>),
}

#[derive(Clone, Debug)]
pub enum Binding4Runtime {
    Struct(FunctionKey, bool, Option<Vec<OpIndex>>),
    Iface(usize, Option<Vec<OpIndex>>),
}

impl From<IfaceBinding> for Binding4Runtime {
    fn from(item: IfaceBinding) -> Binding4Runtime {
        match item {
            IfaceBinding::Struct(f, indices) => {
                let md = f.borrow();
                Binding4Runtime::Struct(md.func.unwrap(), md.pointer_recv, indices)
            }
            IfaceBinding::Iface(a, b) => Binding4Runtime::Iface(a, b),
        }
    }
}

#[derive(Clone, Debug)]
pub enum InterfaceObj {
    // The Meta and Binding info are all determined at compile time.
    // They are not available if the Interface is created at runtime
    // as an empty Interface holding a GosValue, which acts like a
    // dynamic type.
    Gos(GosValue, Option<(Meta, Vec<Binding4Runtime>)>),
    Ffi(UnderlyingFfi),
}

impl InterfaceObj {
    pub fn with_value(val: GosValue, meta: Option<(Meta, Vec<Binding4Runtime>)>) -> InterfaceObj {
        InterfaceObj::Gos(val, meta)
    }

    #[inline]
    pub fn underlying_value(&self) -> Option<&GosValue> {
        match self {
            Self::Gos(v, _) => Some(v),
            _ => None,
        }
    }

    #[inline]
    pub fn equals_value(&self, val: &GosValue) -> bool {
        match self.underlying_value() {
            Some(v) => v == val,
            None => false,
        }
    }

    /// for gc
    pub fn ref_sub_one(&self) {
        match self {
            Self::Gos(v, _) => v.ref_sub_one(),
            _ => {}
        };
    }

    /// for gc
    pub fn mark_dirty(&self, queue: &mut RCQueue) {
        match self {
            Self::Gos(v, _) => v.mark_dirty(queue),
            _ => {}
        };
    }
}

impl Eq for InterfaceObj {}

impl PartialEq for InterfaceObj {
    #[inline]
    fn eq(&self, other: &InterfaceObj) -> bool {
        match (self, other) {
            (Self::Gos(x, _), Self::Gos(y, _)) => x == y,
            (Self::Ffi(x), Self::Ffi(y)) => Rc::ptr_eq(&x.ffi_obj, &y.ffi_obj),
            _ => false,
        }
    }
}

impl Hash for InterfaceObj {
    #[inline]
    fn hash<H: Hasher>(&self, state: &mut H) {
        match self {
            Self::Gos(v, _) => v.hash(state),
            Self::Ffi(ffi) => Rc::as_ptr(&ffi.ffi_obj).hash(state),
        }
    }
}

impl Display for InterfaceObj {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Gos(v, _) => write!(f, "{}", v),
            Self::Ffi(ffi) => write!(f, "<ffi>{:?}", ffi.ffi_obj),
        }
    }
}

// ----------------------------------------------------------------------------
// ChannelObj

#[derive(Clone, Debug)]
pub struct ChannelObj {
    pub recv_zero: GosValue,
    pub chan: Channel,
}

impl ChannelObj {
    pub fn new(cap: usize, recv_zero: GosValue) -> ChannelObj {
        ChannelObj {
            chan: Channel::new(cap),
            recv_zero: recv_zero,
        }
    }

    pub fn with_chan(chan: Channel, recv_zero: GosValue) -> ChannelObj {
        ChannelObj {
            chan: chan,
            recv_zero: recv_zero,
        }
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.chan.len()
    }

    #[inline]
    pub fn cap(&self) -> usize {
        self.chan.cap()
    }

    #[inline]
    pub fn close(&self) {
        self.chan.close()
    }

    pub async fn send(&self, v: &GosValue) -> RuntimeResult<()> {
        self.chan.send(v).await
    }

    pub async fn recv(&self) -> Option<GosValue> {
        self.chan.recv().await
    }
}

// ----------------------------------------------------------------------------
// PointerObj

/// There are 4 types of pointers, they point to:
/// - local
/// - slice member
/// - struct field
/// - package member
/// References to outer function's local vars and pointers to local vars are all
/// UpValues

#[derive(Debug, Clone)]
pub enum PointerObj {
    UpVal(UpValue),
    SliceMember(GosValue, OpIndex),
    StructField(GosValue, OpIndex),
    PkgMember(PackageKey, OpIndex),
}

impl PointerObj {
    #[inline]
    pub fn new_closed_up_value(val: &GosValue) -> PointerObj {
        PointerObj::UpVal(UpValue::new_closed(val.clone()))
    }

    #[inline]
    pub fn new_array_member(
        val: GosValue,
        i: OpIndex,
        t_elem: ValueType,
    ) -> RuntimeResult<PointerObj> {
        let slice = GosValue::slice_array(val, 0, -1, t_elem)?;
        // todo: index check!
        Ok(PointerObj::SliceMember(slice, i))
    }

    #[inline]
    pub fn new_slice_member(
        val: GosValue,
        i: OpIndex,
        t: ValueType,
        t_elem: ValueType,
    ) -> RuntimeResult<PointerObj> {
        match t {
            ValueType::Array => PointerObj::new_array_member(val, i, t_elem),
            ValueType::Slice => match val.is_nil() {
                false => Ok(PointerObj::SliceMember(val, i)),
                true => Err("access nil value".to_owned()),
            },
            _ => unreachable!(),
        }
    }

    #[inline]
    pub fn deref(&self, stack: &Stack, pkgs: &PackageObjs) -> RuntimeResult<GosValue> {
        match self {
            PointerObj::UpVal(uv) => Ok(uv.value(stack).into_owned()),
            PointerObj::SliceMember(s, index) => s.dispatcher_a_s().slice_get(s, *index as usize),
            PointerObj::StructField(s, index) => {
                Ok(s.as_struct().0.borrow_fields()[*index as usize].clone())
            }
            PointerObj::PkgMember(pkg, index) => Ok(pkgs[*pkg].member(*index).clone()),
        }
    }

    /// cast returns the GosValue the pointer points to, with a new type
    /// obviously this is very unsafe, more rigorous checks should be done here.
    #[inline]
    pub fn cast(
        &self,
        new_type: ValueType,
        stack: &Stack,
        pkgs: &PackageObjs,
    ) -> RuntimeResult<PointerObj> {
        let val = self.deref(stack, pkgs)?;
        let val = val.cast(new_type);
        Ok(PointerObj::new_closed_up_value(&val))
    }

    /// set_value is not used by VM, it's for FFI
    pub fn set_pointee(
        &self,
        val: &GosValue,
        stack: &mut Stack,
        pkgs: &PackageObjs,
        gcc: &GcContainer,
    ) -> RuntimeResult<()> {
        match self {
            PointerObj::UpVal(uv) => uv.set_value(val.copy_semantic(gcc), stack),
            PointerObj::SliceMember(s, index) => {
                s.dispatcher_a_s()
                    .slice_set(s, &val.copy_semantic(gcc), *index as usize)?;
            }
            PointerObj::StructField(s, index) => {
                let target: &mut GosValue =
                    &mut s.as_struct().0.borrow_fields_mut()[*index as usize];
                *target = val.copy_semantic(gcc);
            }
            PointerObj::PkgMember(p, index) => {
                let target: &mut GosValue = &mut pkgs[*p].member_mut(*index);
                *target = val.copy_semantic(gcc);
            }
        }
        Ok(())
    }

    /// for gc
    pub fn ref_sub_one(&self) {
        match &self {
            PointerObj::UpVal(uv) => uv.ref_sub_one(),
            PointerObj::SliceMember(s, _) => {
                let rc = &s.as_gos_slice().unwrap().1;
                rc.set(rc.get() - 1);
            }
            PointerObj::StructField(s, _) => {
                let rc = &s.as_struct().1;
                rc.set(rc.get() - 1);
            }
            _ => {}
        };
    }

    /// for gc
    pub fn mark_dirty(&self, queue: &mut RCQueue) {
        match &self {
            PointerObj::UpVal(uv) => uv.mark_dirty(queue),
            PointerObj::SliceMember(s, _) => {
                rcount_mark_and_queue(&s.as_gos_slice().unwrap().1, queue)
            }
            PointerObj::StructField(s, _) => rcount_mark_and_queue(&s.as_struct().1, queue),
            _ => {}
        };
    }
}

impl Eq for PointerObj {}

impl PartialEq for PointerObj {
    #[inline]
    fn eq(&self, other: &PointerObj) -> bool {
        match (self, other) {
            (Self::UpVal(x), Self::UpVal(y)) => x == y,
            (Self::SliceMember(x, ix), Self::SliceMember(y, iy)) => {
                x.as_addr() == y.as_addr() && ix == iy
            }
            (Self::StructField(x, ix), Self::StructField(y, iy)) => {
                x.as_addr() == y.as_addr() && ix == iy
            }
            (Self::PkgMember(ka, ix), Self::PkgMember(kb, iy)) => ka == kb && ix == iy,
            _ => false,
        }
    }
}

impl Hash for PointerObj {
    fn hash<H: Hasher>(&self, state: &mut H) {
        match self {
            Self::UpVal(x) => x.hash(state),
            Self::SliceMember(s, index) => {
                s.as_addr().hash(state);
                index.hash(state);
            }
            Self::StructField(s, index) => {
                s.as_addr().hash(state);
                index.hash(state);
            }
            Self::PkgMember(p, index) => {
                p.hash(state);
                index.hash(state);
            }
        }
    }
}

impl Display for PointerObj {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UpVal(uv) => write!(f, "{:p}", Rc::as_ptr(&uv.inner)),
            Self::SliceMember(s, i) => write!(f, "{:p}i{}", s.as_addr(), i),
            Self::StructField(s, i) => write!(f, "{:p}i{}", s.as_addr(), i),
            Self::PkgMember(p, i) => write!(f, "{:x}i{}", p.as_usize(), i),
        }
    }
}

// ----------------------------------------------------------------------------
// UnsafePtrObj

pub trait UnsafePtr {
    /// For downcasting
    fn as_any(&self) -> &dyn Any;

    fn eq(&self, _: &dyn UnsafePtr) -> bool {
        panic!("implement your own eq for your type");
    }

    /// for gc
    fn ref_sub_one(&self) {}

    /// for gc
    fn mark_dirty(&self, _: &mut RCQueue) {}

    /// Returns true if the user data can make reference cycles, so that GC can
    fn can_make_cycle(&self) -> bool {
        false
    }

    /// If can_make_cycle returns true, implement this to break cycle
    fn break_cycle(&self) {}
}

impl std::fmt::Debug for dyn UnsafePtr {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}", "unsafe pointer")
    }
}

/// PointerHandle is used when converting a runtime pointer to a unsafe.Default
#[derive(Debug, Clone)]
pub struct PointerHandle {
    ptr: PointerObj,
}

impl UnsafePtr for PointerHandle {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn eq(&self, other: &dyn UnsafePtr) -> bool {
        let a = self.as_any().downcast_ref::<PointerHandle>();
        let b = other.as_any().downcast_ref::<PointerHandle>();
        match b {
            Some(h) => h.ptr == a.unwrap().ptr,
            None => false,
        }
    }

    /// for gc
    fn ref_sub_one(&self) {
        self.ptr.ref_sub_one()
    }

    /// for gc
    fn mark_dirty(&self, q: &mut RCQueue) {
        self.ptr.mark_dirty(q)
    }
}

impl PointerHandle {
    pub fn new(ptr: &GosValue) -> GosValue {
        match ptr.as_pointer() {
            Some(p) => {
                let handle = PointerHandle { ptr: p.clone() };
                GosValue::new_unsafe_ptr(handle)
            }
            None => GosValue::new_nil(ValueType::UnsafePtr),
        }
    }

    /// Get a reference to the pointer handle's ptr.
    pub fn ptr(&self) -> &PointerObj {
        &self.ptr
    }
}

#[derive(Debug, Clone)]
pub struct UnsafePtrObj {
    ptr: Rc<dyn UnsafePtr>,
}

impl UnsafePtrObj {
    pub fn new<T: 'static + UnsafePtr>(p: T) -> UnsafePtrObj {
        UnsafePtrObj { ptr: Rc::new(p) }
    }

    /// Get a reference to the unsafe ptr obj's ptr.
    pub fn ptr(&self) -> &dyn UnsafePtr {
        &*self.ptr
    }

    pub fn as_rust_ptr(&self) -> *const dyn UnsafePtr {
        &*self.ptr
    }

    pub fn downcast_ref<T: Any>(&self) -> RuntimeResult<&T> {
        self.ptr
            .as_any()
            .downcast_ref::<T>()
            .ok_or("Unexpected unsafe pointer type".to_owned())
    }
}

impl Eq for UnsafePtrObj {}

impl PartialEq for UnsafePtrObj {
    #[inline]
    fn eq(&self, other: &UnsafePtrObj) -> bool {
        self.ptr.eq(other.ptr())
    }
}

impl Display for UnsafePtrObj {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:p}", &*self.ptr as *const dyn UnsafePtr)
    }
}
// ----------------------------------------------------------------------------
// ClosureObj

#[derive(Clone)]
pub struct ValueDesc {
    pub func: FunctionKey,
    pub index: OpIndex,
    pub typ: ValueType,
    pub is_local: bool,
    pub stack: Weak<RefCell<Stack>>,
    pub stack_base: OpIndex,
}

impl Eq for ValueDesc {}

impl PartialEq for ValueDesc {
    #[inline]
    fn eq(&self, other: &ValueDesc) -> bool {
        self.stack_base + self.index == other.stack_base + other.index
            && self.stack.ptr_eq(&other.stack)
    }
}

impl ValueDesc {
    pub fn new(func: FunctionKey, index: OpIndex, typ: ValueType, is_local: bool) -> ValueDesc {
        ValueDesc {
            func: func,
            index: index,
            typ: typ,
            is_local: is_local,
            stack: Weak::new(),
            stack_base: 0,
        }
    }

    #[inline]
    pub fn clone_with_stack(&self, stack: Weak<RefCell<Stack>>, stack_base: OpIndex) -> ValueDesc {
        ValueDesc {
            func: self.func,
            index: self.index,
            typ: self.typ,
            is_local: self.is_local,
            stack: stack,
            stack_base: stack_base,
        }
    }

    #[inline]
    pub fn abs_index(&self) -> OpIndex {
        self.stack_base + self.index
    }

    #[inline]
    pub fn load<'a>(&self, stack: &'a Stack) -> Cow<'a, GosValue> {
        let index = self.abs_index();
        let uv_stack = self.stack.upgrade().unwrap();
        if ptr::eq(uv_stack.as_ptr(), stack) {
            Cow::Borrowed(stack.get(index))
        } else {
            Cow::Owned(uv_stack.borrow().get(index).clone())
        }
    }

    #[inline]
    pub fn store(&self, val: GosValue, stack: &mut Stack) {
        let index = self.abs_index();
        let uv_stack = self.stack.upgrade().unwrap();
        if ptr::eq(uv_stack.as_ptr(), stack) {
            stack.set(index, val);
        } else {
            uv_stack.borrow_mut().set(index, val);
        }
    }
}

impl std::fmt::Debug for ValueDesc {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.debug_struct("ValueDesc")
            .field("type", &self.typ)
            .field("is_local", &self.is_local)
            .field("index", &self.abs_index())
            .field("stack", &self.stack.as_ptr())
            .finish()
    }
}

#[derive(Clone, PartialEq, Eq)]
pub enum UpValueState {
    /// Parent CallFrame is still alive, pointing to a local variable
    Open(ValueDesc), // (what func is the var defined, the index of the var)
    // Parent CallFrame is released, pointing to a pointer value in the global pool
    Closed(GosValue),
}

impl std::fmt::Debug for UpValueState {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match &self {
            Self::Open(desc) => write!(f, "UpValue::Open(  {:#?} )", desc),
            Self::Closed(v) => write!(f, "UpValue::Closed(  {:#018x} )", v.data().as_uint()),
        }
    }
}

#[derive(Clone, Debug)]
pub struct UpValue {
    pub inner: Rc<RefCell<UpValueState>>,
}

impl UpValue {
    pub fn new(d: ValueDesc) -> UpValue {
        UpValue {
            inner: Rc::new(RefCell::new(UpValueState::Open(d))),
        }
    }

    pub fn new_closed(v: GosValue) -> UpValue {
        UpValue {
            inner: Rc::new(RefCell::new(UpValueState::Closed(v))),
        }
    }

    pub fn is_open(&self) -> bool {
        match &self.inner.borrow() as &UpValueState {
            UpValueState::Open(_) => true,
            UpValueState::Closed(_) => false,
        }
    }

    pub fn downgrade(&self) -> WeakUpValue {
        WeakUpValue {
            inner: Rc::downgrade(&self.inner),
        }
    }

    pub fn desc(&self) -> ValueDesc {
        let r: &UpValueState = &self.inner.borrow();
        match r {
            UpValueState::Open(d) => d.clone(),
            _ => unreachable!(),
        }
    }

    pub fn close(&self, val: GosValue) {
        *self.inner.borrow_mut() = UpValueState::Closed(val);
    }

    pub fn value<'a>(&self, stack: &'a Stack) -> Cow<'a, GosValue> {
        match &self.inner.borrow() as &UpValueState {
            UpValueState::Open(desc) => desc.load(stack),
            UpValueState::Closed(val) => Cow::Owned(val.clone()),
        }
    }

    pub fn set_value(&self, val: GosValue, stack: &mut Stack) {
        match &mut self.inner.borrow_mut() as &mut UpValueState {
            UpValueState::Open(desc) => desc.store(val, stack),
            UpValueState::Closed(v) => {
                *v = val;
            }
        }
    }

    /// for gc
    pub fn ref_sub_one(&self) {
        let state: &UpValueState = &self.inner.borrow();
        if let UpValueState::Closed(uvs) = state {
            uvs.ref_sub_one()
        }
    }

    /// for gc
    pub fn mark_dirty(&self, queue: &mut RCQueue) {
        let state: &UpValueState = &self.inner.borrow();
        if let UpValueState::Closed(uvs) = state {
            uvs.mark_dirty(queue)
        }
    }
}

impl Eq for UpValue {}

impl PartialEq for UpValue {
    #[inline]
    fn eq(&self, b: &UpValue) -> bool {
        let state_a: &UpValueState = &self.inner.borrow();
        let state_b: &UpValueState = &b.inner.borrow();
        match (state_a, state_b) {
            (UpValueState::Closed(va), UpValueState::Closed(vb)) => {
                if va.typ().copyable() {
                    Rc::ptr_eq(&self.inner, &b.inner)
                } else {
                    va.data().as_addr() == vb.data().as_addr()
                }
            }
            (UpValueState::Open(da), UpValueState::Open(db)) => {
                da.abs_index() == db.abs_index() && Weak::ptr_eq(&da.stack, &db.stack)
            }
            _ => false,
        }
    }
}

impl Hash for UpValue {
    #[inline]
    fn hash<H: Hasher>(&self, state: &mut H) {
        let b: &UpValueState = &self.inner.borrow();
        match b {
            UpValueState::Open(desc) => desc.abs_index().hash(state),
            UpValueState::Closed(_) => Rc::as_ptr(&self.inner).hash(state),
        }
    }
}

#[derive(Clone, Debug)]
pub struct WeakUpValue {
    pub inner: Weak<RefCell<UpValueState>>,
}

impl WeakUpValue {
    pub fn upgrade(&self) -> Option<UpValue> {
        Weak::upgrade(&self.inner).map(|x| UpValue { inner: x })
    }
}

#[derive(Clone, Debug)]
pub struct GosClosureObj {
    pub func: FunctionKey,
    pub uvs: Option<HashMap<usize, UpValue>>,
    pub recv: Option<GosValue>,
    pub meta: Meta,
}

impl GosClosureObj {
    fn new(
        func: FunctionKey,
        up_ptrs: Option<&Vec<ValueDesc>>,
        recv: Option<GosValue>,
        meta: Meta,
    ) -> GosClosureObj {
        let uvs = up_ptrs.map(|uv| {
            uv.iter()
                .enumerate()
                .filter(|(_, x)| !x.is_local)
                .map(|(i, x)| (i, UpValue::new(x.clone())))
                .collect()
        });
        GosClosureObj {
            func,
            uvs,
            recv,
            meta,
        }
    }
}

#[derive(Clone, Debug)]
pub struct FfiClosureObj {
    pub ffi: Rc<dyn Ffi>,
    pub func_name: String,
    pub meta: Meta,
}

#[derive(Clone, Debug)]
pub enum ClosureObj {
    Gos(GosClosureObj),
    Ffi(FfiClosureObj),
}

impl ClosureObj {
    pub fn new_gos(
        func: FunctionKey,
        up_ptrs: Option<&Vec<ValueDesc>>,
        recv: Option<GosValue>,
        meta: Meta,
    ) -> ClosureObj {
        ClosureObj::Gos(GosClosureObj::new(func, up_ptrs, recv, meta))
    }

    pub fn gos_from_func(
        func: FunctionKey,
        fobjs: &FunctionObjs,
        recv: Option<GosValue>,
    ) -> ClosureObj {
        let func_val = &fobjs[func];
        let uvs = if func_val.up_ptrs.is_empty() {
            None
        } else {
            Some(&func_val.up_ptrs)
        };
        ClosureObj::Gos(GosClosureObj::new(func, uvs, recv, func_val.meta))
    }

    #[inline]
    pub fn new_ffi(ffi: FfiClosureObj) -> ClosureObj {
        ClosureObj::Ffi(ffi)
    }

    #[inline]
    pub fn as_gos(&self) -> &GosClosureObj {
        match self {
            Self::Gos(c) => c,
            _ => unreachable!(),
        }
    }

    /// for gc
    pub fn ref_sub_one(&self) {
        match self {
            Self::Gos(obj) => {
                if let Some(uvs) = &obj.uvs {
                    for (_, v) in uvs.iter() {
                        v.ref_sub_one()
                    }
                }
                if let Some(recv) = &obj.recv {
                    recv.ref_sub_one()
                }
            }
            Self::Ffi(_) => {}
        }
    }

    /// for gc
    pub fn mark_dirty(&self, queue: &mut RCQueue) {
        match self {
            Self::Gos(obj) => {
                if let Some(uvs) = &obj.uvs {
                    for (_, v) in uvs.iter() {
                        v.mark_dirty(queue)
                    }
                }
                if let Some(recv) = &obj.recv {
                    recv.mark_dirty(queue)
                }
            }
            Self::Ffi(_) => {}
        }
    }
}

// ----------------------------------------------------------------------------
// PackageVal

/// PackageVal is part of the generated Bytecode, it stores imports, consts,
/// vars, funcs declared in a package
#[derive(Clone, Debug)]
pub struct PackageVal {
    members: Vec<RefCell<GosValue>>, // imports, const, var, func are all stored here
    member_types: Vec<ValueType>,
    member_indices: HashMap<String, OpIndex>,
    init_funcs: Vec<GosValue>,
    // maps func_member_index of the constructor to pkg_member_index
    var_mapping: RefCell<Option<HashMap<OpIndex, OpIndex>>>,
}

impl PackageVal {
    pub fn new() -> PackageVal {
        PackageVal {
            members: vec![],
            member_types: vec![],
            member_indices: HashMap::new(),
            init_funcs: vec![],
            var_mapping: RefCell::new(Some(HashMap::new())),
        }
    }

    pub fn add_member(&mut self, name: String, val: GosValue, typ: ValueType) -> OpIndex {
        self.members.push(RefCell::new(val));
        self.member_types.push(typ);
        let index = (self.members.len() - 1) as OpIndex;
        self.member_indices.insert(name, index);
        index as OpIndex
    }

    pub fn add_var_mapping(&mut self, name: String, fn_index: OpIndex) -> OpIndex {
        let index = *self.get_member_index(&name).unwrap();
        self.var_mapping
            .borrow_mut()
            .as_mut()
            .unwrap()
            .insert(fn_index, index);
        index
    }

    pub fn add_init_func(&mut self, func: GosValue) {
        self.init_funcs.push(func);
    }

    pub fn get_member_index(&self, name: &str) -> Option<&OpIndex> {
        self.member_indices.get(name)
    }

    pub fn inited(&self) -> bool {
        self.var_mapping.borrow().is_none()
    }

    #[inline]
    pub fn member(&self, i: OpIndex) -> Ref<GosValue> {
        self.members[i as usize].borrow()
    }

    #[inline]
    pub fn member_mut(&self, i: OpIndex) -> RefMut<GosValue> {
        self.members[i as usize].borrow_mut()
    }

    #[inline]
    pub fn get_init_func(&self, i: OpIndex) -> Option<&GosValue> {
        self.init_funcs.get(i as usize)
    }

    #[inline]
    pub fn init_vars(&self, vals: Vec<GosValue>) {
        let mut borrow = self.var_mapping.borrow_mut();
        let mapping = borrow.as_ref().unwrap();
        for (i, v) in vals.into_iter().enumerate() {
            let vi = mapping[&(i as OpIndex)];
            *self.member_mut(vi) = v;
        }
        *borrow = None;
    }
}

// ----------------------------------------------------------------------------
// FunctionVal

#[derive(Eq, PartialEq, Copy, Clone, Debug)]
pub enum FuncFlag {
    Default,
    PkgCtor,
    HasDefer,
}

/// FunctionVal is the direct container of the Opcode.
#[derive(Clone, Debug)]
pub struct FunctionVal {
    pub package: PackageKey,
    pub meta: Meta,
    pub flag: FuncFlag,
    pub param_count: OpIndex,
    pub max_write_index: OpIndex,
    pub ret_zeros: Vec<GosValue>,

    pub code: Vec<Instruction>,
    pub pos: Vec<Option<usize>>,
    pub up_ptrs: Vec<ValueDesc>,
    pub local_zeros: Vec<GosValue>,
}

impl FunctionVal {
    pub fn new(
        package: PackageKey,
        meta: Meta,
        metas: &MetadataObjs,
        gcc: &GcContainer,
        flag: FuncFlag,
    ) -> FunctionVal {
        let s = &metas[meta.key].as_signature();
        let ret_zeros = s.results.iter().map(|m| m.zero(metas, gcc)).collect();
        let mut param_count = s.recv.map_or(0, |_| 1);
        param_count += s.params.len() as OpIndex;
        FunctionVal {
            package,
            meta,
            flag,
            param_count,
            max_write_index: 0,
            ret_zeros,
            code: Vec::new(),
            pos: Vec::new(),
            up_ptrs: Vec::new(),
            local_zeros: Vec::new(),
        }
    }

    #[inline]
    pub fn param_count(&self) -> OpIndex {
        self.param_count
    }

    #[inline]
    pub fn local_count(&self) -> OpIndex {
        self.local_zeros.len() as OpIndex
    }

    #[inline]
    pub fn ret_count(&self) -> OpIndex {
        self.ret_zeros.len() as OpIndex
    }

    #[inline]
    pub fn is_ctor(&self) -> bool {
        self.flag == FuncFlag::PkgCtor
    }
}
