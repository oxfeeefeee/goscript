// Copyright 2022 The Goscript Authors. All rights reserved.
// Use of this source code is governed by a BSD-style
// license that can be found in the LICENSE file.

#![macro_use]

#[cfg(feature = "async")]
use crate::channel::Channel;
use crate::ffi::Ffi;
use crate::gc::GcContainer;
use crate::instruction::{Instruction, OpIndex, ValueType};
use crate::metadata::*;
use crate::stack::Stack;
use crate::value::*;

#[cfg(feature = "serde_borsh")]
use borsh::{
    maybestd::io::Result as BorshResult, maybestd::io::Write as BorshWrite, BorshDeserialize,
    BorshSerialize,
};
use go_parser::{Map, MapIter, PiggyVecKey};
use std::any::Any;
use std::borrow::Cow;
use std::cell::{Cell, Ref, RefCell, RefMut};
use std::cmp::Ordering;
use std::fmt::{self, Debug, Display, Write};
use std::hash::{Hash, Hasher};
use std::marker::PhantomData;
use std::ops::Range;
use std::rc::{Rc, Weak};
use std::{panic, ptr, str};

// ----------------------------------------------------------------------------
// MapObj

pub type GosMap = Map<GosValue, GosValue>;

pub type GosMapIter<'a> = MapIter<'a, GosValue, GosValue>;

#[derive(Debug)]
pub struct MapObj {
    map: RefCell<GosMap>,
}

impl MapObj {
    pub fn new() -> MapObj {
        MapObj {
            map: RefCell::new(Map::new()),
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
    pub fn borrow_data_mut(&self) -> RefMut<GosMap> {
        self.map.borrow_mut()
    }

    #[inline]
    pub fn borrow_data(&self) -> Ref<GosMap> {
        self.map.borrow()
    }

    #[inline]
    pub fn clone_inner(&self) -> RefCell<GosMap> {
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

#[derive(Clone, PartialEq, Eq, Debug, PartialOrd, Ord)]
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

    pub fn borrow(&self) -> Ref<GosValue> {
        self.cell.borrow()
    }

    pub fn borrow_mut(&self) -> RefMut<GosValue> {
        self.cell.borrow_mut()
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
    fn into_value(self, t: ValueType) -> GosValue {
        let v = self.cell.into_inner();
        assert!(v.typ() == t);
        v
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
#[cfg_attr(feature = "serde_borsh", derive(BorshDeserialize, BorshSerialize))]
#[derive(Clone, PartialEq, Eq, Debug, PartialOrd, Ord)]
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
    #[inline]
    pub fn into_inner(self) -> T {
        self.cell.into_inner()
    }

    #[inline]
    pub fn slice_into_inner<U>(inner_slice: &[Self]) -> &[U] {
        assert!(core::mem::size_of::<T>() == core::mem::size_of::<U>());
        unsafe { std::mem::transmute(inner_slice) }
    }

    #[inline]
    pub fn slice_into_inner_mut<U>(inner_slice: &mut [Self]) -> &mut [U] {
        assert!(core::mem::size_of::<T>() == core::mem::size_of::<U>());
        unsafe { std::mem::transmute(inner_slice) }
    }

    #[inline]
    pub fn slice_ref_into_inner<U>(inner_slice: Ref<[Self]>) -> Ref<[U]> {
        assert!(core::mem::size_of::<T>() == core::mem::size_of::<U>());
        unsafe { std::mem::transmute(inner_slice) }
    }

    #[inline]
    pub fn slice_ref_into_inner_mut<U>(inner_slice: RefMut<[Self]>) -> RefMut<[U]> {
        assert!(core::mem::size_of::<T>() == core::mem::size_of::<U>());
        unsafe { std::mem::transmute(inner_slice) }
    }

    #[inline]
    fn clone_slice(dst: &mut [Self], src: &[Self]) {
        let d: &mut [T] = Self::slice_into_inner_mut(dst);
        let s: &[T] = &Self::slice_into_inner(src);
        d.copy_from_slice(s)
    }
}

pub trait CellData: Copy + PartialEq + Hash + Debug {
    fn from_value(val: &GosValue) -> Self;

    fn into_value(self, t: ValueType) -> GosValue;
}

macro_rules! impl_cell_data {
    ($typ:ty, $as:tt, $value_type:tt, $new:tt) => {
        impl CellData for $typ {
            fn from_value(val: &GosValue) -> Self {
                *val.$as()
            }

            fn into_value(self, t: ValueType) -> GosValue {
                GosValue::new(t, ValueData::$new(self))
            }
        }
    };
}

impl_cell_data!(u8, as_uint8, Uint8, new_uint8);
impl_cell_data!(u16, as_uint16, Uint16, new_uint16);
impl_cell_data!(u32, as_uint32, Uint32, new_uint32);
impl_cell_data!(u64, as_uint64, Uint64, new_uint64);
impl_cell_data!(usize, as_uint, Uint, new_uint);

pub type Elem8 = CellElem<u8>;
pub type Elem16 = CellElem<u16>;
pub type Elem32 = CellElem<u32>;
pub type Elem64 = CellElem<u64>;
pub type ElemWord = CellElem<usize>;

/// This can be used when any version of Slice/Array returns the same thing
/// kind of unsafe
pub type AnyElem = CellElem<u8>;

impl<T> Hash for CellElem<T>
where
    T: CellData,
{
    fn hash<H: Hasher>(&self, state: &mut H) {
        let d = self.cell.get();
        d.hash(state)
    }
}

impl<T> Element for CellElem<T>
where
    T: CellData,
{
    #[inline]
    fn from_value(val: GosValue) -> Self {
        CellElem {
            cell: Cell::new(T::from_value(&val)),
        }
    }

    #[inline]
    fn into_value(self, t: ValueType) -> GosValue {
        self.cell.get().into_value(t)
    }

    #[inline]
    fn set_value(&self, val: &GosValue) {
        self.cell.set(T::from_value(val));
    }

    #[inline]
    fn copy_or_clone_slice(dst: &mut [Self], src: &[Self]) {
        Self::clone_slice(dst, src)
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
            return Err(format!("index {} out of range", i).to_owned().into());
        }
        Ok(self.borrow_data()[i].clone().into_value(t))
    }

    #[inline(always)]
    pub fn set(&self, i: usize, val: &GosValue) -> RuntimeResult<()> {
        if i >= self.len() {
            return Err(format!("index {} out of range", i).to_owned().into());
        }
        Ok(self.borrow_data()[i].set_value(&val))
    }

    #[inline]
    pub fn size_of_data(&self) -> usize {
        std::mem::size_of::<T>() * self.len()
    }
}

impl<T> ArrayObj<CellElem<T>>
where
    T: CellData,
{
    #[inline]
    pub fn as_raw_slice<U>(&self) -> Ref<[U]> {
        CellElem::<T>::slice_ref_into_inner(self.as_rust_slice())
    }

    #[inline]
    pub fn as_raw_slice_mut<U>(&self) -> RefMut<[U]> {
        CellElem::<T>::slice_ref_into_inner_mut(self.as_rust_slice_mut())
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

impl<T> PartialOrd for ArrayObj<T>
where
    T: Element + PartialEq + Ord,
{
    #[inline]
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl<T> Ord for ArrayObj<T>
where
    T: Element + PartialEq + Ord,
{
    fn cmp(&self, other: &Self) -> Ordering {
        let a = self.borrow_data();
        let b = other.borrow_data();
        let order = a.len().cmp(&b.len());
        if order != Ordering::Equal {
            return order;
        }
        for (i, elem) in self.borrow_data().iter().enumerate() {
            let order = elem.cmp(&b[i]);
            if order != Ordering::Equal {
                return order;
            }
        }

        Ordering::Equal
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
    pub fn swap(&self, i: usize, j: usize) -> RuntimeResult<()> {
        let len = self.len();
        if i >= len {
            Err(format!("index {} out of range", i).into())
        } else if j >= len {
            Err(format!("index {} out of range", j).into())
        } else {
            self.borrow_all_data_mut()
                .swap(i + self.begin(), j + self.begin());
            Ok(())
        }
    }

    #[inline]
    fn borrow_all_data_mut(&self) -> std::cell::RefMut<Vec<T>> {
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
            return Err(format!("index {} out of range", begin).to_owned().into());
        }

        let cap = if max < 0 {
            this_cap
        } else {
            let val = max as usize;
            if val > this_cap {
                return Err(format!("index {} out of range", max).to_owned().into());
            }
            val
        };

        let ei = if end < 0 {
            this_len
        } else {
            let val = this_begin + end as usize;
            if val < bi || val > cap {
                return Err(format!("index {} out of range", end).to_owned().into());
            }
            val
        };

        Ok((bi, ei, cap))
    }
}

impl<T> SliceObj<CellElem<T>>
where
    T: CellData,
{
    #[inline]
    pub fn as_raw_slice<U>(&self) -> Ref<[U]> {
        CellElem::<T>::slice_ref_into_inner(self.as_rust_slice())
    }

    #[inline]
    pub fn as_raw_slice_mut<U>(&self) -> RefMut<[U]> {
        CellElem::<T>::slice_ref_into_inner_mut(self.as_rust_slice_mut())
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

impl StringObj {
    #[inline]
    pub fn with_str(s: &str) -> StringObj {
        let buf: Vec<Elem8> = unsafe { std::mem::transmute(s.as_bytes().to_vec()) };
        Self::with_buf(buf)
    }

    #[inline]
    fn with_buf(buf: Vec<Elem8>) -> StringObj {
        let arr = GosValue::new_non_gc_array(ArrayObj::with_raw_data(buf), ValueType::Uint8);
        SliceObj::with_array(arr, 0, -1).unwrap()
    }

    /// It's safe because strings are readonly
    /// <https://stackoverflow.com/questions/50431702/is-it-safe-and-defined-behavior-to-transmute-between-a-t-and-an-unsafecellt>
    /// <https://doc.rust-lang.org/src/core/str/converts.rs.html#173>
    #[inline]
    pub fn as_str(&self) -> Ref<str> {
        unsafe { std::mem::transmute(self.as_rust_slice()) }
    }

    #[inline]
    pub fn index(&self, i: usize) -> RuntimeResult<GosValue> {
        self.get(i, ValueType::Uint8)
    }

    #[inline]
    pub fn index_elem_u8(&self, i: usize) -> u8 {
        self.index_elem(i).into_inner()
    }

    #[inline]
    pub fn add(&self, other: &StringObj) -> StringObj {
        let mut buf = self.as_rust_slice().to_vec();
        buf.extend_from_slice(&other.as_rust_slice());
        Self::with_buf(buf)
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

impl PartialOrd for StructObj {
    #[inline]
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

// For when used as a BTreeMap key
impl Ord for StructObj {
    fn cmp(&self, other: &Self) -> Ordering {
        let other_fields = other.borrow_fields();
        for (i, f) in self.borrow_fields().iter().enumerate() {
            let order = f.cmp(&other_fields[i]);
            if order != Ordering::Equal {
                return order;
            }
        }
        Ordering::Equal
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
    pub fn new(ffi_obj: Rc<dyn Ffi>, meta: Meta) -> UnderlyingFfi {
        UnderlyingFfi { ffi_obj, meta }
    }
}

/// Info about how to invoke a method of the underlying value
/// of an interface
#[derive(Clone, Debug)]
pub enum IfaceBinding {
    Struct(Rc<RefCell<MethodDesc>>, Option<Vec<OpIndex>>),
    Iface(usize, Option<Vec<OpIndex>>),
}

#[cfg_attr(feature = "serde_borsh", derive(BorshDeserialize, BorshSerialize))]
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
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Gos(x, _), Self::Gos(y, _)) => x == y,
            (Self::Ffi(x), Self::Ffi(y)) => Rc::ptr_eq(&x.ffi_obj, &y.ffi_obj),
            _ => false,
        }
    }
}

impl PartialOrd for InterfaceObj {
    #[inline]
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for InterfaceObj {
    fn cmp(&self, other: &Self) -> Ordering {
        match (self, other) {
            (Self::Gos(x, _), Self::Gos(y, _)) => {
                let xt = x.typ();
                let yt = y.typ();
                if xt == yt {
                    x.cmp(y)
                } else {
                    xt.cmp(&yt)
                }
            }
            (Self::Ffi(x), Self::Ffi(y)) => Rc::as_ptr(&x.ffi_obj).cmp(&Rc::as_ptr(&y.ffi_obj)),
            (Self::Gos(_, _), Self::Ffi(_)) => Ordering::Greater,
            (Self::Ffi(_), Self::Gos(_, _)) => Ordering::Less,
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
#[cfg(feature = "async")]
#[derive(Clone, Debug)]
pub struct ChannelObj {
    pub recv_zero: GosValue,
    pub chan: Channel,
}

#[cfg(feature = "async")]
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
    pub fn new_slice_member(
        val: GosValue,
        i: OpIndex,
        t: ValueType,
        t_elem: ValueType,
    ) -> RuntimeResult<PointerObj> {
        PointerObj::new_slice_member_internal(val, i, t, &ArrCaller::get_slow(t_elem))
    }

    #[inline]
    pub(crate) fn new_array_member_internal(
        val: GosValue,
        i: OpIndex,
        caller: &Box<dyn Dispatcher>,
    ) -> RuntimeResult<PointerObj> {
        let slice = GosValue::slice_array(val, 0, -1, caller)?;
        // todo: index check!
        Ok(PointerObj::SliceMember(slice, i))
    }

    #[inline]
    pub(crate) fn new_slice_member_internal(
        val: GosValue,
        i: OpIndex,
        t: ValueType,
        caller: &Box<dyn Dispatcher>,
    ) -> RuntimeResult<PointerObj> {
        match t {
            ValueType::Array => PointerObj::new_array_member_internal(val, i, caller),
            ValueType::Slice => match val.is_nil() {
                false => Ok(PointerObj::SliceMember(val, i)),
                true => Err("access nil value".to_owned().into()),
            },
            _ => unreachable!(),
        }
    }

    #[inline]
    pub fn deref(&self, stack: &Stack, pkgs: &PackageObjs) -> RuntimeResult<GosValue> {
        match self {
            PointerObj::UpVal(uv) => Ok(uv.value(stack).into_owned()),
            PointerObj::SliceMember(s, index) => s.caller_slow().slice_get(s, *index as usize),
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
                s.caller_slow()
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
    pub(crate) fn ref_sub_one(&self) {
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
    pub(crate) fn mark_dirty(&self, queue: &mut RCQueue) {
        match &self {
            PointerObj::UpVal(uv) => uv.mark_dirty(queue),
            PointerObj::SliceMember(s, _) => {
                rcount_mark_and_queue(&s.as_gos_slice().unwrap().1, queue)
            }
            PointerObj::StructField(s, _) => rcount_mark_and_queue(&s.as_struct().1, queue),
            _ => {}
        };
    }

    #[inline]
    fn order(&self) -> usize {
        match self {
            Self::UpVal(_) => 0,
            Self::SliceMember(_, _) => 1,
            Self::StructField(_, _) => 2,
            Self::PkgMember(_, _) => 3,
        }
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

impl PartialOrd for PointerObj {
    #[inline]
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for PointerObj {
    fn cmp(&self, other: &Self) -> Ordering {
        match (self, other) {
            (Self::UpVal(x), Self::UpVal(y)) => x.cmp(&y),
            (Self::SliceMember(x, ix), Self::SliceMember(y, iy)) => {
                x.as_addr().cmp(&y.as_addr()).then(ix.cmp(&iy))
            }
            (Self::StructField(x, ix), Self::StructField(y, iy)) => {
                x.as_addr().cmp(&y.as_addr()).then(ix.cmp(&iy))
            }
            (Self::PkgMember(ka, ix), Self::PkgMember(kb, iy)) => ka.cmp(&kb).then(ix.cmp(&iy)),
            _ => self.order().cmp(&other.order()),
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
                GosValue::new_unsafe_ptr(Rc::new(handle))
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
    pub fn new(ptr: Rc<dyn UnsafePtr>) -> UnsafePtrObj {
        UnsafePtrObj { ptr }
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
            .ok_or("Unexpected unsafe pointer type".to_owned().into())
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

#[cfg_attr(feature = "serde_borsh", derive(BorshDeserialize, BorshSerialize))]
#[derive(Clone)]
pub struct ValueDesc {
    pub func: FunctionKey,
    pub index: OpIndex,
    pub typ: ValueType,
    pub is_local: bool,
    #[cfg_attr(feature = "serde_borsh", borsh_skip)]
    pub stack: Weak<RefCell<Stack>>,
    #[cfg_attr(feature = "serde_borsh", borsh_skip)]
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
            func,
            index,
            typ,
            is_local,
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
            (UpValueState::Closed(_), UpValueState::Closed(_)) => Rc::ptr_eq(&self.inner, &b.inner),
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

impl PartialOrd for UpValue {
    #[inline]
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for UpValue {
    fn cmp(&self, b: &Self) -> Ordering {
        let state_a: &UpValueState = &self.inner.borrow();
        let state_b: &UpValueState = &b.inner.borrow();
        match (state_a, state_b) {
            (UpValueState::Closed(_), UpValueState::Closed(_)) => {
                Rc::as_ptr(&self.inner).cmp(&Rc::as_ptr(&b.inner))
            }
            (UpValueState::Open(da), UpValueState::Open(db)) => da
                .abs_index()
                .cmp(&db.abs_index())
                .then(Weak::as_ptr(&da.stack).cmp(&Weak::as_ptr(&db.stack))),
            (UpValueState::Open(_), UpValueState::Closed(_)) => Ordering::Greater,
            (UpValueState::Closed(_), UpValueState::Open(_)) => Ordering::Less,
        }
    }
}

#[cfg(feature = "serde_borsh")]
impl BorshSerialize for UpValue {
    fn serialize<W: BorshWrite>(&self, writer: &mut W) -> BorshResult<()> {
        match &self.inner.borrow() as &UpValueState {
            UpValueState::Open(uv) => uv,
            UpValueState::Closed(_) => unreachable!(),
        }
        .serialize(writer)
    }
}

#[cfg(feature = "serde_borsh")]
impl BorshDeserialize for UpValue {
    fn deserialize(buf: &mut &[u8]) -> BorshResult<Self> {
        let uv = ValueDesc::deserialize(buf)?;
        Ok(Self::new(uv))
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

#[cfg_attr(feature = "serde_borsh", derive(BorshDeserialize, BorshSerialize))]
#[derive(Clone, Debug)]
pub struct GosClosureObj {
    pub func: FunctionKey,
    pub uvs: Option<Map<usize, UpValue>>,
    #[cfg_attr(feature = "serde_borsh", borsh_skip)]
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
    pub is_async: bool,
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
// PackageObj

/// PackageObj is part of the generated Bytecode, it stores imports, consts,
/// vars, funcs declared in a package
#[derive(Clone, Debug)]
pub struct PackageObj {
    name: String,
    members: Vec<RefCell<GosValue>>, // imports, const, var, func are all stored here
    member_indices: Map<String, OpIndex>,
    init_funcs: Vec<GosValue>,
    // maps func_member_index of the constructor to pkg_member_index
    var_mapping: RefCell<Option<Map<OpIndex, OpIndex>>>,
}

impl PackageObj {
    pub fn new(name: String) -> PackageObj {
        PackageObj {
            name,
            members: vec![],
            member_indices: Map::new(),
            init_funcs: vec![],
            var_mapping: RefCell::new(Some(Map::new())),
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn add_member(&mut self, name: String, val: GosValue) -> OpIndex {
        self.members.push(RefCell::new(val));
        let index = (self.members.len() - 1) as OpIndex;
        self.member_indices.insert(name, index);
        index as OpIndex
    }

    pub fn add_var_mapping(&mut self, name: String, fn_index: OpIndex) -> OpIndex {
        let index = *self.member_index(&name).unwrap();
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

    pub fn member_indices(&self) -> &Map<String, OpIndex> {
        &self.member_indices
    }

    pub fn member_index(&self, name: &str) -> Option<&OpIndex> {
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
    pub fn init_func(&self, i: OpIndex) -> Option<&GosValue> {
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

#[cfg(feature = "serde_borsh")]
impl BorshSerialize for PackageObj {
    fn serialize<W: BorshWrite>(&self, writer: &mut W) -> BorshResult<()> {
        self.name.serialize(writer)?;
        let members: Vec<GosValue> = self
            .members
            .iter()
            .map(|x| x.clone().into_inner())
            .collect();
        members.serialize(writer)?;
        self.member_indices.serialize(writer)?;
        self.init_funcs.serialize(writer)?;
        self.var_mapping.borrow().serialize(writer)
    }
}

#[cfg(feature = "serde_borsh")]
impl BorshDeserialize for PackageObj {
    fn deserialize(buf: &mut &[u8]) -> BorshResult<Self> {
        let name = String::deserialize(buf)?;
        let members = Vec::<GosValue>::deserialize(buf)?
            .into_iter()
            .map(|x| RefCell::new(x))
            .collect();
        let member_indices = Map::<String, OpIndex>::deserialize(buf)?;
        let init_funcs = Vec::<GosValue>::deserialize(buf)?;
        let var_mapping = RefCell::new(Option::<Map<OpIndex, OpIndex>>::deserialize(buf)?);
        Ok(PackageObj {
            name,
            members,
            member_indices,
            init_funcs,
            var_mapping,
        })
    }
}

// ----------------------------------------------------------------------------
// FunctionObj

#[cfg_attr(feature = "serde_borsh", derive(BorshDeserialize, BorshSerialize))]
#[derive(Eq, PartialEq, Copy, Clone, Debug)]
pub enum FuncFlag {
    Default,
    PkgCtor,
    HasDefer,
}

/// FunctionObj is the direct container of the Opcode.
#[cfg_attr(feature = "serde_borsh", derive(BorshDeserialize, BorshSerialize))]
#[derive(Clone, Debug)]
pub struct FunctionObj {
    pub package: PackageKey,
    pub meta: Meta,
    pub flag: FuncFlag,
    pub param_count: OpIndex,
    pub max_write_index: OpIndex,
    pub ret_zeros: Vec<GosValue>,

    pub code: Vec<Instruction>,
    #[cfg_attr(
        all(feature = "serde_borsh", not(feature = "instruction_pos")),
        borsh_skip
    )]
    pub pos: Vec<Option<u32>>,
    pub up_ptrs: Vec<ValueDesc>,
    pub local_zeros: Vec<GosValue>,
}

impl FunctionObj {
    pub fn new(
        package: PackageKey,
        meta: Meta,
        metas: &MetadataObjs,
        gcc: &GcContainer,
        flag: FuncFlag,
    ) -> FunctionObj {
        let s = &metas[meta.key].as_signature();
        let ret_zeros = s.results.iter().map(|m| m.zero(metas, gcc)).collect();
        let mut param_count = s.recv.map_or(0, |_| 1);
        param_count += s.params.len() as OpIndex;
        FunctionObj {
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
