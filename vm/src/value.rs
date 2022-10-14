// Copyright 2022 The Goscript Authors. All rights reserved.
// Use of this source code is governed by a BSD-style
// license that can be found in the LICENSE file.

#![allow(dead_code)]

#[cfg(feature = "async")]
use crate::channel::Channel;
pub(crate) use crate::dispatcher::*;
use crate::gc::GcContainer;
pub use crate::instruction::*;
pub use crate::metadata::*;
pub use crate::objects::*;
use ordered_float;
use std::cell::Cell;
use std::cmp::Ordering;
use std::collections::VecDeque;
use std::convert::From;
use std::fmt::{self, Debug, Display};
use std::hash::{Hash, Hasher};
use std::num::Wrapping;
use std::ptr;
use std::rc::Rc;
use std::result;

pub type F32 = ordered_float::OrderedFloat<f32>;
pub type F64 = ordered_float::OrderedFloat<f64>;
pub type IRC = i32;
pub type RCount = Cell<IRC>;
pub type RCQueue = VecDeque<IRC>;

#[inline]
pub(crate) fn rcount_mark_and_queue(rc: &RCount, queue: &mut RCQueue) {
    let i = rc.get();
    if i <= 0 {
        queue.push_back(i);
        rc.set(1);
    }
}

fn ref_ptr_eq<T>(x: Option<&T>, y: Option<&T>) -> bool {
    match (x, y) {
        (Some(a), Some(b)) => a as *const T == b as *const T,
        (None, None) => true,
        _ => false,
    }
}

macro_rules! nil_err_str {
    () => {
        "access nil value".to_owned()
    };
}

macro_rules! union_op_wrap {
    ($a:ident, $b:ident, $name:tt, $op:tt) => {
        ValueData {
            $name: (Wrapping($a.$name) $op Wrapping($b.$name)).0,
        }
    };
}

macro_rules! union_op {
    ($a:ident, $b:ident, $name:tt, $op:tt) => {
        ValueData {
            $name: $a.$name $op $b.$name,
        }
    };
}

macro_rules! union_shift {
    ($a:ident, $b:ident, $name:tt, $op:tt) => {
        ValueData {
            $name: $a.$name.$op(*$b).unwrap_or(0),
        }
    };
}

macro_rules! union_cmp {
    ($a:ident, $b:ident, $name:tt, $op:tt) => {
        $a.$name $op $b.$name
    };
}

macro_rules! binary_op_int_float_str {
    ($t:ident, $a:ident, $b:ident, $op:tt) => {
        match $t {
            ValueType::Int => union_op_wrap!($a, $b, int, $op),
            ValueType::Int8 => union_op_wrap!($a, $b, int8, $op),
            ValueType::Int16 => union_op_wrap!($a, $b, int16, $op),
            ValueType::Int32 => union_op_wrap!($a, $b, int32, $op),
            ValueType::Int64 => union_op_wrap!($a, $b, int64, $op),
            ValueType::Uint => union_op_wrap!($a, $b, uint, $op),
            ValueType::UintPtr => union_op_wrap!($a, $b, uint_ptr, $op),
            ValueType::Uint8 => union_op_wrap!($a, $b, uint8, $op),
            ValueType::Uint16 => union_op_wrap!($a, $b, uint16, $op),
            ValueType::Uint32 => union_op_wrap!($a, $b, uint32, $op),
            ValueType::Uint64 => union_op_wrap!($a, $b, uint64, $op),
            ValueType::Float32 => union_op!($a, $b, float32, $op),
            ValueType::Float64 => union_op!($a, $b, float64, $op),
            ValueType::String => $a.add_str($b),
            _ => unreachable!(),
        }
    };
}

macro_rules! binary_op_int_no_wrap {
    ($t:ident, $a:ident, $b:ident, $op:tt) => {
        match $t {
            ValueType::Int => union_op!($a, $b, int, $op),
            ValueType::Int8 => union_op!($a, $b, int8, $op),
            ValueType::Int16 => union_op!($a, $b, int16, $op),
            ValueType::Int32 => union_op!($a, $b, int32, $op),
            ValueType::Int64 => union_op!($a, $b, int64, $op),
            ValueType::Uint => union_op!($a, $b, uint, $op),
            ValueType::UintPtr => union_op!($a, $b, uint_ptr, $op),
            ValueType::Uint8 => union_op!($a, $b, uint8, $op),
            ValueType::Uint16 => union_op!($a, $b, uint16, $op),
            ValueType::Uint32 => union_op!($a, $b, uint32, $op),
            ValueType::Uint64 => union_op!($a, $b, uint64, $op),
            _ => unreachable!(),
        }
    };
}

macro_rules! cmp_bool_int_float {
    ($t:ident, $a:ident, $b:ident, $op:tt) => {
        match $t {
            ValueType::Bool => union_cmp!($a, $b, boolean, $op),
            ValueType::Int => union_cmp!($a, $b, int, $op),
            ValueType::Int8 => union_cmp!($a, $b, int8, $op),
            ValueType::Int16 => union_cmp!($a, $b, int16, $op),
            ValueType::Int32 => union_cmp!($a, $b, int32, $op),
            ValueType::Int64 => union_cmp!($a, $b, int64, $op),
            ValueType::Uint => union_cmp!($a, $b, uint, $op),
            ValueType::UintPtr => union_cmp!($a, $b, uint_ptr, $op),
            ValueType::Uint8 => union_cmp!($a, $b, uint8, $op),
            ValueType::Uint16 => union_cmp!($a, $b, uint16, $op),
            ValueType::Uint32 => union_cmp!($a, $b, uint32, $op),
            ValueType::Uint64 => union_cmp!($a, $b, uint64, $op),
            ValueType::Float32 => union_cmp!($a, $b, float32, $op),
            ValueType::Float64 => union_cmp!($a, $b, float64, $op),
            _ => unreachable!(),
        }
    };
}

macro_rules! cmp_int_float {
    ($t:ident, $a:ident, $b:ident, $op:tt) => {
        match $t {
            ValueType::Int => union_cmp!($a, $b, int, $op),
            ValueType::Int8 => union_cmp!($a, $b, int8, $op),
            ValueType::Int16 => union_cmp!($a, $b, int16, $op),
            ValueType::Int32 => union_cmp!($a, $b, int32, $op),
            ValueType::Int64 => union_cmp!($a, $b, int64, $op),
            ValueType::Uint => union_cmp!($a, $b, uint, $op),
            ValueType::UintPtr => union_cmp!($a, $b, uint_ptr, $op),
            ValueType::Uint8 => union_cmp!($a, $b, uint8, $op),
            ValueType::Uint16 => union_cmp!($a, $b, uint16, $op),
            ValueType::Uint32 => union_cmp!($a, $b, uint32, $op),
            ValueType::Uint64 => union_cmp!($a, $b, uint64, $op),
            ValueType::Float32 => union_cmp!($a, $b, float32, $op),
            ValueType::Float64 => union_cmp!($a, $b, float64, $op),
            _ => unreachable!(),
        }
    };
}

macro_rules! shift_int {
    ($t:ident, $a:ident, $b:ident, $op:tt) => {
        match $t {
            ValueType::Int => union_shift!($a, $b, int, $op),
            ValueType::Int8 => union_shift!($a, $b, int8, $op),
            ValueType::Int16 => union_shift!($a, $b, int16, $op),
            ValueType::Int32 => union_shift!($a, $b, int32, $op),
            ValueType::Int64 => union_shift!($a, $b, int64, $op),
            ValueType::Uint => union_shift!($a, $b, uint, $op),
            ValueType::UintPtr => union_shift!($a, $b, uint_ptr, $op),
            ValueType::Uint8 => union_shift!($a, $b, uint8, $op),
            ValueType::Uint16 => union_shift!($a, $b, uint16, $op),
            ValueType::Uint32 => union_shift!($a, $b, uint32, $op),
            ValueType::Uint64 => union_shift!($a, $b, uint64, $op),
            _ => unreachable!(),
        }
    };
}

macro_rules! convert_to_int {
    ($val:expr, $vt:expr, $d_type:tt, $typ:tt) => {{
        unsafe {
            match $vt {
                ValueType::Uint => $val.$d_type = $val.uint as $typ,
                ValueType::UintPtr => $val.$d_type = $val.uint_ptr as $typ,
                ValueType::Uint8 => $val.$d_type = $val.uint8 as $typ,
                ValueType::Uint16 => $val.$d_type = $val.uint16 as $typ,
                ValueType::Uint32 => $val.$d_type = $val.uint32 as $typ,
                ValueType::Uint64 => $val.$d_type = $val.uint64 as $typ,
                ValueType::Int => $val.$d_type = $val.int as $typ,
                ValueType::Int8 => $val.$d_type = $val.int8 as $typ,
                ValueType::Int16 => $val.$d_type = $val.int16 as $typ,
                ValueType::Int32 => $val.$d_type = $val.int32 as $typ,
                ValueType::Int64 => $val.$d_type = $val.int64 as $typ,
                ValueType::Float32 => $val.$d_type = f32::from($val.float32) as $typ,
                ValueType::Float64 => $val.$d_type = f64::from($val.float64) as $typ,
                _ => unreachable!(),
            }
        }
    }};
}

macro_rules! convert_to_float {
    ($val:expr, $vt:expr, $d_type:tt, $f_type:tt, $typ:tt) => {{
        unsafe {
            match $vt {
                ValueType::Uint => $val.$d_type = $f_type::from($val.uint as $typ),
                ValueType::UintPtr => $val.$d_type = $f_type::from($val.uint_ptr as $typ),
                ValueType::Uint8 => $val.$d_type = $f_type::from($val.uint8 as $typ),
                ValueType::Uint16 => $val.$d_type = $f_type::from($val.uint16 as $typ),
                ValueType::Uint32 => $val.$d_type = $f_type::from($val.uint32 as $typ),
                ValueType::Uint64 => $val.$d_type = $f_type::from($val.uint64 as $typ),
                ValueType::Int => $val.$d_type = $f_type::from($val.int as $typ),
                ValueType::Int8 => $val.$d_type = $f_type::from($val.int8 as $typ),
                ValueType::Int16 => $val.$d_type = $f_type::from($val.int16 as $typ),
                ValueType::Int32 => $val.$d_type = $f_type::from($val.int32 as $typ),
                ValueType::Int64 => $val.$d_type = $f_type::from($val.int64 as $typ),
                ValueType::Float32 => $val.$d_type = $f_type::from(f32::from($val.float32) as $typ),
                ValueType::Float64 => $val.$d_type = $f_type::from(f64::from($val.float64) as $typ),
                _ => unreachable!(),
            }
        }
    }};
}

pub type RuntimeResult<T> = result::Result<T, String>;

pub(crate) type OptionBox<T> = Option<Box<T>>;

pub(crate) type OptionRc<T> = Option<Rc<T>>;

#[derive(Debug, Clone, Copy)]
pub struct Complex64 {
    pub r: F32,
    pub i: F32,
}

#[derive(Debug, Clone, Copy)]
pub struct Complex128 {
    pub r: F64,
    pub i: F64,
}

pub trait AsPrimitive<T>
where
    T: 'static + Copy,
{
    /// Convert a value to another, using the `as` operator.
    fn as_(&self) -> T;
}

// ----------------------------------------------------------------------------
// GosValue

/// Nil is a virtual type representing zero value for pointer, interfaces,
/// maps, slices, channels and function types. For nil-able types, we use
/// null pointer to represent nil value.
pub(crate) union ValueData {
    // untyped_nil is only used in ware cases, when the type of the nil value is ValueType::VOID
    untyped_nil: *const usize,
    boolean: bool,
    int: isize,
    int8: i8,
    int16: i16,
    int32: i32,
    int64: i64,
    uint: usize,
    uint_ptr: usize,
    uint8: u8,
    uint16: u16,
    uint32: u32,
    uint64: u64,
    float32: F32,
    float64: F64,
    complex64: Complex64,
    function: FunctionKey,
    package: PackageKey,
    metadata: *mut Meta, // not visible to users
    complex128: *mut Complex128,
    string: *const StringObj,
    array: *const (GosArrayObj, RCount),
    structure: *const (StructObj, RCount),
    pointer: *mut PointerObj,
    unsafe_ptr: *mut UnsafePtrObj,
    closure: *const (ClosureObj, RCount),
    slice: *const (GosSliceObj, RCount),
    map: *const (MapObj, RCount),
    interface: *const InterfaceObj,
    #[cfg(feature = "async")]
    channel: *const ChannelObj,
}

impl ValueData {
    #[inline]
    fn new_nil(t: ValueType) -> ValueData {
        match t {
            ValueType::Pointer => ValueData {
                pointer: ptr::null_mut(),
            },
            ValueType::UnsafePtr => ValueData {
                unsafe_ptr: ptr::null_mut(),
            },
            ValueType::Closure => ValueData {
                closure: ptr::null(),
            },
            ValueType::Slice => ValueData { slice: ptr::null() },
            ValueType::Map => ValueData { map: ptr::null() },
            ValueType::Interface => ValueData {
                interface: ptr::null(),
            },
            #[cfg(feature = "async")]
            ValueType::Channel => ValueData {
                channel: ptr::null(),
            },
            ValueType::Void => ValueData {
                untyped_nil: ptr::null(),
            },
            _ => unreachable!(),
        }
    }

    #[inline]
    pub(crate) fn new_bool(b: bool) -> ValueData {
        ValueData { boolean: b }
    }

    #[inline]
    pub(crate) fn new_int(i: isize) -> ValueData {
        ValueData { int: i }
    }

    #[inline]
    pub(crate) fn new_int8(i: i8) -> ValueData {
        ValueData { int8: i }
    }

    #[inline]
    pub(crate) fn new_int16(i: i16) -> ValueData {
        ValueData { int16: i }
    }

    #[inline]
    pub(crate) fn new_int32(i: i32) -> ValueData {
        ValueData { int32: i }
    }

    #[inline]
    pub(crate) fn new_int64(i: i64) -> ValueData {
        ValueData { int64: i }
    }

    #[inline]
    pub(crate) fn new_uint(u: usize) -> ValueData {
        ValueData { uint: u }
    }

    #[inline]
    pub(crate) fn new_uint_ptr(u: usize) -> ValueData {
        ValueData { uint_ptr: u }
    }

    #[inline]
    pub(crate) fn new_uint8(u: u8) -> ValueData {
        ValueData { uint8: u }
    }

    #[inline]
    pub(crate) fn new_uint16(u: u16) -> ValueData {
        ValueData { uint16: u }
    }

    #[inline]
    pub(crate) fn new_uint32(u: u32) -> ValueData {
        ValueData { uint32: u }
    }

    #[inline]
    pub(crate) fn new_uint64(u: u64) -> ValueData {
        ValueData { uint64: u }
    }

    #[inline]
    pub(crate) fn new_float32(f: F32) -> ValueData {
        ValueData { float32: f }
    }

    #[inline]
    pub(crate) fn new_float64(f: F64) -> ValueData {
        ValueData { float64: f }
    }

    #[inline]
    pub(crate) fn new_complex64(r: F32, i: F32) -> ValueData {
        ValueData {
            complex64: Complex64 { r: r, i: i },
        }
    }

    #[inline]
    pub(crate) fn new_function(f: FunctionKey) -> ValueData {
        ValueData { function: f }
    }

    #[inline]
    pub(crate) fn new_package(p: PackageKey) -> ValueData {
        ValueData { package: p }
    }

    #[inline]
    pub(crate) fn as_bool(&self) -> &bool {
        unsafe { &self.boolean }
    }

    #[inline]
    pub(crate) fn as_int(&self) -> &isize {
        unsafe { &self.int }
    }

    #[inline]
    pub(crate) fn as_int8(&self) -> &i8 {
        unsafe { &self.int8 }
    }

    #[inline]
    pub(crate) fn as_int16(&self) -> &i16 {
        unsafe { &self.int16 }
    }

    #[inline]
    pub(crate) fn as_int32(&self) -> &i32 {
        unsafe { &self.int32 }
    }

    #[inline]
    pub(crate) fn as_int64(&self) -> &i64 {
        unsafe { &self.int64 }
    }

    #[inline]
    pub(crate) fn as_uint(&self) -> &usize {
        unsafe { &self.uint }
    }

    #[inline]
    pub(crate) fn as_uint_ptr(&self) -> &usize {
        unsafe { &self.uint_ptr }
    }

    #[inline]
    pub(crate) fn as_uint8(&self) -> &u8 {
        unsafe { &self.uint8 }
    }

    #[inline]
    pub(crate) fn as_uint16(&self) -> &u16 {
        unsafe { &self.uint16 }
    }

    #[inline]
    pub(crate) fn as_uint32(&self) -> &u32 {
        unsafe { &self.uint32 }
    }

    #[inline]
    pub(crate) fn as_uint64(&self) -> &u64 {
        unsafe { &self.uint64 }
    }

    #[inline]
    pub(crate) fn as_float32(&self) -> &F32 {
        unsafe { &self.float32 }
    }

    #[inline]
    pub(crate) fn as_float64(&self) -> &F64 {
        unsafe { &self.float64 }
    }

    #[inline]
    pub(crate) fn as_complex64(&self) -> &Complex64 {
        unsafe { &self.complex64 }
    }

    #[inline]
    pub(crate) fn as_function(&self) -> &FunctionKey {
        unsafe { &self.function }
    }

    #[inline]
    pub(crate) fn as_package(&self) -> &PackageKey {
        unsafe { &self.package }
    }

    #[inline]
    pub(crate) fn as_metadata(&self) -> &Meta {
        unsafe { self.metadata.as_ref().unwrap() }
    }

    #[inline]
    pub(crate) fn as_complex128(&self) -> &Complex128 {
        unsafe { &self.complex128.as_ref().unwrap() }
    }

    #[inline]
    pub(crate) fn as_string(&self) -> &StringObj {
        unsafe { &self.string.as_ref().unwrap() }
    }

    #[inline]
    pub(crate) fn as_gos_array(&self) -> &(GosArrayObj, RCount) {
        unsafe { &self.array.as_ref().unwrap() }
    }

    #[inline]
    pub(crate) fn as_array<T>(&self) -> &(ArrayObj<T>, RCount) {
        unsafe {
            let p: *const (ArrayObj<T>, RCount) = std::mem::transmute(self.array);
            &p.as_ref().unwrap()
        }
    }

    #[inline]
    pub(crate) fn as_struct(&self) -> &(StructObj, RCount) {
        unsafe { &self.structure.as_ref().unwrap() }
    }

    #[inline]
    pub(crate) fn as_pointer(&self) -> Option<&PointerObj> {
        unsafe { self.pointer.as_ref() }
    }

    #[inline]
    pub(crate) fn as_unsafe_ptr(&self) -> Option<&UnsafePtrObj> {
        unsafe { self.unsafe_ptr.as_ref() }
    }

    #[inline]
    pub(crate) fn as_closure(&self) -> Option<&(ClosureObj, RCount)> {
        unsafe { self.closure.as_ref() }
    }

    #[inline]
    pub(crate) fn as_slice<T>(&self) -> Option<&(SliceObj<T>, RCount)> {
        unsafe {
            let p: *const (SliceObj<T>, RCount) = std::mem::transmute(self.array);
            p.as_ref()
        }
    }

    #[inline]
    pub(crate) fn as_map(&self) -> Option<&(MapObj, RCount)> {
        unsafe { self.map.as_ref() }
    }

    #[inline]
    pub(crate) fn as_interface(&self) -> Option<&InterfaceObj> {
        unsafe { self.interface.as_ref() }
    }

    #[cfg(feature = "async")]
    #[inline]
    pub(crate) fn as_channel(&self) -> Option<&ChannelObj> {
        unsafe { self.channel.as_ref() }
    }

    #[inline]
    pub(crate) fn as_addr(&self) -> *const usize {
        unsafe { self.untyped_nil }
    }

    #[inline]
    pub(crate) fn int32_as(i: i32, t: ValueType) -> ValueData {
        match t {
            ValueType::Int => ValueData { int: i as isize },
            ValueType::Int8 => ValueData { int8: i as i8 },
            ValueType::Int16 => ValueData { int16: i as i16 },
            ValueType::Int32 => ValueData { int32: i as i32 },
            ValueType::Int64 => ValueData { int64: i as i64 },
            ValueType::Uint => ValueData { uint: i as usize },
            ValueType::UintPtr => ValueData {
                uint_ptr: i as usize,
            },
            ValueType::Uint8 => ValueData { uint8: i as u8 },
            ValueType::Uint16 => ValueData { uint16: i as u16 },
            ValueType::Uint32 => ValueData { uint32: i as u32 },
            ValueType::Uint64 => ValueData { uint64: i as u64 },
            ValueType::Float32 => ValueData {
                float32: F32::from(i as f32),
            },
            ValueType::Float64 => ValueData {
                float64: F64::from(i as f64),
            },
            _ => unreachable!(),
        }
    }

    #[inline]
    pub(crate) fn as_index(&self, t: ValueType) -> usize {
        match t {
            ValueType::Int => *self.as_int() as usize,
            ValueType::Int8 => *self.as_int8() as usize,
            ValueType::Int16 => *self.as_int16() as usize,
            ValueType::Int32 => *self.as_int32() as usize,
            ValueType::Int64 => *self.as_int64() as usize,
            ValueType::Uint => *self.as_uint() as usize,
            ValueType::Uint8 => *self.as_uint8() as usize,
            ValueType::Uint16 => *self.as_uint16() as usize,
            ValueType::Uint32 => *self.as_uint32() as usize,
            ValueType::Uint64 => *self.as_uint64() as usize,
            _ => unreachable!(),
        }
    }

    #[inline]
    pub(crate) fn add_str(&self, b: &ValueData) -> ValueData {
        let s = StrUtil::add(self.as_string(), b.as_string());
        ValueData::new_string(s)
    }

    #[inline]
    pub(crate) fn cast_copyable(&self, from: ValueType, to: ValueType) -> ValueData {
        let mut v = unsafe { self.copy_non_ptr() };
        match to {
            ValueType::Int => convert_to_int!(&mut v, from, int, isize),
            ValueType::Int8 => convert_to_int!(&mut v, from, int8, i8),
            ValueType::Int16 => convert_to_int!(&mut v, from, int16, i16),
            ValueType::Int32 => convert_to_int!(&mut v, from, int32, i32),
            ValueType::Int64 => convert_to_int!(&mut v, from, int64, i64),
            ValueType::Uint => convert_to_int!(&mut v, from, uint, usize),
            ValueType::UintPtr => convert_to_int!(&mut v, from, uint_ptr, usize),
            ValueType::Uint8 => convert_to_int!(&mut v, from, uint8, u8),
            ValueType::Uint16 => convert_to_int!(&mut v, from, uint16, u16),
            ValueType::Uint32 => convert_to_int!(&mut v, from, uint32, u32),
            ValueType::Uint64 => convert_to_int!(&mut v, from, uint64, u64),
            ValueType::Float32 => convert_to_float!(&mut v, from, float32, F32, f32),
            ValueType::Float64 => convert_to_float!(&mut v, from, float64, F64, f64),
            _ => unreachable!(),
        }
        v
    }

    #[inline]
    pub(crate) fn unary_negate(&self, t: ValueType) -> ValueData {
        let mut v = unsafe { self.copy_non_ptr() };
        match t {
            ValueType::Int => v.int = -unsafe { self.int },
            ValueType::Int8 => v.int8 = -unsafe { self.int8 },
            ValueType::Int16 => v.int16 = -unsafe { self.int16 },
            ValueType::Int32 => v.int32 = -unsafe { self.int32 },
            ValueType::Int64 => v.int64 = -unsafe { self.int64 },
            ValueType::Float32 => v.float32 = -unsafe { self.float32 },
            ValueType::Float64 => v.float64 = -unsafe { self.float64 },
            ValueType::Uint => v.uint = unsafe { (!0) ^ self.uint } + 1,
            ValueType::Uint8 => v.uint8 = unsafe { (!0) ^ self.uint8 } + 1,
            ValueType::Uint16 => v.uint16 = unsafe { (!0) ^ self.uint16 } + 1,
            ValueType::Uint32 => v.uint32 = unsafe { (!0) ^ self.uint32 } + 1,
            ValueType::Uint64 => v.uint64 = unsafe { (!0) ^ self.uint64 } + 1,
            _ => unreachable!(),
        };
        v
    }

    #[inline]
    pub(crate) fn unary_xor(&self, t: ValueType) -> ValueData {
        let mut v = unsafe { self.copy_non_ptr() };
        match t {
            ValueType::Uint => v.uint = unsafe { (!0) ^ self.uint },
            ValueType::Uint8 => v.uint8 = unsafe { (!0) ^ self.uint8 },
            ValueType::Uint16 => v.uint16 = unsafe { (!0) ^ self.uint16 },
            ValueType::Uint32 => v.uint32 = unsafe { (!0) ^ self.uint32 },
            ValueType::Uint64 => v.uint64 = unsafe { (!0) ^ self.uint64 },
            ValueType::Int => v.int = unsafe { -1 ^ self.int },
            ValueType::Int8 => v.int8 = unsafe { -1 ^ self.int8 },
            ValueType::Int16 => v.int16 = unsafe { -1 ^ self.int16 },
            ValueType::Int32 => v.int32 = unsafe { -1 ^ self.int32 },
            ValueType::Int64 => v.int64 = unsafe { -1 ^ self.int64 },
            _ => unreachable!(),
        }
        v
    }

    #[inline]
    pub(crate) fn logical_not(&self, t: ValueType) -> ValueData {
        debug_assert!(t == ValueType::Bool);
        let mut v = unsafe { self.copy_non_ptr() };
        v.boolean = unsafe { !self.boolean };
        v
    }

    #[inline]
    pub(crate) fn inc(&self, t: ValueType) -> ValueData {
        let mut v = unsafe { self.copy_non_ptr() };
        match t {
            ValueType::Int => v.int = unsafe { self.int } + 1,
            ValueType::Int8 => v.int8 = unsafe { self.int8 } + 1,
            ValueType::Int16 => v.int16 = unsafe { self.int16 } + 1,
            ValueType::Int32 => v.int32 = unsafe { self.int32 } + 1,
            ValueType::Int64 => v.int64 = unsafe { self.int64 } + 1,
            ValueType::Float32 => v.float32 = unsafe { self.float32 } + 1.0,
            ValueType::Float64 => v.float64 = unsafe { self.float64 } + 1.0,
            ValueType::Uint => v.uint = unsafe { self.uint } + 1,
            ValueType::Uint8 => v.uint8 = unsafe { self.uint8 } + 1,
            ValueType::Uint16 => v.uint16 = unsafe { self.uint16 } + 1,
            ValueType::Uint32 => v.uint32 = unsafe { self.uint32 } + 1,
            ValueType::Uint64 => v.uint64 = unsafe { self.uint64 } + 1,
            _ => unreachable!(),
        };
        v
    }

    #[inline]
    pub(crate) fn dec(&self, t: ValueType) -> ValueData {
        let mut v = unsafe { self.copy_non_ptr() };
        match t {
            ValueType::Int => v.int = unsafe { self.int } - 1,
            ValueType::Int8 => v.int8 = unsafe { self.int8 } - 1,
            ValueType::Int16 => v.int16 = unsafe { self.int16 } - 1,
            ValueType::Int32 => v.int32 = unsafe { self.int32 } - 1,
            ValueType::Int64 => v.int64 = unsafe { self.int64 } - 1,
            ValueType::Float32 => v.float32 = unsafe { self.float32 } - 1.0,
            ValueType::Float64 => v.float64 = unsafe { self.float64 } - 1.0,
            ValueType::Uint => v.uint = unsafe { self.uint } - 1,
            ValueType::Uint8 => v.uint8 = unsafe { self.uint8 } - 1,
            ValueType::Uint16 => v.uint16 = unsafe { self.uint16 } - 1,
            ValueType::Uint32 => v.uint32 = unsafe { self.uint32 } - 1,
            ValueType::Uint64 => v.uint64 = unsafe { self.uint64 } - 1,
            _ => unreachable!(),
        };
        v
    }

    #[inline]
    pub(crate) fn binary_op_add(&self, b: &ValueData, t: ValueType) -> ValueData {
        unsafe { binary_op_int_float_str!(t, self, b, +) }
    }

    #[inline]
    pub(crate) fn binary_op_sub(&self, b: &ValueData, t: ValueType) -> ValueData {
        unsafe { binary_op_int_float_str!(t, self, b, -) }
    }

    #[inline]
    pub(crate) fn binary_op_mul(&self, b: &ValueData, t: ValueType) -> ValueData {
        unsafe { binary_op_int_float_str!(t, self, b, *) }
    }

    #[inline]
    pub(crate) fn binary_op_quo(&self, b: &ValueData, t: ValueType) -> ValueData {
        unsafe { binary_op_int_float_str!(t, self, b, /) }
    }

    #[inline]
    pub(crate) fn binary_op_rem(&self, b: &ValueData, t: ValueType) -> ValueData {
        unsafe { binary_op_int_no_wrap!(t, self, b, %) }
    }

    #[inline]
    pub(crate) fn binary_op_and(&self, b: &ValueData, t: ValueType) -> ValueData {
        unsafe { binary_op_int_no_wrap!(t, self, b, &) }
    }

    #[inline]
    pub(crate) fn binary_op_or(&self, b: &ValueData, t: ValueType) -> ValueData {
        unsafe { binary_op_int_no_wrap!(t, self, b, |) }
    }

    #[inline]
    pub(crate) fn binary_op_xor(&self, b: &ValueData, t: ValueType) -> ValueData {
        unsafe { binary_op_int_no_wrap!(t, self, b, ^) }
    }

    #[inline]
    pub(crate) fn binary_op_shl(&self, b: &u32, t: ValueType) -> ValueData {
        unsafe { shift_int!(t, self, b, checked_shl) }
    }

    #[inline]
    pub(crate) fn binary_op_shr(&self, b: &u32, t: ValueType) -> ValueData {
        unsafe { shift_int!(t, self, b, checked_shr) }
    }

    #[inline]
    pub(crate) fn binary_op_and_not(&self, b: &ValueData, t: ValueType) -> ValueData {
        unsafe {
            match t {
                ValueType::Int => ValueData {
                    int: self.int & !b.int,
                },
                ValueType::Int8 => ValueData {
                    int8: self.int8 & !b.int8,
                },
                ValueType::Int16 => ValueData {
                    int16: self.int16 & !b.int16,
                },
                ValueType::Int32 => ValueData {
                    int32: self.int32 & !b.int32,
                },
                ValueType::Int64 => ValueData {
                    int64: self.int64 & !b.int64,
                },
                ValueType::Uint => ValueData {
                    uint: self.uint & !b.uint,
                },
                ValueType::Uint8 => ValueData {
                    uint8: self.uint8 & !b.uint8,
                },
                ValueType::Uint16 => ValueData {
                    uint16: self.uint16 & !b.uint16,
                },
                ValueType::Uint32 => ValueData {
                    uint32: self.uint32 & !b.uint32,
                },
                ValueType::Uint64 => ValueData {
                    uint64: self.uint64 & !b.uint64,
                },
                _ => unreachable!(),
            }
        }
    }

    #[inline]
    pub(crate) fn compare_eql(&self, b: &ValueData, t: ValueType) -> bool {
        unsafe { cmp_bool_int_float!(t, self, b, ==) }
    }

    #[inline]
    pub(crate) fn compare_neq(&self, b: &ValueData, t: ValueType) -> bool {
        unsafe { cmp_bool_int_float!(t, self, b, !=) }
    }

    #[inline]
    pub(crate) fn compare_lss(&self, b: &ValueData, t: ValueType) -> bool {
        unsafe { cmp_int_float!(t, self, b, <) }
    }

    #[inline]
    pub(crate) fn compare_gtr(&self, b: &ValueData, t: ValueType) -> bool {
        unsafe { cmp_int_float!(t, self, b, >) }
    }

    #[inline]
    pub(crate) fn compare_leq(&self, b: &ValueData, t: ValueType) -> bool {
        unsafe { cmp_int_float!(t, self, b, <=) }
    }

    #[inline]
    pub(crate) fn compare_geq(&self, b: &ValueData, t: ValueType) -> bool {
        unsafe { cmp_int_float!(t, self, b, >=) }
    }

    #[inline]
    pub(crate) fn rc(&self, t: ValueType) -> Option<&Cell<IRC>> {
        match t {
            ValueType::Array => Some(&self.as_gos_array().1),
            ValueType::Closure => self.as_closure().map(|x| &x.1),
            ValueType::Map => self.as_map().map(|x| &x.1),
            ValueType::Struct => Some(&self.as_struct().1),
            _ => unreachable!(),
        }
    }

    pub(crate) fn fmt_debug(
        &self,
        t: ValueType,
        t_elem: ValueType,
        f: &mut fmt::Formatter,
    ) -> fmt::Result {
        match t {
            ValueType::Bool => write!(f, "Type: {:?}, Data: {:?}", t, self.as_bool()),
            ValueType::Int => write!(f, "Type: {:?}, Data: {:?}", t, self.as_int()),
            ValueType::Int8 => write!(f, "Type: {:?}, Data: {:?}", t, self.as_int8()),
            ValueType::Int16 => write!(f, "Type: {:?}, Data: {:?}", t, self.as_int16()),
            ValueType::Int32 => write!(f, "Type: {:?}, Data: {:?}", t, self.as_int32()),
            ValueType::Int64 => write!(f, "Type: {:?}, Data: {:?}", t, self.as_int64()),
            ValueType::Uint => write!(f, "Type: {:?}, Data: {:?}", t, self.as_uint()),
            ValueType::UintPtr => write!(f, "Type: {:?}, Data: {:?}", t, self.as_uint_ptr()),
            ValueType::Uint8 => write!(f, "Type: {:?}, Data: {:?}", t, self.as_uint8()),
            ValueType::Uint16 => write!(f, "Type: {:?}, Data: {:?}", t, self.as_uint16()),
            ValueType::Uint32 => write!(f, "Type: {:?}, Data: {:?}", t, self.as_uint32()),
            ValueType::Uint64 => write!(f, "Type: {:?}, Data: {:?}", t, self.as_uint64()),
            ValueType::Float32 => write!(f, "Type: {:?}, Data: {:?}", t, self.as_float32()),
            ValueType::Float64 => write!(f, "Type: {:?}, Data: {:?}", t, self.as_float64()),
            ValueType::Complex64 => write!(f, "Type: {:?}, Data: {:#?}", t, self.as_complex64()),
            ValueType::Function => write!(f, "Type: {:?}, Data: {:#?}", t, self.as_function()),
            ValueType::Package => write!(f, "Type: {:?}, Data: {:#?}", t, self.as_package()),
            ValueType::Metadata => write!(f, "Type: {:?}, Data: {:#?}", t, self.as_metadata()),
            ValueType::Complex128 => write!(f, "Type: {:?}, Data: {:#?}", t, self.as_complex128()),
            ValueType::String => write!(
                f,
                "Type: {:?}, Data: {:#?}",
                t,
                StrUtil::as_str(&self.clone(t).into_string())
            ),
            ValueType::Array => match t_elem {
                ValueType::Void => write!(f, "Type: {:?}, Data: {:?}", t, "unknown"),
                _ => ArrCaller::get_slow(t_elem).array_debug_fmt(self, f),
            },
            ValueType::Struct => write!(f, "Type: {:?}, Data: {:#?}", t, self.as_struct()),
            ValueType::Pointer => write!(f, "Type: {:?}, Data: {:#?}", t, self.as_pointer()),
            ValueType::UnsafePtr => write!(f, "Type: {:?}, Data: {:#?}", t, self.as_unsafe_ptr()),
            ValueType::Closure => write!(f, "Type: {:?}, Data: {:#?}", t, self.as_closure()),
            ValueType::Slice => match t_elem {
                ValueType::Void => write!(f, "Type: {:?}, Data: {:?}", t, "unknown"),
                _ => ArrCaller::get_slow(t_elem).slice_debug_fmt(self, f),
            },
            ValueType::Map => write!(f, "Type: {:?}, Data: {:#?}", t, self.as_map()),
            ValueType::Interface => write!(f, "Type: {:?}, Data: {:#?}", t, self.as_interface()),
            #[cfg(feature = "async")]
            ValueType::Channel => write!(f, "Type: {:?}, Data: {:#?}", t, self.as_channel()),
            ValueType::Void => write!(
                f,
                "Type: {:?}, Data: {:#018x}/{:?}",
                t,
                self.as_uint(),
                self.as_uint()
            ),
            _ => unreachable!(),
        }
    }

    #[inline]
    pub(crate) unsafe fn copy_non_ptr(&self) -> ValueData {
        self.copy()
    }

    #[inline]
    fn from_metadata(m: Box<Meta>) -> ValueData {
        ValueData {
            metadata: Box::into_raw(m),
        }
    }

    #[inline]
    fn from_complex128(c: Box<Complex128>) -> ValueData {
        ValueData {
            complex128: Box::into_raw(c),
        }
    }

    #[inline]
    fn from_string(s: Rc<StringObj>) -> ValueData {
        ValueData {
            string: Rc::into_raw(s),
        }
    }

    #[inline]
    fn from_array<T>(rc: Rc<(ArrayObj<T>, RCount)>) -> ValueData {
        let p = Rc::into_raw(rc);
        ValueData {
            slice: unsafe { std::mem::transmute(p) },
        }
    }

    #[inline]
    fn from_struct(s: Rc<(StructObj, RCount)>) -> ValueData {
        ValueData {
            structure: Rc::into_raw(s),
        }
    }

    #[inline]
    fn from_pointer(p: OptionBox<PointerObj>) -> ValueData {
        ValueData {
            pointer: p.map_or(ptr::null_mut(), |x| Box::into_raw(x)),
        }
    }

    #[inline]
    fn from_unsafe_ptr(p: OptionBox<UnsafePtrObj>) -> ValueData {
        ValueData {
            unsafe_ptr: p.map_or(ptr::null_mut(), |x| Box::into_raw(x)),
        }
    }

    #[inline]
    fn from_closure(cls: OptionRc<(ClosureObj, RCount)>) -> ValueData {
        ValueData {
            closure: cls.map_or(ptr::null(), |x| Rc::into_raw(x)),
        }
    }

    #[inline]
    fn from_slice<T>(s: OptionRc<(SliceObj<T>, RCount)>) -> ValueData {
        s.map_or(ValueData { slice: ptr::null() }, |x| {
            let p = Rc::into_raw(x);
            ValueData {
                slice: unsafe { std::mem::transmute(p) },
            }
        })
    }

    #[inline]
    fn from_map(m: OptionRc<(MapObj, RCount)>) -> ValueData {
        ValueData {
            map: m.map_or(ptr::null(), |x| Rc::into_raw(x)),
        }
    }

    #[inline]
    fn from_interface(i: OptionRc<InterfaceObj>) -> ValueData {
        ValueData {
            interface: i.map_or(ptr::null(), |x| Rc::into_raw(x)),
        }
    }

    #[cfg(feature = "async")]
    #[inline]
    fn from_channel(c: OptionRc<ChannelObj>) -> ValueData {
        ValueData {
            channel: c.map_or(ptr::null(), |x| Rc::into_raw(x)),
        }
    }

    #[inline]
    fn new_metadata(m: Meta) -> ValueData {
        ValueData::from_metadata(Box::new(m))
    }

    #[inline]
    fn new_complex128(r: F64, i: F64) -> ValueData {
        ValueData::from_complex128(Box::new(Complex128 { r, i }))
    }

    #[inline]
    fn new_string(s: StringObj) -> ValueData {
        ValueData::from_string(Rc::new(s))
    }

    #[inline]
    fn new_array<T>(arr: ArrayObj<T>, gcc: &GcContainer) -> ValueData
    where
        T: Element,
    {
        let rc = Rc::new((arr, Cell::new(0)));
        if T::need_gc() {
            gcc.add_array(&ValueData::from_array(rc.clone()).into_array::<GosElem>());
        }
        ValueData::from_array(rc)
    }

    #[inline]
    fn new_non_gc_array<T>(arr: ArrayObj<T>) -> ValueData
    where
        T: Element,
    {
        debug_assert!(!T::need_gc());
        let rc = Rc::new((arr, Cell::new(0)));
        ValueData::from_array(rc)
    }

    #[inline]
    fn new_struct(obj: StructObj, gcc: &GcContainer) -> ValueData {
        let s = Rc::new((obj, Cell::new(0)));
        gcc.add_struct(&s);
        ValueData::from_struct(s)
    }

    #[inline]
    fn new_pointer(obj: PointerObj) -> ValueData {
        ValueData::from_pointer(Some(Box::new(obj)))
    }

    #[inline]
    fn new_unsafe_ptr<T: 'static + UnsafePtr>(p: T) -> ValueData {
        ValueData::from_unsafe_ptr(Some(Box::new(UnsafePtrObj::new(p))))
    }

    #[inline]
    fn new_closure(obj: ClosureObj, gcc: &GcContainer) -> ValueData {
        let cls = Rc::new((obj, Cell::new(0)));
        gcc.add_closure(&cls);
        ValueData::from_closure(Some(cls))
    }

    #[inline]
    fn new_closure_static(
        func: FunctionKey,
        up_ptrs: Option<&Vec<ValueDesc>>,
        meta: Meta,
    ) -> ValueData {
        let obj = ClosureObj::new_gos(func, up_ptrs, None, meta);
        ValueData::from_closure(Some(Rc::new((obj, Cell::new(0)))))
    }

    #[inline]
    fn new_slice<T>(slice: SliceObj<T>) -> ValueData {
        let rc = Rc::new((slice, Cell::new(0)));
        ValueData::from_slice(Some(rc))
    }

    #[inline]
    fn new_map(obj: MapObj, gcc: &GcContainer) -> ValueData {
        let m = Rc::new((obj, Cell::new(0)));
        gcc.add_map(&m);
        ValueData::from_map(Some(m))
    }

    #[inline]
    fn new_interface(obj: InterfaceObj) -> ValueData {
        ValueData::from_interface(Some(Rc::new(obj)))
    }

    #[cfg(feature = "async")]
    #[inline]
    fn new_channel(obj: ChannelObj) -> ValueData {
        ValueData::from_channel(Some(Rc::new(obj)))
    }

    #[inline]
    fn into_metadata(self) -> Box<Meta> {
        unsafe { Box::from_raw(self.metadata) }
    }

    #[inline]
    fn into_complex128(self) -> Box<Complex128> {
        unsafe { Box::from_raw(self.complex128) }
    }

    #[inline]
    fn into_string(self) -> Rc<StringObj> {
        unsafe { Rc::from_raw(self.string) }
    }

    #[inline]
    fn into_array<T>(self) -> Rc<(ArrayObj<T>, RCount)> {
        let p = unsafe { std::mem::transmute(self.array) };
        unsafe { Rc::from_raw(p) }
    }

    #[inline]
    fn into_struct(self) -> Rc<(StructObj, RCount)> {
        unsafe { Rc::from_raw(self.structure) }
    }

    #[inline]
    fn into_pointer(self) -> OptionBox<PointerObj> {
        unsafe { (!self.pointer.is_null()).then(|| Box::from_raw(self.pointer)) }
    }

    #[inline]
    fn into_unsafe_ptr(self) -> OptionBox<UnsafePtrObj> {
        unsafe { (!self.unsafe_ptr.is_null()).then(|| Box::from_raw(self.unsafe_ptr)) }
    }

    #[inline]
    fn into_closure(self) -> OptionRc<(ClosureObj, RCount)> {
        unsafe { (!self.closure.is_null()).then(|| Rc::from_raw(self.closure)) }
    }

    #[inline]
    fn into_slice<T>(self) -> OptionRc<(SliceObj<T>, RCount)> {
        let p = unsafe { std::mem::transmute(self.slice) };
        unsafe { (!self.slice.is_null()).then(|| Rc::from_raw(p)) }
    }

    #[inline]
    fn into_map(self) -> OptionRc<(MapObj, RCount)> {
        unsafe { (!self.map.is_null()).then(|| Rc::from_raw(self.map)) }
    }

    #[inline]
    fn into_interface(self) -> OptionRc<InterfaceObj> {
        unsafe { (!self.interface.is_null()).then(|| Rc::from_raw(self.interface)) }
    }

    #[cfg(feature = "async")]
    #[inline]
    fn into_channel(self) -> OptionRc<ChannelObj> {
        unsafe { (!self.channel.is_null()).then(|| Rc::from_raw(self.channel)) }
    }

    #[inline]
    fn clone(&self, t: ValueType) -> ValueData {
        match t {
            ValueType::Metadata => ValueData::from_metadata(Box::new(self.as_metadata().clone())),
            ValueType::Complex128 => {
                ValueData::from_complex128(Box::new(self.as_complex128().clone()))
            }
            ValueType::String => unsafe {
                Rc::increment_strong_count(self.string);
                self.copy()
            },
            ValueType::Array => unsafe {
                Rc::increment_strong_count(self.array);
                self.copy()
            },
            ValueType::Struct => unsafe {
                Rc::increment_strong_count(self.structure);
                self.copy()
            },
            ValueType::Pointer => {
                ValueData::from_pointer(self.as_pointer().map(|x| Box::new(x.clone())))
            }
            ValueType::UnsafePtr => {
                ValueData::from_unsafe_ptr(self.as_unsafe_ptr().map(|x| Box::new(x.clone())))
            }
            ValueType::Closure => unsafe {
                if !self.closure.is_null() {
                    Rc::increment_strong_count(self.closure);
                }
                self.copy()
            },
            ValueType::Slice => unsafe {
                if !self.slice.is_null() {
                    Rc::increment_strong_count(self.slice);
                }
                self.copy()
            },
            ValueType::Map => unsafe {
                if !self.map.is_null() {
                    Rc::increment_strong_count(self.map);
                }
                self.copy()
            },
            ValueType::Interface => unsafe {
                if !self.interface.is_null() {
                    Rc::increment_strong_count(self.interface);
                }
                self.copy()
            },
            #[cfg(feature = "async")]
            ValueType::Channel => unsafe {
                if !self.channel.is_null() {
                    Rc::increment_strong_count(self.pointer);
                }
                self.copy()
            },
            _ => self.copy(),
        }
    }

    #[inline]
    fn copy_semantic(&self, t: ValueType, t_elem: ValueType, gcc: &GcContainer) -> ValueData {
        match t {
            _ if t != ValueType::Array
                && t != ValueType::Struct
                && t != ValueType::Slice
                && t != ValueType::Map =>
            {
                self.clone(t)
            }
            ValueType::Array => ArrCaller::get_slow(t_elem).array_copy_semantic(self, gcc),
            ValueType::Struct => ValueData::new_struct(StructObj::clone(&self.as_struct().0), gcc),
            ValueType::Slice => ArrCaller::get_slow(t_elem).slice_copy_semantic(self),
            ValueType::Map => match self.as_map() {
                Some(m) => ValueData::new_map(m.0.clone(), gcc),
                None => ValueData::new_nil(t),
            },
            _ => unreachable!(),
        }
    }

    #[inline]
    fn copy(&self) -> ValueData {
        unsafe { std::mem::transmute_copy(self) }
    }

    #[inline]
    fn drop_as_ptr(&self, t: ValueType, t_elem: ValueType) {
        match t {
            ValueType::Metadata => {
                self.copy().into_metadata();
            }
            ValueType::Complex128 => {
                self.copy().into_complex128();
            }
            ValueType::String => {
                self.copy().into_string();
            }
            // Release mannualy for performance
            ValueType::Array => match ArrCaller::get_elem_type(t_elem) {
                ElemType::ElemType8 => drop(self.copy().into_array::<Elem8>()),
                ElemType::ElemType16 => drop(self.copy().into_array::<Elem16>()),
                ElemType::ElemType32 => drop(self.copy().into_array::<Elem32>()),
                ElemType::ElemType64 => drop(self.copy().into_array::<Elem64>()),
                ElemType::ElemTypeGos => drop(self.copy().into_array::<GosElem>()),
            },
            ValueType::Pointer => {
                self.copy().into_pointer();
            }
            ValueType::UnsafePtr => {
                self.copy().into_unsafe_ptr();
            }
            ValueType::Closure => {
                self.copy().into_closure();
            }
            // Release mannualy for performance
            ValueType::Slice => match ArrCaller::get_elem_type(t_elem) {
                ElemType::ElemType8 => drop(self.copy().into_slice::<Elem8>()),
                ElemType::ElemType16 => drop(self.copy().into_slice::<Elem16>()),
                ElemType::ElemType32 => drop(self.copy().into_slice::<Elem32>()),
                ElemType::ElemType64 => drop(self.copy().into_slice::<Elem64>()),
                ElemType::ElemTypeGos => drop(self.copy().into_slice::<GosElem>()),
            },
            ValueType::Map => {
                self.copy().into_map();
            }
            ValueType::Interface => {
                self.copy().into_interface();
            }
            ValueType::Struct => {
                self.copy().into_struct();
            }
            #[cfg(feature = "async")]
            ValueType::Channel => {
                self.copy().into_channel();
            }
            _ => unreachable!(),
        }
    }
}

impl fmt::Debug for ValueData {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.fmt_debug(ValueType::Void, ValueType::Void, f)
    }
}

pub struct GosValue {
    typ: ValueType,
    t_elem: ValueType,
    data: ValueData,
}

impl GosValue {
    #[inline]
    pub fn typ(&self) -> ValueType {
        self.typ
    }

    #[inline]
    pub fn t_elem(&self) -> ValueType {
        self.t_elem
    }

    /// Get a reference to the gos value's data.
    #[inline]
    pub(crate) fn data(&self) -> &ValueData {
        &self.data
    }

    /// Get a mutable reference to the gos value's data.
    #[inline]
    pub(crate) unsafe fn data_mut(&mut self) -> &mut ValueData {
        &mut self.data
    }

    #[inline]
    pub(crate) fn caller<'a>(&self, c: &'a ArrCaller) -> &'a Box<dyn Dispatcher> {
        c.get(self.t_elem)
    }

    #[inline]
    pub(crate) fn caller_slow(&self) -> Box<dyn Dispatcher> {
        ArrCaller::get_slow(self.t_elem)
    }

    #[inline]
    pub(crate) fn new_nil(t: ValueType) -> GosValue {
        debug_assert!(t != ValueType::Slice);
        GosValue::new(t, ValueData::new_nil(t))
    }

    #[inline]
    pub(crate) fn new_nil_slice(t_elem: ValueType) -> GosValue {
        GosValue::with_elem_type(
            ValueType::Slice,
            t_elem,
            ValueData::new_nil(ValueType::Slice),
        )
    }

    #[inline]
    pub(crate) fn new_uint_ptr(u: usize) -> GosValue {
        GosValue::new(ValueType::UintPtr, ValueData::new_uint_ptr(u))
    }

    #[inline]
    pub(crate) fn new_float32(f: F32) -> GosValue {
        GosValue::new(ValueType::Float32, ValueData::new_float32(f))
    }

    #[inline]
    pub(crate) fn new_float64(f: F64) -> GosValue {
        GosValue::new(ValueType::Float64, ValueData::new_float64(f))
    }

    #[inline]
    pub(crate) fn new_complex64(r: F32, i: F32) -> GosValue {
        GosValue::new(ValueType::Complex64, ValueData::new_complex64(r, i))
    }

    #[inline]
    pub(crate) fn new_function(f: FunctionKey) -> GosValue {
        GosValue::new(ValueType::Function, ValueData::new_function(f))
    }

    #[inline]
    pub(crate) fn new_package(p: PackageKey) -> GosValue {
        GosValue::new(ValueType::Package, ValueData::new_package(p))
    }

    #[inline]
    pub(crate) fn new_metadata(m: Meta) -> GosValue {
        GosValue::new(ValueType::Metadata, ValueData::new_metadata(m))
    }

    #[inline]
    pub(crate) fn new_complex128(r: F64, i: F64) -> GosValue {
        GosValue::new(ValueType::Complex128, ValueData::new_complex128(r, i))
    }

    #[inline]
    pub(crate) fn new_string(s: StringObj) -> GosValue {
        GosValue::new(ValueType::String, ValueData::new_string(s))
    }

    #[inline]
    pub(crate) fn with_str(s: &str) -> GosValue {
        GosValue::new_string(StrUtil::with_str(s))
    }

    #[inline]
    pub(crate) fn new_array<T>(obj: ArrayObj<T>, t_elem: ValueType, gcc: &GcContainer) -> GosValue
    where
        T: Element,
    {
        let data = ValueData::new_array(obj, gcc);
        GosValue::with_elem_type(ValueType::Array, t_elem, data)
    }

    #[inline]
    pub(crate) fn new_non_gc_array<T>(obj: ArrayObj<T>, t_elem: ValueType) -> GosValue
    where
        T: Element,
    {
        let data = ValueData::new_non_gc_array(obj);
        GosValue::with_elem_type(ValueType::Array, t_elem, data)
    }

    #[inline]
    pub(crate) fn new_struct(obj: StructObj, gcc: &GcContainer) -> GosValue {
        let data = ValueData::new_struct(obj, gcc);
        GosValue::new(ValueType::Struct, data)
    }

    #[inline]
    pub(crate) fn new_pointer(obj: PointerObj) -> GosValue {
        GosValue::new(ValueType::Pointer, ValueData::new_pointer(obj))
    }

    #[inline]
    pub(crate) fn new_unsafe_ptr<T: 'static + UnsafePtr>(p: T) -> GosValue {
        GosValue::new(ValueType::UnsafePtr, ValueData::new_unsafe_ptr(p))
    }

    #[inline]
    pub(crate) fn new_closure(obj: ClosureObj, gcc: &GcContainer) -> GosValue {
        let data = ValueData::new_closure(obj, gcc);
        GosValue::new(ValueType::Closure, data)
    }

    #[inline]
    pub(crate) fn new_closure_static(
        func: FunctionKey,
        up_ptrs: Option<&Vec<ValueDesc>>,
        meta: Meta,
    ) -> GosValue {
        GosValue::new(
            ValueType::Closure,
            ValueData::new_closure_static(func, up_ptrs, meta),
        )
    }

    #[inline]
    pub fn new_slice<T>(obj: SliceObj<T>, t_elem: ValueType) -> GosValue
    where
        T: Element,
    {
        GosValue::with_elem_type(ValueType::Slice, t_elem, ValueData::new_slice(obj))
    }

    #[inline]
    pub(crate) fn new_map(gcc: &GcContainer) -> GosValue {
        let data = ValueData::new_map(MapObj::new(), gcc);
        GosValue::new(ValueType::Map, data)
    }

    #[inline]
    pub(crate) fn new_interface(obj: InterfaceObj) -> GosValue {
        GosValue::new(ValueType::Interface, ValueData::new_interface(obj))
    }

    #[cfg(feature = "async")]
    #[inline]
    pub(crate) fn new_channel(obj: ChannelObj) -> GosValue {
        GosValue::new(ValueType::Channel, ValueData::new_channel(obj))
    }

    #[inline]
    pub(crate) fn array_with_size(
        size: usize,
        cap: usize,
        val: &GosValue,
        caller: &Box<dyn Dispatcher>,
        gcc: &GcContainer,
    ) -> GosValue {
        debug_assert!(caller.typ() != ValueType::Void);
        caller.array_with_size(size, cap, val, gcc)
    }

    #[inline]
    pub(crate) fn array_with_data(
        data: Vec<GosValue>,
        caller: &Box<dyn Dispatcher>,
        gcc: &GcContainer,
    ) -> GosValue {
        debug_assert!(caller.typ() != ValueType::Void);
        caller.array_with_data(data, gcc)
    }

    #[inline]
    pub(crate) fn slice_with_size(
        size: usize,
        cap: usize,
        val: &GosValue,
        caller: &Box<dyn Dispatcher>,
        gcc: &GcContainer,
    ) -> GosValue {
        let arr = GosValue::array_with_size(size, cap, val, caller, gcc);
        GosValue::slice_array(arr, 0, size as isize, caller).unwrap()
    }

    #[inline]
    pub(crate) fn slice_with_data(
        data: Vec<GosValue>,
        caller: &Box<dyn Dispatcher>,
        gcc: &GcContainer,
    ) -> GosValue {
        assert!(caller.typ() != ValueType::Void);
        let len = data.len();
        let arr = GosValue::array_with_data(data, caller, gcc);
        GosValue::slice_array(arr, 0, len as isize, caller).unwrap()
    }

    #[inline]
    pub(crate) fn slice_array(
        arr: GosValue,
        begin: isize,
        end: isize,
        caller: &Box<dyn Dispatcher>,
    ) -> RuntimeResult<GosValue> {
        caller.slice_array(arr, begin, end)
    }

    #[inline]
    pub(crate) fn empty_iface_with_val(val: GosValue) -> GosValue {
        GosValue::new_interface(InterfaceObj::with_value(val, None))
    }

    #[cfg(feature = "async")]
    #[inline]
    pub(crate) fn channel_with_chan(chan: Channel, recv_zero: GosValue) -> GosValue {
        GosValue::new_channel(ChannelObj::with_chan(chan, recv_zero))
    }

    #[inline]
    pub(crate) fn from_string(s: Rc<StringObj>) -> GosValue {
        GosValue::new(ValueType::String, ValueData::from_string(s))
    }

    #[inline]
    pub(crate) fn from_gos_array(arr: Rc<(GosArrayObj, RCount)>) -> GosValue {
        GosValue::with_elem_type(
            ValueType::Array,
            ValueType::Void,
            ValueData::from_array(arr),
        )
    }

    #[inline]
    pub(crate) fn from_struct(s: Rc<(StructObj, RCount)>) -> GosValue {
        GosValue::new(ValueType::Struct, ValueData::from_struct(s))
    }

    #[inline]
    pub(crate) fn from_closure(cls: OptionRc<(ClosureObj, RCount)>) -> GosValue {
        GosValue::new(ValueType::Closure, ValueData::from_closure(cls))
    }

    #[inline]
    pub(crate) fn from_slice<T>(s: OptionRc<(SliceObj<T>, RCount)>) -> GosValue {
        GosValue::new(ValueType::Slice, ValueData::from_slice(s))
    }

    #[inline]
    pub(crate) fn from_map(m: OptionRc<(MapObj, RCount)>) -> GosValue {
        GosValue::new(ValueType::Map, ValueData::from_map(m))
    }

    #[inline]
    pub(crate) fn from_interface(i: OptionRc<InterfaceObj>) -> GosValue {
        GosValue::new(ValueType::Interface, ValueData::from_interface(i))
    }

    #[cfg(feature = "async")]
    #[inline]
    pub(crate) fn from_channel(c: OptionRc<ChannelObj>) -> GosValue {
        GosValue::new(ValueType::Channel, ValueData::from_channel(c))
    }

    #[inline]
    pub(crate) fn into_metadata(mut self) -> Box<Meta> {
        debug_assert!(self.typ == ValueType::Metadata);
        self.typ = ValueType::Void;
        self.data.copy().into_metadata()
    }

    #[inline]
    pub(crate) fn into_complex128(mut self) -> Box<Complex128> {
        debug_assert!(self.typ == ValueType::Complex128);
        self.typ = ValueType::Void;
        self.data.copy().into_complex128()
    }

    #[inline]
    pub(crate) fn into_string(mut self) -> Rc<StringObj> {
        debug_assert!(self.typ == ValueType::String);
        self.typ = ValueType::Void;
        self.data.copy().into_string()
    }

    #[inline]
    pub(crate) fn into_array<T>(mut self) -> Rc<(ArrayObj<T>, RCount)> {
        debug_assert!(self.typ == ValueType::Array);
        self.typ = ValueType::Void;
        self.data.copy().into_array()
    }

    #[inline]
    pub(crate) fn into_gos_array(mut self) -> Rc<(GosArrayObj, RCount)> {
        debug_assert!(self.typ == ValueType::Array);
        self.typ = ValueType::Void;
        self.data.copy().into_array()
    }

    #[inline]
    pub(crate) fn into_struct(mut self) -> Rc<(StructObj, RCount)> {
        debug_assert!(self.typ == ValueType::Struct);
        self.typ = ValueType::Void;
        self.data.copy().into_struct()
    }

    #[inline]
    pub(crate) fn into_pointer(mut self) -> OptionBox<PointerObj> {
        debug_assert!(self.typ == ValueType::Pointer);
        self.typ = ValueType::Void;
        self.data.copy().into_pointer()
    }

    #[inline]
    pub(crate) fn into_unsafe_ptr(mut self) -> OptionBox<UnsafePtrObj> {
        debug_assert!(self.typ == ValueType::UnsafePtr);
        self.typ = ValueType::Void;
        self.data.copy().into_unsafe_ptr()
    }

    #[inline]
    pub(crate) fn into_closure(mut self) -> OptionRc<(ClosureObj, RCount)> {
        debug_assert!(self.typ == ValueType::Closure);
        self.typ = ValueType::Void;
        self.data.copy().into_closure()
    }

    #[inline]
    pub(crate) fn into_slice<T>(mut self) -> OptionRc<(SliceObj<T>, RCount)> {
        debug_assert!(self.typ == ValueType::Slice);
        self.typ = ValueType::Void;
        self.data.copy().into_slice()
    }

    #[inline]
    pub(crate) fn into_map(mut self) -> OptionRc<(MapObj, RCount)> {
        debug_assert!(self.typ == ValueType::Map);
        self.typ = ValueType::Void;
        self.data.copy().into_map()
    }

    #[inline]
    pub(crate) fn into_interface(mut self) -> OptionRc<InterfaceObj> {
        debug_assert!(self.typ == ValueType::Interface);
        self.typ = ValueType::Void;
        self.data.copy().into_interface()
    }

    #[cfg(feature = "async")]
    #[inline]
    pub(crate) fn into_channel(mut self) -> OptionRc<ChannelObj> {
        debug_assert!(self.typ == ValueType::Channel);
        self.typ = ValueType::Void;
        self.data.copy().into_channel()
    }

    #[inline]
    pub(crate) fn into_non_nil_pointer(self) -> RuntimeResult<Box<PointerObj>> {
        self.into_pointer().ok_or(nil_err_str!())
    }

    #[inline]
    pub(crate) fn into_non_nil_unsafe_ptr(self) -> RuntimeResult<Box<UnsafePtrObj>> {
        self.into_unsafe_ptr().ok_or(nil_err_str!())
    }

    #[inline]
    pub(crate) fn into_non_nil_closure(self) -> RuntimeResult<Rc<(ClosureObj, RCount)>> {
        self.into_closure().ok_or(nil_err_str!())
    }

    #[inline]
    pub(crate) fn into_non_nil_slice<T>(self) -> RuntimeResult<Rc<(SliceObj<T>, RCount)>> {
        self.into_slice().ok_or(nil_err_str!())
    }

    #[inline]
    pub(crate) fn into_non_nil_map(self) -> RuntimeResult<Rc<(MapObj, RCount)>> {
        self.into_map().ok_or(nil_err_str!())
    }

    #[inline]
    pub(crate) fn into_non_nil_interface(self) -> RuntimeResult<Rc<InterfaceObj>> {
        self.into_interface().ok_or(nil_err_str!())
    }

    #[cfg(feature = "async")]
    #[inline]
    pub(crate) fn into_non_nil_channel(self) -> RuntimeResult<Rc<ChannelObj>> {
        self.into_channel().ok_or(nil_err_str!())
    }

    #[inline]
    pub fn as_bool(&self) -> &bool {
        debug_assert!(self.typ.copyable());
        self.data.as_bool()
    }

    #[inline]
    pub fn as_int(&self) -> &isize {
        debug_assert!(self.typ.copyable());
        self.data.as_int()
    }

    #[inline]
    pub fn as_int8(&self) -> &i8 {
        debug_assert!(self.typ.copyable());
        self.data.as_int8()
    }

    #[inline]
    pub fn as_int16(&self) -> &i16 {
        debug_assert!(self.typ.copyable());
        self.data.as_int16()
    }

    #[inline]
    pub fn as_int32(&self) -> &i32 {
        debug_assert!(self.typ.copyable());
        self.data.as_int32()
    }

    #[inline]
    pub fn as_int64(&self) -> &i64 {
        debug_assert!(self.typ.copyable());
        self.data.as_int64()
    }

    #[inline]
    pub fn as_uint(&self) -> &usize {
        debug_assert!(self.typ.copyable());
        self.data.as_uint()
    }

    #[inline]
    pub fn as_uint_ptr(&self) -> &usize {
        debug_assert!(self.typ.copyable());
        self.data.as_uint_ptr()
    }

    #[inline]
    pub fn as_uint8(&self) -> &u8 {
        debug_assert!(self.typ.copyable());
        self.data.as_uint8()
    }

    #[inline]
    pub fn as_uint16(&self) -> &u16 {
        debug_assert!(self.typ.copyable());
        self.data.as_uint16()
    }

    #[inline]
    pub fn as_uint32(&self) -> &u32 {
        debug_assert!(self.typ.copyable());
        self.data.as_uint32()
    }

    #[inline]
    pub fn as_uint64(&self) -> &u64 {
        debug_assert!(self.typ.copyable());
        self.data.as_uint64()
    }

    #[inline]
    pub fn as_float32(&self) -> &F32 {
        debug_assert!(self.typ.copyable());
        self.data.as_float32()
    }

    #[inline]
    pub fn as_float64(&self) -> &F64 {
        debug_assert!(self.typ.copyable());
        self.data.as_float64()
    }

    #[inline]
    pub fn as_complex64(&self) -> &Complex64 {
        debug_assert!(self.typ.copyable());
        self.data.as_complex64()
    }

    #[inline]
    pub fn as_function(&self) -> &FunctionKey {
        debug_assert!(self.typ.copyable());
        self.data.as_function()
    }

    #[inline]
    pub fn as_package(&self) -> &PackageKey {
        debug_assert!(self.typ.copyable());
        self.data.as_package()
    }

    #[inline]
    pub fn as_metadata(&self) -> &Meta {
        debug_assert!(self.typ == ValueType::Metadata);
        self.data.as_metadata()
    }

    #[inline]
    pub fn as_complex128(&self) -> &Complex128 {
        debug_assert!(self.typ == ValueType::Complex128);
        self.data.as_complex128()
    }

    #[inline]
    pub fn as_string(&self) -> &StringObj {
        debug_assert!(self.typ == ValueType::String);
        self.data.as_string()
    }

    #[inline]
    pub fn as_gos_array(&self) -> &(GosArrayObj, RCount) {
        debug_assert!(self.typ == ValueType::Array);
        self.data.as_array()
    }

    #[inline]
    pub fn as_array<T>(&self) -> &(ArrayObj<T>, RCount) {
        self.data.as_array::<T>()
    }

    #[inline]
    pub fn as_struct(&self) -> &(StructObj, RCount) {
        debug_assert!(self.typ == ValueType::Struct);
        self.data.as_struct()
    }

    #[inline]
    pub fn as_pointer(&self) -> Option<&PointerObj> {
        debug_assert!(self.typ == ValueType::Pointer);
        self.data.as_pointer()
    }

    #[inline]
    pub fn as_unsafe_ptr(&self) -> Option<&UnsafePtrObj> {
        debug_assert!(self.typ == ValueType::UnsafePtr);
        self.data.as_unsafe_ptr()
    }

    #[inline]
    pub fn as_closure(&self) -> Option<&(ClosureObj, RCount)> {
        debug_assert!(self.typ == ValueType::Closure);
        self.data.as_closure()
    }

    #[inline]
    pub fn as_slice<T>(&self) -> Option<&(SliceObj<T>, RCount)> {
        debug_assert!(self.typ == ValueType::Slice || self.typ == ValueType::String);
        self.data.as_slice::<T>()
    }

    #[inline]
    pub fn as_gos_slice(&self) -> Option<&(SliceObj<GosElem>, RCount)> {
        debug_assert!(self.typ == ValueType::Slice);
        self.data.as_slice::<GosElem>()
    }

    #[inline]
    pub fn as_map(&self) -> Option<&(MapObj, RCount)> {
        debug_assert!(self.typ == ValueType::Map);
        self.data.as_map()
    }

    #[inline]
    pub fn as_interface(&self) -> Option<&InterfaceObj> {
        debug_assert!(self.typ == ValueType::Interface);
        self.data.as_interface()
    }

    #[cfg(feature = "async")]
    #[inline]
    pub fn as_channel(&self) -> Option<&ChannelObj> {
        debug_assert!(self.typ == ValueType::Channel);
        self.data.as_channel()
    }

    #[inline]
    pub fn as_non_nil_pointer(&self) -> RuntimeResult<&PointerObj> {
        self.as_pointer().ok_or(nil_err_str!())
    }

    #[inline]
    pub fn as_non_nil_unsafe_ptr(&self) -> RuntimeResult<&UnsafePtrObj> {
        self.as_unsafe_ptr().ok_or(nil_err_str!())
    }

    #[inline]
    pub fn as_non_nil_closure(&self) -> RuntimeResult<&(ClosureObj, RCount)> {
        self.as_closure().ok_or(nil_err_str!())
    }

    #[inline]
    pub fn as_non_nil_slice<T>(&self) -> RuntimeResult<&(SliceObj<T>, RCount)> {
        self.as_slice::<T>().ok_or(nil_err_str!())
    }

    #[inline]
    pub fn as_non_nil_map(&self) -> RuntimeResult<&(MapObj, RCount)> {
        self.as_map().ok_or(nil_err_str!())
    }

    #[inline]
    pub fn as_non_nil_interface(&self) -> RuntimeResult<&InterfaceObj> {
        self.as_interface().ok_or(nil_err_str!())
    }

    #[cfg(feature = "async")]
    #[inline]
    pub fn as_non_nil_channel(&self) -> RuntimeResult<&ChannelObj> {
        self.as_channel().ok_or(nil_err_str!())
    }

    #[inline]
    pub(crate) fn slice_array_equivalent(&self, index: usize) -> RuntimeResult<(&GosValue, usize)> {
        Ok(self
            .as_non_nil_slice::<AnyElem>()?
            .0
            .get_array_equivalent(index))
    }

    pub fn slice_swap(&self, i: usize, j: usize) -> RuntimeResult<()> {
        self.caller_slow().slice_swap(self, i, j)
    }

    #[inline]
    pub(crate) fn int32_as(i: i32, t: ValueType) -> GosValue {
        GosValue::new(t, ValueData::int32_as(i, t))
    }

    #[inline]
    pub fn is_nil(&self) -> bool {
        match self.typ {
            ValueType::Pointer => self.as_pointer().is_none(),
            ValueType::UnsafePtr => self.as_unsafe_ptr().is_none(),
            ValueType::Closure => self.as_closure().is_none(),
            ValueType::Slice => self.as_gos_slice().is_none(),
            ValueType::Map => self.as_map().is_none(),
            ValueType::Interface => self.as_interface().is_none(),
            #[cfg(feature = "async")]
            ValueType::Channel => self.as_channel().is_none(),
            ValueType::Void => true,
            _ => false,
        }
    }

    #[inline]
    pub(crate) fn copy_semantic(&self, gcc: &GcContainer) -> GosValue {
        if self.typ.copyable() {
            GosValue::new(self.typ, self.data.copy())
        } else {
            GosValue::with_elem_type(
                self.typ,
                self.t_elem,
                self.data.copy_semantic(self.typ, self.t_elem, gcc),
            )
        }
    }

    #[inline]
    pub fn cast_copyable(&self, from: ValueType, to: ValueType) -> GosValue {
        assert!(from.copyable());
        assert!(to.copyable());
        GosValue::new(to, self.data.cast_copyable(from, to))
    }

    #[inline]
    pub fn as_index(&self) -> usize {
        debug_assert!(self.typ.copyable());
        self.data.as_index(self.typ)
    }

    #[inline]
    pub fn as_addr(&self) -> *const usize {
        self.data.as_addr()
    }

    #[inline]
    pub fn iface_underlying(&self) -> RuntimeResult<Option<GosValue>> {
        let iface = self.as_non_nil_interface()?;
        Ok(iface.underlying_value().map(|x| x.clone()))
    }

    #[inline]
    pub fn slice_string(
        s: &GosValue,
        begin: isize,
        end: isize,
        max: isize,
    ) -> RuntimeResult<GosValue> {
        Ok(GosValue::new_string(s.as_string().slice(begin, end, max)?))
    }

    #[inline]
    pub fn identical(&self, other: &GosValue) -> bool {
        self.typ() == other.typ() && self == other
    }

    #[inline]
    pub fn len(&self) -> usize {
        match self.typ {
            ValueType::Array => self.as_array::<AnyElem>().0.len(),
            ValueType::Slice => match self.as_slice::<AnyElem>() {
                Some(s) => s.0.len(),
                None => 0,
            },
            ValueType::Map => self.as_map().map_or(0, |x| x.0.len()),
            ValueType::String => self.as_string().len(),
            #[cfg(feature = "async")]
            ValueType::Channel => self.as_channel().map_or(0, |x| x.len()),
            _ => unreachable!(),
        }
    }

    #[inline]
    pub fn cap(&self) -> usize {
        match self.typ {
            ValueType::Slice => match self.as_slice::<AnyElem>() {
                Some(s) => s.0.cap(),
                None => 0,
            },
            #[cfg(feature = "async")]
            ValueType::Channel => self.as_channel().map_or(0, |x| x.cap()),
            _ => unreachable!(),
        }
    }

    /// for gc
    pub fn ref_sub_one(&self) {
        match &self.typ {
            ValueType::Pointer => {
                self.as_pointer().map(|p| p.ref_sub_one());
            }
            ValueType::UnsafePtr => {
                self.as_unsafe_ptr().map(|p| p.ptr().ref_sub_one());
            }
            ValueType::Interface => {
                self.as_interface().map(|x| x.ref_sub_one());
            }
            ValueType::Array => self.as_gos_array().1.set(self.as_gos_array().1.get() - 1),
            ValueType::Struct => self.as_struct().1.set(self.as_struct().1.get() - 1),
            ValueType::Closure => {
                self.as_closure().map(|x| x.1.set(x.1.get() - 1));
            }
            ValueType::Slice => {
                self.as_gos_slice().map(|x| x.0.array().ref_sub_one());
            }
            ValueType::Map => {
                self.as_map().map(|x| x.1.set(x.1.get() - 1));
            }
            _ => {}
        };
    }

    /// for gc
    pub(crate) fn mark_dirty(&self, queue: &mut RCQueue) {
        match &self.typ {
            ValueType::Array => rcount_mark_and_queue(&self.as_gos_array().1, queue),
            ValueType::Pointer => {
                self.as_pointer().map(|x| x.mark_dirty(queue));
            }
            ValueType::UnsafePtr => {
                self.as_unsafe_ptr().map(|x| x.ptr().mark_dirty(queue));
            }
            ValueType::Closure => {
                self.as_closure()
                    .map(|x| rcount_mark_and_queue(&x.1, queue));
            }
            ValueType::Slice => {
                self.as_gos_slice().map(|x| x.0.array().mark_dirty(queue));
            }
            ValueType::Map => {
                self.as_map().map(|x| rcount_mark_and_queue(&x.1, queue));
            }
            ValueType::Interface => {
                self.as_interface().map(|x| x.mark_dirty(queue));
            }
            ValueType::Struct => rcount_mark_and_queue(&self.as_struct().1, queue),
            _ => {}
        };
    }

    #[inline]
    pub(crate) fn rc(&self) -> IRC {
        self.data.rc(self.typ).unwrap().get()
    }

    #[inline]
    pub(crate) fn set_rc(&self, rc: IRC) {
        self.data.rc(self.typ).unwrap().set(rc)
    }

    #[inline]
    pub(crate) fn drop_as_copyable(self) {
        debug_assert!(self.typ.copyable());
        drop(self);
    }

    #[inline]
    pub(crate) fn cast(&self, new_type: ValueType) -> GosValue {
        GosValue::new(new_type, self.data.clone(self.typ))
    }

    #[inline]
    pub(crate) fn new(typ: ValueType, data: ValueData) -> GosValue {
        debug_assert!(typ != ValueType::Slice && typ != ValueType::Array);
        GosValue {
            typ,
            t_elem: ValueType::Void,
            data,
        }
    }

    #[inline]
    fn with_elem_type(typ: ValueType, t_elem: ValueType, data: ValueData) -> GosValue {
        GosValue { typ, t_elem, data }
    }
}

impl Drop for GosValue {
    #[inline]
    fn drop(&mut self) {
        if !self.typ.copyable() {
            self.data.drop_as_ptr(self.typ, self.t_elem);
        }
    }
}

impl Clone for GosValue {
    #[inline]
    fn clone(&self) -> Self {
        if self.typ.copyable() {
            GosValue::new(self.typ, self.data.copy())
        } else {
            GosValue::with_elem_type(self.typ, self.t_elem, self.data.clone(self.typ))
        }
    }
}

impl Eq for GosValue {}

impl PartialEq for GosValue {
    #[inline]
    fn eq(&self, b: &GosValue) -> bool {
        match (self.typ, b.typ) {
            (ValueType::Bool, ValueType::Bool) => self.as_bool().eq(b.as_bool()),
            (ValueType::Int, ValueType::Int) => self.as_int().eq(b.as_int()),
            (ValueType::Int8, ValueType::Int8) => self.as_int8().eq(b.as_int8()),
            (ValueType::Int16, ValueType::Int16) => self.as_int16().eq(b.as_int16()),
            (ValueType::Int32, ValueType::Int32) => self.as_int32().eq(b.as_int32()),
            (ValueType::Int64, ValueType::Int64) => self.as_int64().eq(b.as_int64()),
            (ValueType::Uint, ValueType::Uint) => self.as_uint().eq(b.as_uint()),
            (ValueType::UintPtr, ValueType::UintPtr) => self.as_uint_ptr().eq(b.as_uint_ptr()),
            (ValueType::Uint8, ValueType::Uint8) => self.as_uint8().eq(b.as_uint8()),
            (ValueType::Uint16, ValueType::Uint16) => self.as_uint16().eq(b.as_uint16()),
            (ValueType::Uint32, ValueType::Uint32) => self.as_uint32().eq(b.as_uint32()),
            (ValueType::Uint64, ValueType::Uint64) => self.as_uint64().eq(b.as_uint64()),
            (ValueType::Float32, ValueType::Float32) => self.as_float32().eq(b.as_float32()),
            (ValueType::Float64, ValueType::Float64) => self.as_float64().eq(b.as_float64()),
            (ValueType::Complex64, ValueType::Complex64) => {
                let x = self.as_complex64();
                let y = b.as_complex64();
                x.r == y.r && x.i == y.i
            }
            (ValueType::Function, ValueType::Function) => self.as_function().eq(b.as_function()),
            (ValueType::Package, ValueType::Package) => self.as_package().eq(b.as_package()),
            (ValueType::Metadata, ValueType::Metadata) => self.as_metadata().eq(b.as_metadata()),
            (ValueType::Complex128, ValueType::Complex128) => {
                let x = self.as_complex128();
                let y = b.as_complex128();
                x.r == y.r && x.i == y.i
            }
            (ValueType::String, ValueType::String) => {
                *StrUtil::as_str(self.as_string()) == *StrUtil::as_str(b.as_string())
            }
            (ValueType::Array, ValueType::Array) => {
                self.caller_slow().array_eq(self.data(), b.data())
            }
            (ValueType::Struct, ValueType::Struct) => {
                StructObj::eq(&self.as_struct().0, &b.as_struct().0)
            }
            (ValueType::Pointer, ValueType::Pointer) => self.as_pointer() == b.as_pointer(),
            (ValueType::UnsafePtr, ValueType::UnsafePtr) => {
                self.as_unsafe_ptr() == b.as_unsafe_ptr()
            }
            (ValueType::Closure, ValueType::Closure) => {
                ref_ptr_eq(self.as_closure(), b.as_closure())
            }
            #[cfg(feature = "async")]
            (ValueType::Channel, ValueType::Channel) => {
                ref_ptr_eq(self.as_channel(), b.as_channel())
            }
            (ValueType::Interface, ValueType::Interface) => {
                match (self.as_interface(), b.as_interface()) {
                    (Some(a), Some(b)) => a.eq(b),
                    (None, None) => true,
                    _ => false,
                }
            }
            (_, ValueType::Void) => self.is_nil(),
            (ValueType::Void, _) => b.is_nil(),
            (ValueType::Interface, _) => self
                .as_interface()
                .map_or(b.is_nil(), |x| x.equals_value(b)),
            (_, ValueType::Interface) => b
                .as_interface()
                .map_or(self.is_nil(), |x| x.equals_value(self)),
            _ => false,
        }
    }
}

impl Hash for GosValue {
    fn hash<H: Hasher>(&self, state: &mut H) {
        match &self.typ {
            ValueType::Bool => self.as_bool().hash(state),
            ValueType::Int => self.as_int().hash(state),
            ValueType::Int8 => self.as_int8().hash(state),
            ValueType::Int16 => self.as_int16().hash(state),
            ValueType::Int32 => self.as_int32().hash(state),
            ValueType::Int64 => self.as_int64().hash(state),
            ValueType::Uint => self.as_uint().hash(state),
            ValueType::UintPtr => self.as_uint_ptr().hash(state),
            ValueType::Uint8 => self.as_uint8().hash(state),
            ValueType::Uint16 => self.as_uint16().hash(state),
            ValueType::Uint32 => self.as_uint32().hash(state),
            ValueType::Uint64 => self.as_uint64().hash(state),
            ValueType::Float32 => self.as_float32().hash(state),
            ValueType::Float64 => self.as_float64().hash(state),
            ValueType::Complex64 => self.as_uint64().hash(state),
            ValueType::Function => self.as_function().hash(state),
            ValueType::Package => self.as_package().hash(state),
            ValueType::Metadata => self.as_metadata().hash(state),
            ValueType::String => StrUtil::as_str(self.as_string()).hash(state),
            ValueType::Array => self.caller_slow().array_hash(self, state),
            ValueType::Complex128 => {
                let c = self.as_complex128();
                c.r.hash(state);
                c.i.hash(state);
            }
            ValueType::Struct => {
                self.as_struct().0.hash(state);
            }
            ValueType::Interface => match self.as_interface() {
                Some(iface) => iface.hash(state),
                None => 0.hash(state),
            },
            ValueType::Pointer => match self.as_pointer() {
                Some(p) => PointerObj::hash(&p, state),
                None => 0.hash(state),
            },
            _ => {
                dbg!(self.typ);
                unreachable!();
            }
        }
    }
}

impl PartialOrd for GosValue {
    #[inline]
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for GosValue {
    fn cmp(&self, b: &Self) -> Ordering {
        match (self.typ, b.typ) {
            (ValueType::Bool, ValueType::Bool) => self.as_bool().cmp(b.as_bool()),
            (ValueType::Int, ValueType::Int) => self.as_int().cmp(b.as_int()),
            (ValueType::Int8, ValueType::Int8) => self.as_int8().cmp(b.as_int8()),
            (ValueType::Int16, ValueType::Int16) => self.as_int16().cmp(b.as_int16()),
            (ValueType::Int32, ValueType::Int32) => self.as_int32().cmp(b.as_int32()),
            (ValueType::Int64, ValueType::Int64) => self.as_int64().cmp(b.as_int64()),
            (ValueType::Uint, ValueType::Uint) => self.as_uint().cmp(b.as_uint()),
            (ValueType::UintPtr, ValueType::UintPtr) => self.as_uint_ptr().cmp(b.as_uint_ptr()),
            (ValueType::Uint8, ValueType::Uint8) => self.as_uint8().cmp(b.as_uint8()),
            (ValueType::Uint16, ValueType::Uint16) => self.as_uint16().cmp(b.as_uint16()),
            (ValueType::Uint32, ValueType::Uint32) => self.as_uint32().cmp(b.as_uint32()),
            (ValueType::Uint64, ValueType::Uint64) => self.as_uint64().cmp(b.as_uint64()),
            (ValueType::Float32, ValueType::Float32) => self.as_float32().cmp(b.as_float32()),
            (ValueType::Float64, ValueType::Float64) => self.as_float64().cmp(b.as_float64()),
            (ValueType::String, ValueType::String) => {
                StrUtil::as_str(self.as_string()).cmp(&StrUtil::as_str(b.as_string()))
            }
            (ValueType::Complex64, ValueType::Complex64) => self.as_uint64().cmp(b.as_uint64()),
            (ValueType::Function, ValueType::Function) => self.as_uint64().cmp(b.as_uint64()),
            (ValueType::Package, ValueType::Package) => self.as_uint64().cmp(b.as_uint64()),
            (ValueType::Struct, ValueType::Struct) => self.as_struct().0.cmp(&b.as_struct().0),
            (ValueType::Pointer, ValueType::Pointer) => match (self.as_pointer(), b.as_pointer()) {
                (Some(a), Some(b)) => a.cmp(b),
                (None, None) => Ordering::Equal,
                (Some(_), None) => Ordering::Greater,
                (None, Some(_)) => Ordering::Less,
            },
            (ValueType::Interface, ValueType::Interface) => {
                match (self.as_interface(), b.as_interface()) {
                    (Some(a), Some(b)) => a.cmp(b),
                    (None, None) => Ordering::Equal,
                    (Some(_), None) => Ordering::Greater,
                    (None, Some(_)) => Ordering::Less,
                }
            }
            _ => {
                dbg!(&self, b);
                unreachable!()
            }
        }
    }
}

impl Display for GosValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.typ {
            ValueType::Bool => write!(f, "{}", self.as_bool()),
            ValueType::Int => write!(f, "{}", self.as_int()),
            ValueType::Int8 => write!(f, "{}", self.as_int8()),
            ValueType::Int16 => write!(f, "{}", self.as_int16()),
            ValueType::Int32 => write!(f, "{}", self.as_int32()),
            ValueType::Int64 => write!(f, "{}", self.as_int64()),
            ValueType::Uint => write!(f, "{}", self.as_uint()),
            ValueType::UintPtr => write!(f, "{}", self.as_uint_ptr()),
            ValueType::Uint8 => write!(f, "{}", self.as_uint8()),
            ValueType::Uint16 => write!(f, "{}", self.as_uint16()),
            ValueType::Uint32 => write!(f, "{}", self.as_uint32()),
            ValueType::Uint64 => write!(f, "{}", self.as_uint64()),
            ValueType::Float32 => write!(f, "{}", self.as_float32()),
            ValueType::Float64 => write!(f, "{}", self.as_float64()),
            ValueType::Complex64 => {
                let c = self.as_complex64();
                write!(f, "({}, {})", c.r, c.i)
            }
            ValueType::Function => f.write_str("<function>"),
            ValueType::Package => f.write_str("<package>"),
            ValueType::Metadata => f.write_str("<metadata>"),
            ValueType::Complex128 => {
                let c = self.as_complex128();
                write!(f, "({}, {})", c.r, c.i)
            }
            ValueType::String => f.write_str(&StrUtil::as_str(self.as_string())),
            ValueType::Array => self.caller_slow().array_display_fmt(self.data(), f),
            ValueType::Struct => write!(f, "{}", self.as_struct().0),
            ValueType::Pointer => match self.as_pointer() {
                Some(p) => std::fmt::Display::fmt(p, f),
                None => f.write_str("<nil(pointer)>"),
            },
            ValueType::UnsafePtr => match self.as_unsafe_ptr() {
                Some(p) => std::fmt::Display::fmt(p, f),
                None => f.write_str("<nil(unsafe pointer)>"),
            },
            ValueType::Closure => match self.as_closure() {
                Some(_) => f.write_str("<closure>"),
                None => f.write_str("<nil(closure)>"),
            },
            ValueType::Slice => self.caller_slow().slice_display_fmt(self.data(), f),
            ValueType::Map => match self.as_map() {
                Some(m) => write!(f, "{}", m.0),
                None => f.write_str("<nil(map)>"),
            },
            ValueType::Interface => match self.as_interface() {
                Some(i) => write!(f, "{}", i),
                None => f.write_str("<nil(interface)>"),
            },
            #[cfg(feature = "async")]
            ValueType::Channel => match self.as_channel() {
                Some(_) => f.write_str("<channel>"),
                None => f.write_str("<nil(channel)>"),
            },
            ValueType::Void => f.write_str("<nil(untyped)>"),
            _ => unreachable!(),
        }
    }
}

impl fmt::Debug for GosValue {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.data.fmt_debug(self.typ, self.t_elem, f)
    }
}

impl From<bool> for GosValue {
    #[inline]
    fn from(b: bool) -> Self {
        GosValue::new(ValueType::Bool, ValueData::new_bool(b))
    }
}

impl AsPrimitive<bool> for GosValue {
    #[inline]
    fn as_(&self) -> bool {
        debug_assert!(self.typ.copyable());
        *self.data.as_bool()
    }
}

impl From<isize> for GosValue {
    #[inline]
    fn from(i: isize) -> Self {
        GosValue::new(ValueType::Int, ValueData::new_int(i))
    }
}

impl AsPrimitive<isize> for GosValue {
    #[inline]
    fn as_(&self) -> isize {
        *self.as_int()
    }
}

impl From<i8> for GosValue {
    #[inline]
    fn from(i: i8) -> Self {
        GosValue::new(ValueType::Int8, ValueData::new_int8(i))
    }
}

impl AsPrimitive<i8> for GosValue {
    #[inline]
    fn as_(&self) -> i8 {
        *self.as_int8()
    }
}

impl From<i16> for GosValue {
    #[inline]
    fn from(i: i16) -> Self {
        GosValue::new(ValueType::Int16, ValueData::new_int16(i))
    }
}

impl AsPrimitive<i16> for GosValue {
    #[inline]
    fn as_(&self) -> i16 {
        *self.as_int16()
    }
}

impl From<i32> for GosValue {
    #[inline]
    fn from(i: i32) -> Self {
        GosValue::new(ValueType::Int32, ValueData::new_int32(i))
    }
}

impl AsPrimitive<i32> for GosValue {
    #[inline]
    fn as_(&self) -> i32 {
        *self.as_int32()
    }
}

impl From<i64> for GosValue {
    #[inline]
    fn from(i: i64) -> Self {
        GosValue::new(ValueType::Int64, ValueData::new_int64(i))
    }
}

impl AsPrimitive<i64> for GosValue {
    #[inline]
    fn as_(&self) -> i64 {
        *self.as_int64()
    }
}

impl From<usize> for GosValue {
    #[inline]
    fn from(i: usize) -> Self {
        GosValue::new(ValueType::Uint, ValueData::new_uint(i))
    }
}

impl AsPrimitive<usize> for GosValue {
    #[inline]
    fn as_(&self) -> usize {
        *self.as_uint()
    }
}

impl From<u8> for GosValue {
    #[inline]
    fn from(i: u8) -> Self {
        GosValue::new(ValueType::Uint8, ValueData::new_uint8(i))
    }
}

impl AsPrimitive<u8> for GosValue {
    #[inline]
    fn as_(&self) -> u8 {
        *self.as_uint8()
    }
}

impl From<u16> for GosValue {
    #[inline]
    fn from(i: u16) -> Self {
        GosValue::new(ValueType::Uint16, ValueData::new_uint16(i))
    }
}

impl AsPrimitive<u16> for GosValue {
    #[inline]
    fn as_(&self) -> u16 {
        *self.as_uint16()
    }
}

impl From<u32> for GosValue {
    #[inline]
    fn from(i: u32) -> Self {
        GosValue::new(ValueType::Uint32, ValueData::new_uint32(i))
    }
}

impl AsPrimitive<u32> for GosValue {
    #[inline]
    fn as_(&self) -> u32 {
        *self.as_uint32()
    }
}

impl From<u64> for GosValue {
    #[inline]
    fn from(i: u64) -> Self {
        GosValue::new(ValueType::Uint64, ValueData::new_uint64(i))
    }
}

impl AsPrimitive<u64> for GosValue {
    #[inline]
    fn as_(&self) -> u64 {
        *self.as_uint64()
    }
}

impl From<f32> for GosValue {
    #[inline]
    fn from(f: f32) -> Self {
        GosValue::new(ValueType::Float32, ValueData::new_float32(f.into()))
    }
}

impl AsPrimitive<f32> for GosValue {
    #[inline]
    fn as_(&self) -> f32 {
        self.as_float32().into_inner()
    }
}

impl From<f64> for GosValue {
    #[inline]
    fn from(f: f64) -> Self {
        GosValue::new(ValueType::Float64, ValueData::new_float64(f.into()))
    }
}

impl AsPrimitive<f64> for GosValue {
    #[inline]
    fn as_(&self) -> f64 {
        self.as_float64().into_inner()
    }
}

define_dispatcher!(Dispatcher8, Elem8);
define_dispatcher!(Dispatcher16, Elem16);
define_dispatcher!(Dispatcher32, Elem32);
define_dispatcher!(Dispatcher64, Elem64);
define_dispatcher!(DispatcherGos, GosElem);

#[cfg(test)]
mod test {
    use super::super::value::*;
    use std::cell::RefCell;
    use std::collections::HashMap;
    use std::mem;

    #[test]
    fn test_container() {}

    #[test]
    fn test_size() {
        dbg!(mem::size_of::<HashMap<GosValue, GosValue>>());
        dbg!(mem::size_of::<String>());
        dbg!(mem::size_of::<Rc<String>>());
        dbg!(mem::size_of::<Rc<dyn UnsafePtr>>());
        dbg!(mem::size_of::<Box<Rc<dyn UnsafePtr>>>());
        dbg!(mem::size_of::<RefCell<GosValue>>());
        dbg!(mem::size_of::<GosValue>());
        dbg!(mem::size_of::<ValueData>());
        dbg!(mem::size_of::<Meta>());
        dbg!(mem::size_of::<Box<Meta>>());
        dbg!(mem::size_of::<OptionBox<Meta>>());

        dbg!(mem::size_of::<Option<bool>>());
        dbg!(mem::size_of::<ValueData>());
        dbg!(mem::size_of::<Cell<bool>>());
        dbg!(mem::size_of::<Cell<u8>>());
        dbg!(mem::size_of::<RefCell<u8>>());

        let s = GosValue::with_str("aaa");
        dbg!(s.data());
        //let s2 = s.clone().into_string();
        //dbg!(s2);
    }
}
