//#![allow(dead_code)]
use super::gc::GcoVec;
use super::instruction::{OpIndex, ValueType};
use super::metadata::*;
pub use super::objects::*;
use crate::channel::Channel;
use ordered_float;
use std::cell::{Cell, RefCell};
use std::cmp::Ordering;
use std::collections::VecDeque;
use std::fmt::{self, Display};
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
pub fn rcount_mark_and_queue(rc: &RCount, queue: &mut RCQueue) {
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

macro_rules! rc_ptr_eq {
    ($x:expr, $y:expr) => {
        match ($x, $y) {
            (Some(a), Some(b)) => Rc::ptr_eq(a, b),
            (None, None) => true,
            _ => false,
        }
    };
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
            ValueType::Str => $a.add_str($b),
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
        *$a = match $t {
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

pub type OptionBox<T> = Option<Box<T>>;

pub type OptionRc<T> = Option<Rc<T>>;

// ----------------------------------------------------------------------------
// GosValue

/// Nil is a virtual type representing zero value for pointer, interfaces,
/// maps, slices, channels and function types. For nil-able types, we use
/// null pointer to represent nil value.
pub union ValueData {
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
    complex64: (F32, F32),
    function: FunctionKey,
    package: PackageKey,
    metadata: *mut Meta, // not visible to users
    complex128: *mut (F64, F64),
    str: *const StringObj, // "String" is taken
    array: *const (ArrayObj, RCount),
    structure: *const (RefCell<StructObj>, RCount),
    pointer: *mut PointerObj,
    unsafe_ptr: *mut Rc<dyn UnsafePtr>,
    closure: *const (RefCell<ClosureObj>, RCount),
    slice: *const (SliceObj, RCount),
    map: *const (MapObj, RCount),
    interface: *const RefCell<InterfaceObj>,
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
    pub fn new_bool(b: bool) -> ValueData {
        ValueData { boolean: b }
    }

    #[inline]
    pub fn new_int(i: isize) -> ValueData {
        ValueData { int: i }
    }

    #[inline]
    pub fn new_int8(i: i8) -> ValueData {
        ValueData { int8: i }
    }

    #[inline]
    pub fn new_int16(i: i16) -> ValueData {
        ValueData { int16: i }
    }

    #[inline]
    pub fn new_int32(i: i32) -> ValueData {
        ValueData { int32: i }
    }

    #[inline]
    pub fn new_int64(i: i64) -> ValueData {
        ValueData { int64: i }
    }

    #[inline]
    pub fn new_uint(u: usize) -> ValueData {
        ValueData { uint: u }
    }

    #[inline]
    pub fn new_uint_ptr(u: usize) -> ValueData {
        ValueData { uint_ptr: u }
    }

    #[inline]
    pub fn new_uint8(u: u8) -> ValueData {
        ValueData { uint8: u }
    }

    #[inline]
    pub fn new_uint16(u: u16) -> ValueData {
        ValueData { uint16: u }
    }

    #[inline]
    pub fn new_uint32(u: u32) -> ValueData {
        ValueData { uint32: u }
    }

    #[inline]
    pub fn new_uint64(u: u64) -> ValueData {
        ValueData { uint64: u }
    }

    #[inline]
    pub fn new_float32(f: F32) -> ValueData {
        ValueData { float32: f }
    }

    #[inline]
    pub fn new_float64(f: F64) -> ValueData {
        ValueData { float64: f }
    }

    #[inline]
    pub fn new_complex64(r: F32, i: F32) -> ValueData {
        ValueData { complex64: (r, i) }
    }

    #[inline]
    pub fn new_function(f: FunctionKey) -> ValueData {
        ValueData { function: f }
    }

    #[inline]
    pub fn new_package(p: PackageKey) -> ValueData {
        ValueData { package: p }
    }

    #[inline]
    pub fn as_bool(&self) -> &bool {
        unsafe { &self.boolean }
    }

    #[inline]
    pub fn as_int(&self) -> &isize {
        unsafe { &self.int }
    }

    #[inline]
    pub fn as_int8(&self) -> &i8 {
        unsafe { &self.int8 }
    }

    #[inline]
    pub fn as_int16(&self) -> &i16 {
        unsafe { &self.int16 }
    }

    #[inline]
    pub fn as_int32(&self) -> &i32 {
        unsafe { &self.int32 }
    }

    #[inline]
    pub fn as_int64(&self) -> &i64 {
        unsafe { &self.int64 }
    }

    #[inline]
    pub fn as_uint(&self) -> &usize {
        unsafe { &self.uint }
    }

    #[inline]
    pub fn as_uint_ptr(&self) -> &usize {
        unsafe { &self.uint_ptr }
    }

    #[inline]
    pub fn as_uint8(&self) -> &u8 {
        unsafe { &self.uint8 }
    }

    #[inline]
    pub fn as_uint16(&self) -> &u16 {
        unsafe { &self.uint16 }
    }

    #[inline]
    pub fn as_uint32(&self) -> &u32 {
        unsafe { &self.uint32 }
    }

    #[inline]
    pub fn as_uint64(&self) -> &u64 {
        unsafe { &self.uint64 }
    }

    #[inline]
    pub fn as_float32(&self) -> &F32 {
        unsafe { &self.float32 }
    }

    #[inline]
    pub fn as_float64(&self) -> &F64 {
        unsafe { &self.float64 }
    }

    #[inline]
    pub fn as_complex64(&self) -> &(F32, F32) {
        unsafe { &self.complex64 }
    }

    #[inline]
    pub fn as_function(&self) -> &FunctionKey {
        unsafe { &self.function }
    }

    #[inline]
    pub fn as_package(&self) -> &PackageKey {
        unsafe { &self.package }
    }

    #[inline]
    pub fn as_metadata(&self) -> &Meta {
        unsafe { self.metadata.as_ref().unwrap() }
    }

    #[inline]
    pub fn as_complex128(&self) -> &(F64, F64) {
        unsafe { &self.complex128.as_ref().unwrap() }
    }

    #[inline]
    pub fn as_str(&self) -> &StringObj {
        unsafe { &self.str.as_ref().unwrap() }
    }

    #[inline]
    pub fn as_array(&self) -> &(ArrayObj, RCount) {
        unsafe { &self.array.as_ref().unwrap() }
    }

    #[inline]
    pub fn as_struct(&self) -> &(RefCell<StructObj>, RCount) {
        unsafe { &self.structure.as_ref().unwrap() }
    }

    #[inline]
    pub fn as_pointer(&self) -> Option<&PointerObj> {
        unsafe { self.pointer.as_ref() }
    }

    #[inline]
    pub fn as_unsafe_ptr(&self) -> Option<&Rc<dyn UnsafePtr>> {
        unsafe { self.unsafe_ptr.as_ref() }
    }

    #[inline]
    pub fn as_closure(&self) -> Option<&(RefCell<ClosureObj>, RCount)> {
        unsafe { self.closure.as_ref() }
    }

    #[inline]
    pub fn as_slice(&self) -> Option<&(SliceObj, RCount)> {
        unsafe { self.slice.as_ref() }
    }

    #[inline]
    pub fn as_map(&self) -> Option<&(MapObj, RCount)> {
        unsafe { self.map.as_ref() }
    }

    #[inline]
    pub fn as_interface(&self) -> Option<&RefCell<InterfaceObj>> {
        unsafe { self.interface.as_ref() }
    }

    #[inline]
    pub fn as_channel(&self) -> Option<&ChannelObj> {
        unsafe { self.channel.as_ref() }
    }

    #[inline]
    pub fn as_addr(&self) -> *const usize {
        unsafe { self.untyped_nil }
    }

    #[inline]
    pub fn into_value(self, t: ValueType) -> GosValue {
        GosValue::new(t, self)
    }

    #[inline]
    pub fn int32_as(i: i32, t: ValueType) -> ValueData {
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
    pub fn as_index(&self, t: ValueType) -> usize {
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
    pub fn add_str(&self, b: &ValueData) -> ValueData {
        let mut s = self.as_str().as_str().to_string();
        s.push_str(b.as_str().as_str());
        ValueData::new_str(s)
    }

    #[inline]
    pub fn cast_copyable(&mut self, from: ValueType, to: ValueType) {
        match to {
            ValueType::Int => convert_to_int!(self, from, int, isize),
            ValueType::Int8 => convert_to_int!(self, from, int8, i8),
            ValueType::Int16 => convert_to_int!(self, from, int16, i16),
            ValueType::Int32 => convert_to_int!(self, from, int32, i32),
            ValueType::Int64 => convert_to_int!(self, from, int64, i64),
            ValueType::Uint => convert_to_int!(self, from, uint, usize),
            ValueType::UintPtr => convert_to_int!(self, from, uint_ptr, usize),
            ValueType::Uint8 => convert_to_int!(self, from, uint8, u8),
            ValueType::Uint16 => convert_to_int!(self, from, uint16, u16),
            ValueType::Uint32 => convert_to_int!(self, from, uint32, u32),
            ValueType::Uint64 => convert_to_int!(self, from, uint64, u64),
            ValueType::Float32 => convert_to_float!(self, from, float32, F32, f32),
            ValueType::Float64 => convert_to_float!(self, from, float64, F64, f64),
            _ => unreachable!(),
        }
    }

    #[inline]
    pub fn unary_negate(&mut self, t: ValueType) {
        match t {
            ValueType::Int => self.int = -unsafe { self.int },
            ValueType::Int8 => self.int8 = -unsafe { self.int8 },
            ValueType::Int16 => self.int16 = -unsafe { self.int16 },
            ValueType::Int32 => self.int32 = -unsafe { self.int32 },
            ValueType::Int64 => self.int64 = -unsafe { self.int64 },
            ValueType::Float32 => self.float32 = -unsafe { self.float32 },
            ValueType::Float64 => self.float64 = -unsafe { self.float64 },
            ValueType::Uint => self.uint = unsafe { (!0) ^ self.uint } + 1,
            ValueType::Uint8 => self.uint8 = unsafe { (!0) ^ self.uint8 } + 1,
            ValueType::Uint16 => self.uint16 = unsafe { (!0) ^ self.uint16 } + 1,
            ValueType::Uint32 => self.uint32 = unsafe { (!0) ^ self.uint32 } + 1,
            ValueType::Uint64 => self.uint64 = unsafe { (!0) ^ self.uint64 } + 1,
            _ => unreachable!(),
        }
    }

    #[inline]
    pub fn unary_xor(&mut self, t: ValueType) {
        match t {
            ValueType::Uint => self.uint = unsafe { (!0) ^ self.uint },
            ValueType::Uint8 => self.uint8 = unsafe { (!0) ^ self.uint8 },
            ValueType::Uint16 => self.uint16 = unsafe { (!0) ^ self.uint16 },
            ValueType::Uint32 => self.uint32 = unsafe { (!0) ^ self.uint32 },
            ValueType::Uint64 => self.uint64 = unsafe { (!0) ^ self.uint64 },
            ValueType::Int => self.int = unsafe { -1 ^ self.int },
            ValueType::Int8 => self.int8 = unsafe { -1 ^ self.int8 },
            ValueType::Int16 => self.int16 = unsafe { -1 ^ self.int16 },
            ValueType::Int32 => self.int32 = unsafe { -1 ^ self.int32 },
            ValueType::Int64 => self.int64 = unsafe { -1 ^ self.int64 },
            _ => unreachable!(),
        }
    }

    #[inline]
    pub fn unary_not(&mut self, t: ValueType) {
        debug_assert!(t == ValueType::Bool);
        self.boolean = unsafe { !self.boolean };
    }

    #[inline]
    pub fn inc(&mut self, t: ValueType) {
        match t {
            ValueType::Int => self.int = unsafe { self.int } + 1,
            ValueType::Int8 => self.int8 = unsafe { self.int8 } + 1,
            ValueType::Int16 => self.int16 = unsafe { self.int16 } + 1,
            ValueType::Int32 => self.int32 = unsafe { self.int32 } + 1,
            ValueType::Int64 => self.int64 = unsafe { self.int64 } + 1,
            ValueType::Float32 => self.float32 = unsafe { self.float32 } + 1.0,
            ValueType::Float64 => self.float64 = unsafe { self.float64 } + 1.0,
            ValueType::Uint => self.uint = unsafe { self.uint } + 1,
            ValueType::Uint8 => self.uint8 = unsafe { self.uint8 } + 1,
            ValueType::Uint16 => self.uint16 = unsafe { self.uint16 } + 1,
            ValueType::Uint32 => self.uint32 = unsafe { self.uint32 } + 1,
            ValueType::Uint64 => self.uint64 = unsafe { self.uint64 } + 1,
            _ => unreachable!(),
        }
    }

    #[inline]
    pub fn dec(&mut self, t: ValueType) {
        match t {
            ValueType::Int => self.int = unsafe { self.int } - 1,
            ValueType::Int8 => self.int8 = unsafe { self.int8 } - 1,
            ValueType::Int16 => self.int16 = unsafe { self.int16 } - 1,
            ValueType::Int32 => self.int32 = unsafe { self.int32 } - 1,
            ValueType::Int64 => self.int64 = unsafe { self.int64 } - 1,
            ValueType::Float32 => self.float32 = unsafe { self.float32 } - 1.0,
            ValueType::Float64 => self.float64 = unsafe { self.float64 } - 1.0,
            ValueType::Uint => self.uint = unsafe { self.uint } - 1,
            ValueType::Uint8 => self.uint8 = unsafe { self.uint8 } - 1,
            ValueType::Uint16 => self.uint16 = unsafe { self.uint16 } - 1,
            ValueType::Uint32 => self.uint32 = unsafe { self.uint32 } - 1,
            ValueType::Uint64 => self.uint64 = unsafe { self.uint64 } - 1,
            _ => unreachable!(),
        }
    }

    #[inline]
    pub fn binary_op_add(&self, b: &ValueData, t: ValueType) -> ValueData {
        unsafe { binary_op_int_float_str!(t, self, b, +) }
    }

    #[inline]
    pub fn binary_op_sub(&self, b: &ValueData, t: ValueType) -> ValueData {
        unsafe { binary_op_int_float_str!(t, self, b, -) }
    }

    #[inline]
    pub fn binary_op_mul(&self, b: &ValueData, t: ValueType) -> ValueData {
        unsafe { binary_op_int_float_str!(t, self, b, *) }
    }

    #[inline]
    pub fn binary_op_quo(&self, b: &ValueData, t: ValueType) -> ValueData {
        unsafe { binary_op_int_float_str!(t, self, b, /) }
    }

    #[inline]
    pub fn binary_op_rem(&self, b: &ValueData, t: ValueType) -> ValueData {
        unsafe { binary_op_int_no_wrap!(t, self, b, %) }
    }

    #[inline]
    pub fn binary_op_and(&self, b: &ValueData, t: ValueType) -> ValueData {
        unsafe { binary_op_int_no_wrap!(t, self, b, &) }
    }

    #[inline]
    pub fn binary_op_or(&self, b: &ValueData, t: ValueType) -> ValueData {
        unsafe { binary_op_int_no_wrap!(t, self, b, |) }
    }

    #[inline]
    pub fn binary_op_xor(&self, b: &ValueData, t: ValueType) -> ValueData {
        unsafe { binary_op_int_no_wrap!(t, self, b, ^) }
    }

    #[inline]
    pub fn binary_op_shl(&mut self, b: &u32, t: ValueType) {
        unsafe { shift_int!(t, self, b, checked_shl) }
    }

    #[inline]
    pub fn binary_op_shr(&mut self, b: &u32, t: ValueType) {
        unsafe { shift_int!(t, self, b, checked_shr) }
    }

    #[inline]
    pub fn binary_op_and_not(&self, b: &ValueData, t: ValueType) -> ValueData {
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
    pub fn compare_eql(a: &ValueData, b: &ValueData, t: ValueType) -> bool {
        unsafe { cmp_bool_int_float!(t, a, b, ==) }
    }

    #[inline]
    pub fn compare_neq(a: &ValueData, b: &ValueData, t: ValueType) -> bool {
        unsafe { cmp_bool_int_float!(t, a, b, !=) }
    }

    #[inline]
    pub fn compare_lss(a: &ValueData, b: &ValueData, t: ValueType) -> bool {
        unsafe { cmp_int_float!(t, a, b, <) }
    }

    #[inline]
    pub fn compare_gtr(a: &ValueData, b: &ValueData, t: ValueType) -> bool {
        unsafe { cmp_int_float!(t, a, b, >) }
    }

    #[inline]
    pub fn compare_leq(a: &ValueData, b: &ValueData, t: ValueType) -> bool {
        unsafe { cmp_int_float!(t, a, b, <=) }
    }

    #[inline]
    pub fn compare_geq(a: &ValueData, b: &ValueData, t: ValueType) -> bool {
        unsafe { cmp_int_float!(t, a, b, >=) }
    }

    #[inline]
    pub fn rc(&self, t: ValueType) -> Option<&Cell<IRC>> {
        match t {
            ValueType::Array => Some(&self.as_array().1),
            ValueType::Closure => self.as_closure().map(|x| &x.1),
            ValueType::Slice => self.as_slice().map(|x| &x.1),
            ValueType::Map => self.as_map().map(|x| &x.1),
            ValueType::Struct => Some(&self.as_struct().1),
            _ => unreachable!(),
        }
    }

    pub fn fmt_debug(&self, t: ValueType, f: &mut fmt::Formatter) -> fmt::Result {
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
            ValueType::Str => write!(f, "Type: {:?}, Data: {:#?}", t, self.as_str()),
            ValueType::Array => write!(f, "Type: {:?}, Data: {:#?}", t, self.as_array()),
            ValueType::Struct => write!(f, "Type: {:?}, Data: {:#?}", t, self.as_struct()),
            ValueType::Pointer => write!(f, "Type: {:?}, Data: {:#?}", t, self.as_pointer()),
            ValueType::UnsafePtr => write!(f, "Type: {:?}, Data: {:#?}", t, self.as_unsafe_ptr()),
            ValueType::Closure => write!(f, "Type: {:?}, Data: {:#?}", t, self.as_closure()),
            ValueType::Slice => write!(f, "Type: {:?}, Data: {:#?}", t, self.as_slice()),
            ValueType::Map => write!(f, "Type: {:?}, Data: {:#?}", t, self.as_map()),
            ValueType::Interface => write!(f, "Type: {:?}, Data: {:#?}", t, self.as_interface()),
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
    pub unsafe fn copy_non_ptr(&self) -> ValueData {
        self.copy()
    }

    #[inline]
    fn new_metadata(m: Meta) -> ValueData {
        ValueData {
            metadata: Box::into_raw(Box::new(m)),
        }
    }

    #[inline]
    fn new_complex128(r: F64, i: F64) -> ValueData {
        ValueData {
            complex128: Box::into_raw(Box::new((r, i))),
        }
    }

    #[inline]
    fn new_str(s: String) -> ValueData {
        ValueData {
            str: Rc::into_raw(Rc::new(StringObj::with_str(s))),
        }
    }

    #[inline]
    fn new_array(obj: ArrayObj, gcv: &GcoVec) -> ValueData {
        let arr = Rc::new((obj, Cell::new(0)));
        gcv.add_array(&arr);
        ValueData {
            array: Rc::into_raw(arr),
        }
    }

    #[inline]
    fn new_struct(obj: StructObj, gcv: &GcoVec) -> ValueData {
        let s = Rc::new((RefCell::new(obj), Cell::new(0)));
        gcv.add_struct(&s);
        ValueData {
            structure: Rc::into_raw(s),
        }
    }

    #[inline]
    fn new_pointer(obj: PointerObj) -> ValueData {
        ValueData {
            pointer: Box::into_raw(Box::new(obj)),
        }
    }

    #[inline]
    fn new_unsafe_ptr<T: 'static + UnsafePtr>(p: T) -> ValueData {
        ValueData {
            unsafe_ptr: Box::into_raw(Box::new(Rc::new(p))),
        }
    }

    #[inline]
    fn new_closure(obj: ClosureObj, gcv: &GcoVec) -> ValueData {
        let cls = Rc::new((RefCell::new(obj), Cell::new(0)));
        gcv.add_closure(&cls);
        ValueData {
            closure: Rc::into_raw(cls),
        }
    }

    #[inline]
    fn new_closure_static(fkey: FunctionKey, fobjs: &FunctionObjs) -> ValueData {
        let obj = ClosureObj::new_gos(fkey, fobjs, None);
        let cls = Rc::into_raw(Rc::new((RefCell::new(obj), Cell::new(0))));
        ValueData { closure: cls }
    }

    #[inline]
    fn new_slice(obj: SliceObj, gcv: &GcoVec) -> ValueData {
        let s = Rc::new((obj, Cell::new(0)));
        gcv.add_slice(&s);
        ValueData {
            slice: Rc::into_raw(s),
        }
    }

    #[inline]
    fn new_map(obj: MapObj, gcv: &GcoVec) -> ValueData {
        let m = Rc::new((obj, Cell::new(0)));
        gcv.add_map(&m);
        ValueData {
            map: Rc::into_raw(m),
        }
    }

    #[inline]
    fn new_interface(obj: InterfaceObj) -> ValueData {
        let iface = Rc::into_raw(Rc::new(RefCell::new(obj)));
        ValueData { interface: iface }
    }

    #[inline]
    fn new_channel(obj: ChannelObj) -> ValueData {
        ValueData {
            channel: Rc::into_raw(Rc::new(obj)),
        }
    }

    #[inline]
    fn from_metadata(m: Box<Meta>) -> ValueData {
        ValueData {
            metadata: Box::into_raw(m),
        }
    }

    #[inline]
    fn from_complex128(c: Box<(F64, F64)>) -> ValueData {
        ValueData {
            complex128: Box::into_raw(c),
        }
    }

    #[inline]
    fn from_str(s: Rc<StringObj>) -> ValueData {
        ValueData {
            str: Rc::into_raw(s),
        }
    }

    #[inline]
    fn from_array(arr: Rc<(ArrayObj, RCount)>) -> ValueData {
        ValueData {
            array: Rc::into_raw(arr),
        }
    }

    #[inline]
    fn from_struct(s: Rc<(RefCell<StructObj>, RCount)>) -> ValueData {
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
    fn from_unsafe_ptr(p: OptionBox<Rc<dyn UnsafePtr>>) -> ValueData {
        ValueData {
            unsafe_ptr: p.map_or(ptr::null_mut(), |x| Box::into_raw(x)),
        }
    }

    #[inline]
    fn from_closure(cls: OptionRc<(RefCell<ClosureObj>, RCount)>) -> ValueData {
        ValueData {
            closure: cls.map_or(ptr::null(), |x| Rc::into_raw(x)),
        }
    }

    #[inline]
    fn from_slice(s: OptionRc<(SliceObj, RCount)>) -> ValueData {
        ValueData {
            slice: s.map_or(ptr::null(), |x| Rc::into_raw(x)),
        }
    }

    #[inline]
    fn from_map(m: OptionRc<(MapObj, RCount)>) -> ValueData {
        ValueData {
            map: m.map_or(ptr::null(), |x| Rc::into_raw(x)),
        }
    }

    #[inline]
    fn from_interface(i: OptionRc<RefCell<InterfaceObj>>) -> ValueData {
        ValueData {
            interface: i.map_or(ptr::null(), |x| Rc::into_raw(x)),
        }
    }

    #[inline]
    fn from_channel(c: OptionRc<ChannelObj>) -> ValueData {
        ValueData {
            channel: c.map_or(ptr::null(), |x| Rc::into_raw(x)),
        }
    }

    #[inline]
    fn into_metadata(self) -> Box<Meta> {
        unsafe { Box::from_raw(self.metadata) }
    }

    #[inline]
    fn into_complex128(self) -> Box<(F64, F64)> {
        unsafe { Box::from_raw(self.complex128) }
    }

    #[inline]
    fn into_str(self) -> Rc<StringObj> {
        unsafe { Rc::from_raw(self.str) }
    }

    #[inline]
    fn into_array(self) -> Rc<(ArrayObj, RCount)> {
        unsafe { Rc::from_raw(self.array) }
    }

    #[inline]
    fn into_struct(self) -> Rc<(RefCell<StructObj>, RCount)> {
        unsafe { Rc::from_raw(self.structure) }
    }

    #[inline]
    fn into_pointer(self) -> OptionBox<PointerObj> {
        unsafe { (!self.pointer.is_null()).then(|| Box::from_raw(self.pointer)) }
    }

    #[inline]
    fn into_unsafe_ptr(self) -> OptionBox<Rc<dyn UnsafePtr>> {
        unsafe { (!self.unsafe_ptr.is_null()).then(|| Box::from_raw(self.unsafe_ptr)) }
    }

    #[inline]
    fn into_closure(self) -> OptionRc<(RefCell<ClosureObj>, RCount)> {
        unsafe { (!self.closure.is_null()).then(|| Rc::from_raw(self.closure)) }
    }

    #[inline]
    fn into_slice(self) -> OptionRc<(SliceObj, RCount)> {
        unsafe { (!self.slice.is_null()).then(|| Rc::from_raw(self.slice)) }
    }

    #[inline]
    fn into_map(self) -> OptionRc<(MapObj, RCount)> {
        unsafe { (!self.map.is_null()).then(|| Rc::from_raw(self.map)) }
    }

    #[inline]
    fn into_interface(self) -> OptionRc<RefCell<InterfaceObj>> {
        unsafe { (!self.interface.is_null()).then(|| Rc::from_raw(self.interface)) }
    }

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
            ValueType::Str => unsafe {
                Rc::increment_strong_count(self.str);
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
    fn copy_semantic(&self, t: ValueType, gcv: &GcoVec) -> ValueData {
        match t {
            ValueType::Metadata => ValueData::from_metadata(Box::new(self.as_metadata().clone())),
            ValueType::Complex128 => {
                ValueData::from_complex128(Box::new(self.as_complex128().clone()))
            }
            ValueType::Str => unsafe {
                Rc::increment_strong_count(self.str);
                self.copy()
            },
            ValueType::Array => ValueData::new_array(self.as_array().0.clone(), gcv),
            ValueType::Struct => {
                ValueData::new_struct(StructObj::clone(&self.as_struct().0.borrow()), gcv)
            }
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
            ValueType::Slice => match self.as_slice() {
                Some(s) => ValueData::new_slice(s.0.clone(), gcv),
                None => ValueData::new_nil(t),
            },
            ValueType::Map => match self.as_map() {
                Some(m) => ValueData::new_map(m.0.clone(), gcv),
                None => ValueData::new_nil(t),
            },
            ValueType::Interface => unsafe {
                if !self.interface.is_null() {
                    Rc::increment_strong_count(self.interface);
                }
                self.copy()
            },
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
    fn copy(&self) -> ValueData {
        unsafe { std::mem::transmute_copy(self) }
    }

    #[inline]
    fn drop_as_ptr(&self, t: ValueType) {
        match t {
            ValueType::Metadata => {
                self.copy().into_metadata();
            }
            ValueType::Complex128 => {
                self.copy().into_complex128();
            }
            ValueType::Str => {
                self.copy().into_str();
            }
            ValueType::Array => {
                self.copy().into_array();
            }
            ValueType::Pointer => {
                self.copy().into_pointer();
            }
            ValueType::UnsafePtr => {
                self.copy().into_unsafe_ptr();
            }
            ValueType::Closure => {
                self.copy().into_closure();
            }
            ValueType::Slice => {
                self.copy().into_slice();
            }
            ValueType::Map => {
                self.copy().into_map();
            }
            ValueType::Interface => {
                self.copy().into_interface();
            }
            ValueType::Struct => {
                self.copy().into_struct();
            }
            ValueType::Channel => {
                self.copy().into_channel();
            }
            _ => unreachable!(),
        }
    }
}

impl fmt::Debug for ValueData {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.fmt_debug(ValueType::Void, f)
    }
}

pub struct GosValue {
    typ: ValueType,
    data: ValueData,
}

impl GosValue {
    #[inline]
    pub fn typ(&self) -> ValueType {
        self.typ
    }

    /// Get a reference to the gos value's data.
    #[inline]
    pub fn data(&self) -> &ValueData {
        &self.data
    }

    /// Get a mutable reference to the gos value's data.
    #[inline]
    pub unsafe fn data_mut(&mut self) -> &mut ValueData {
        &mut self.data
    }

    #[inline]
    pub fn new_nil(t: ValueType) -> GosValue {
        GosValue::new(t, ValueData::new_nil(t))
    }

    #[inline]
    pub fn new_bool(b: bool) -> GosValue {
        GosValue::new(ValueType::Bool, ValueData::new_bool(b))
    }

    #[inline]
    pub fn new_int(i: isize) -> GosValue {
        GosValue::new(ValueType::Int, ValueData::new_int(i))
    }

    #[inline]
    pub fn new_int8(i: i8) -> GosValue {
        GosValue::new(ValueType::Int8, ValueData::new_int8(i))
    }

    #[inline]
    pub fn new_int16(i: i16) -> GosValue {
        GosValue::new(ValueType::Int16, ValueData::new_int16(i))
    }

    #[inline]
    pub fn new_int32(i: i32) -> GosValue {
        GosValue::new(ValueType::Int32, ValueData::new_int32(i))
    }

    #[inline]
    pub fn new_int64(i: i64) -> GosValue {
        GosValue::new(ValueType::Int64, ValueData::new_int64(i))
    }

    #[inline]
    pub fn new_uint(u: usize) -> GosValue {
        GosValue::new(ValueType::Uint, ValueData::new_uint(u))
    }

    #[inline]
    pub fn new_uint_ptr(u: usize) -> GosValue {
        GosValue::new(ValueType::UintPtr, ValueData::new_uint_ptr(u))
    }

    #[inline]
    pub fn new_uint8(u: u8) -> GosValue {
        GosValue::new(ValueType::Uint8, ValueData::new_uint8(u))
    }

    #[inline]
    pub fn new_uint16(u: u16) -> GosValue {
        GosValue::new(ValueType::Uint16, ValueData::new_uint16(u))
    }

    #[inline]
    pub fn new_uint32(u: u32) -> GosValue {
        GosValue::new(ValueType::Uint32, ValueData::new_uint32(u))
    }

    #[inline]
    pub fn new_uint64(u: u64) -> GosValue {
        GosValue::new(ValueType::Uint64, ValueData::new_uint64(u))
    }

    #[inline]
    pub fn new_float32(f: F32) -> GosValue {
        GosValue::new(ValueType::Float32, ValueData::new_float32(f))
    }

    #[inline]
    pub fn new_float64(f: F64) -> GosValue {
        GosValue::new(ValueType::Float64, ValueData::new_float64(f))
    }

    #[inline]
    pub fn new_complex64(r: F32, i: F32) -> GosValue {
        GosValue::new(ValueType::Complex64, ValueData::new_complex64(r, i))
    }

    #[inline]
    pub fn new_function(f: FunctionKey) -> GosValue {
        GosValue::new(ValueType::Function, ValueData::new_function(f))
    }

    #[inline]
    pub fn new_package(p: PackageKey) -> GosValue {
        GosValue::new(ValueType::Package, ValueData::new_package(p))
    }

    #[inline]
    pub fn new_metadata(m: Meta) -> GosValue {
        GosValue::new(ValueType::Metadata, ValueData::new_metadata(m))
    }

    #[inline]
    pub fn new_complex128(r: F64, i: F64) -> GosValue {
        GosValue::new(ValueType::Complex128, ValueData::new_complex128(r, i))
    }

    #[inline]
    pub fn new_str(s: String) -> GosValue {
        GosValue::new(ValueType::Str, ValueData::new_str(s))
    }

    #[inline]
    pub fn new_array(obj: ArrayObj, gcv: &GcoVec) -> GosValue {
        GosValue::new(ValueType::Array, ValueData::new_array(obj, gcv))
    }

    #[inline]
    pub fn new_struct(obj: StructObj, gcv: &GcoVec) -> GosValue {
        GosValue::new(ValueType::Struct, ValueData::new_struct(obj, gcv))
    }

    #[inline]
    pub fn new_pointer(obj: PointerObj) -> GosValue {
        GosValue::new(ValueType::Pointer, ValueData::new_pointer(obj))
    }

    #[inline]
    pub fn new_unsafe_ptr<T: 'static + UnsafePtr>(p: T) -> GosValue {
        GosValue::new(ValueType::UnsafePtr, ValueData::new_unsafe_ptr(p))
    }

    #[inline]
    pub fn new_closure(obj: ClosureObj, gcv: &GcoVec) -> GosValue {
        GosValue::new(ValueType::Closure, ValueData::new_closure(obj, gcv))
    }

    #[inline]
    pub fn new_closure_static(fkey: FunctionKey, fobjs: &FunctionObjs) -> GosValue {
        GosValue::new(
            ValueType::Closure,
            ValueData::new_closure_static(fkey, fobjs),
        )
    }

    #[inline]
    pub fn new_slice(obj: SliceObj, gcv: &GcoVec) -> GosValue {
        GosValue::new(ValueType::Slice, ValueData::new_slice(obj, gcv))
    }

    #[inline]
    pub fn new_map(obj: MapObj, gcv: &GcoVec) -> GosValue {
        GosValue::new(ValueType::Map, ValueData::new_map(obj, gcv))
    }

    #[inline]
    pub fn new_interface(obj: InterfaceObj) -> GosValue {
        GosValue::new(ValueType::Interface, ValueData::new_interface(obj))
    }

    #[inline]
    pub fn new_channel(obj: ChannelObj) -> GosValue {
        GosValue::new(ValueType::Channel, ValueData::new_channel(obj))
    }

    #[inline]
    pub fn array_with_size(size: usize, val: &GosValue, gcv: &GcoVec) -> GosValue {
        GosValue::new_array(ArrayObj::with_size(size, val, gcv), gcv)
    }

    #[inline]
    pub fn array_with_data(data: Vec<GosValue>, gcv: &GcoVec) -> GosValue {
        GosValue::new_array(ArrayObj::with_data(data), gcv)
    }

    #[inline]
    pub fn slice_with_len(
        len: usize,
        cap: usize,
        dval: Option<&GosValue>,
        gcv: &GcoVec,
    ) -> GosValue {
        GosValue::new_slice(SliceObj::new(len, cap, dval), gcv)
    }

    #[inline]
    pub fn slice_with_data(data: Vec<GosValue>, gcv: &GcoVec) -> GosValue {
        GosValue::new_slice(SliceObj::with_data(data), gcv)
    }

    #[inline]
    pub fn slice_with_array(arr: &GosValue, begin: isize, end: isize, gcv: &GcoVec) -> GosValue {
        GosValue::new_slice(SliceObj::with_array(&arr.as_array().0, begin, end), gcv)
    }

    #[inline]
    pub fn map_with_default_val(default_val: GosValue, gcv: &GcoVec) -> GosValue {
        GosValue::new_map(MapObj::new(default_val), gcv)
    }

    #[inline]
    pub fn function_with_meta(
        package: PackageKey,
        meta: Meta,
        objs: &mut VMObjects,
        gcv: &GcoVec,
        flag: FuncFlag,
    ) -> GosValue {
        let val = FunctionVal::new(package, meta, &objs.metas, gcv, flag);
        GosValue::new_function(objs.functions.insert(val))
    }

    #[inline]
    pub fn empty_iface_with_val(val: GosValue) -> GosValue {
        GosValue::new_interface(InterfaceObj::Gos(val, None))
    }

    #[inline]
    pub fn channel_with_chan(chan: Channel, recv_zero: GosValue) -> GosValue {
        GosValue::new_channel(ChannelObj::with_chan(chan, recv_zero))
    }

    #[inline]
    pub fn from_str(s: Rc<StringObj>) -> GosValue {
        GosValue::new(ValueType::Str, ValueData::from_str(s))
    }

    #[inline]
    pub fn from_array(arr: Rc<(ArrayObj, RCount)>) -> GosValue {
        GosValue::new(ValueType::Array, ValueData::from_array(arr))
    }

    #[inline]
    pub fn from_struct(s: Rc<(RefCell<StructObj>, RCount)>) -> GosValue {
        GosValue::new(ValueType::Struct, ValueData::from_struct(s))
    }

    #[inline]
    pub fn from_closure(cls: OptionRc<(RefCell<ClosureObj>, RCount)>) -> GosValue {
        GosValue::new(ValueType::Closure, ValueData::from_closure(cls))
    }

    #[inline]
    pub fn from_slice(s: OptionRc<(SliceObj, RCount)>) -> GosValue {
        GosValue::new(ValueType::Slice, ValueData::from_slice(s))
    }

    #[inline]
    pub fn from_map(m: OptionRc<(MapObj, RCount)>) -> GosValue {
        GosValue::new(ValueType::Map, ValueData::from_map(m))
    }

    #[inline]
    pub fn from_interface(i: OptionRc<RefCell<InterfaceObj>>) -> GosValue {
        GosValue::new(ValueType::Interface, ValueData::from_interface(i))
    }

    #[inline]
    pub fn from_channel(c: OptionRc<ChannelObj>) -> GosValue {
        GosValue::new(ValueType::Channel, ValueData::from_channel(c))
    }

    #[inline]
    pub fn into_metadata(mut self) -> Box<Meta> {
        debug_assert!(self.typ == ValueType::Metadata);
        self.typ = ValueType::Void;
        self.data.copy().into_metadata()
    }

    #[inline]
    pub fn into_complex128(mut self) -> Box<(F64, F64)> {
        debug_assert!(self.typ == ValueType::Complex128);
        self.typ = ValueType::Void;
        self.data.copy().into_complex128()
    }

    #[inline]
    pub fn into_str(mut self) -> Rc<StringObj> {
        debug_assert!(self.typ == ValueType::Str);
        self.typ = ValueType::Void;
        self.data.copy().into_str()
    }

    #[inline]
    pub fn into_array(mut self) -> Rc<(ArrayObj, RCount)> {
        debug_assert!(self.typ == ValueType::Array);
        self.typ = ValueType::Void;
        self.data.copy().into_array()
    }

    #[inline]
    pub fn into_struct(mut self) -> Rc<(RefCell<StructObj>, RCount)> {
        debug_assert!(self.typ == ValueType::Struct);
        self.typ = ValueType::Void;
        self.data.copy().into_struct()
    }

    #[inline]
    pub fn into_pointer(mut self) -> OptionBox<PointerObj> {
        debug_assert!(self.typ == ValueType::Pointer);
        self.typ = ValueType::Void;
        self.data.copy().into_pointer()
    }

    #[inline]
    pub fn into_unsafe_ptr(mut self) -> OptionBox<Rc<dyn UnsafePtr>> {
        debug_assert!(self.typ == ValueType::UnsafePtr);
        self.typ = ValueType::Void;
        self.data.copy().into_unsafe_ptr()
    }

    #[inline]
    pub fn into_closure(mut self) -> OptionRc<(RefCell<ClosureObj>, RCount)> {
        debug_assert!(self.typ == ValueType::Closure);
        self.typ = ValueType::Void;
        self.data.copy().into_closure()
    }

    #[inline]
    pub fn into_slice(mut self) -> OptionRc<(SliceObj, RCount)> {
        debug_assert!(self.typ == ValueType::Slice);
        self.typ = ValueType::Void;
        self.data.copy().into_slice()
    }

    #[inline]
    pub fn into_map(mut self) -> OptionRc<(MapObj, RCount)> {
        debug_assert!(self.typ == ValueType::Map);
        self.typ = ValueType::Void;
        self.data.copy().into_map()
    }

    #[inline]
    pub fn into_interface(mut self) -> OptionRc<RefCell<InterfaceObj>> {
        debug_assert!(self.typ == ValueType::Interface);
        self.typ = ValueType::Void;
        self.data.copy().into_interface()
    }

    #[inline]
    pub fn into_channel(mut self) -> OptionRc<ChannelObj> {
        debug_assert!(self.typ == ValueType::Channel);
        self.typ = ValueType::Void;
        self.data.copy().into_channel()
    }

    #[inline]
    pub fn into_some_pointer(self) -> RuntimeResult<Box<PointerObj>> {
        self.into_pointer().ok_or(nil_err_str!())
    }

    #[inline]
    pub fn into_some_unsafe_ptr(self) -> RuntimeResult<Box<Rc<dyn UnsafePtr>>> {
        self.into_unsafe_ptr().ok_or(nil_err_str!())
    }

    #[inline]
    pub fn into_some_closure(self) -> RuntimeResult<Rc<(RefCell<ClosureObj>, RCount)>> {
        self.into_closure().ok_or(nil_err_str!())
    }

    #[inline]
    pub fn into_some_slice(self) -> RuntimeResult<Rc<(SliceObj, RCount)>> {
        self.into_slice().ok_or(nil_err_str!())
    }

    #[inline]
    pub fn into_some_map(self) -> RuntimeResult<Rc<(MapObj, RCount)>> {
        self.into_map().ok_or(nil_err_str!())
    }

    #[inline]
    pub fn into_some_interface(self) -> RuntimeResult<Rc<RefCell<InterfaceObj>>> {
        self.into_interface().ok_or(nil_err_str!())
    }

    #[inline]
    pub fn into_some_channel(self) -> RuntimeResult<Rc<ChannelObj>> {
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
    pub fn as_complex64(&self) -> &(F32, F32) {
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
    pub fn as_complex128(&self) -> &(F64, F64) {
        debug_assert!(self.typ == ValueType::Complex128);
        self.data.as_complex128()
    }

    #[inline]
    pub fn as_str(&self) -> &StringObj {
        debug_assert!(self.typ == ValueType::Str);
        self.data.as_str()
    }

    #[inline]
    pub fn as_array(&self) -> &(ArrayObj, RCount) {
        debug_assert!(self.typ == ValueType::Array);
        self.data.as_array()
    }

    #[inline]
    pub fn as_struct(&self) -> &(RefCell<StructObj>, RCount) {
        debug_assert!(self.typ == ValueType::Struct);
        self.data.as_struct()
    }

    #[inline]
    pub fn as_pointer(&self) -> Option<&PointerObj> {
        debug_assert!(self.typ == ValueType::Pointer);
        self.data.as_pointer()
    }

    #[inline]
    pub fn as_unsafe_ptr(&self) -> Option<&Rc<dyn UnsafePtr>> {
        debug_assert!(self.typ == ValueType::UnsafePtr);
        self.data.as_unsafe_ptr()
    }

    #[inline]
    pub fn as_closure(&self) -> Option<&(RefCell<ClosureObj>, RCount)> {
        debug_assert!(self.typ == ValueType::Closure);
        self.data.as_closure()
    }

    #[inline]
    pub fn as_slice(&self) -> Option<&(SliceObj, RCount)> {
        debug_assert!(self.typ == ValueType::Slice);
        self.data.as_slice()
    }

    #[inline]
    pub fn as_map(&self) -> Option<&(MapObj, RCount)> {
        debug_assert!(self.typ == ValueType::Map);
        self.data.as_map()
    }

    #[inline]
    pub fn as_interface(&self) -> Option<&RefCell<InterfaceObj>> {
        debug_assert!(self.typ == ValueType::Interface);
        self.data.as_interface()
    }

    #[inline]
    pub fn as_channel(&self) -> Option<&ChannelObj> {
        debug_assert!(self.typ == ValueType::Channel);
        self.data.as_channel()
    }

    #[inline]
    pub fn as_some_pointer(&self) -> RuntimeResult<&PointerObj> {
        self.as_pointer().ok_or(nil_err_str!())
    }

    #[inline]
    pub fn as_some_unsafe_ptr(&self) -> RuntimeResult<&Rc<dyn UnsafePtr>> {
        self.as_unsafe_ptr().ok_or(nil_err_str!())
    }

    #[inline]
    pub fn as_some_closure(&self) -> RuntimeResult<&(RefCell<ClosureObj>, RCount)> {
        self.as_closure().ok_or(nil_err_str!())
    }

    #[inline]
    pub fn as_some_slice(&self) -> RuntimeResult<&(SliceObj, RCount)> {
        self.as_slice().ok_or(nil_err_str!())
    }

    #[inline]
    pub fn as_some_map(&self) -> RuntimeResult<&(MapObj, RCount)> {
        self.as_map().ok_or(nil_err_str!())
    }

    #[inline]
    pub fn as_some_interface(&self) -> RuntimeResult<&RefCell<InterfaceObj>> {
        self.as_interface().ok_or(nil_err_str!())
    }

    #[inline]
    pub fn as_some_channel(&self) -> RuntimeResult<&ChannelObj> {
        self.as_channel().ok_or(nil_err_str!())
    }

    #[inline]
    pub fn int32_as(i: i32, t: ValueType) -> GosValue {
        GosValue::new(t, ValueData::int32_as(i, t))
    }

    #[inline]
    pub fn is_nil(&self) -> bool {
        match self.typ {
            ValueType::Pointer => self.as_pointer().is_none(),
            ValueType::UnsafePtr => self.as_unsafe_ptr().is_none(),
            ValueType::Closure => self.as_closure().is_none(),
            ValueType::Slice => self.as_slice().is_none(),
            ValueType::Map => self.as_map().is_none(),
            ValueType::Interface => self.as_interface().is_none(),
            ValueType::Channel => self.as_channel().is_none(),
            ValueType::Void => true,
            _ => false,
        }
    }

    #[inline]
    pub fn copy_semantic(&self, gcv: &GcoVec) -> GosValue {
        if self.typ.copyable() {
            GosValue::new(self.typ, self.data.copy())
        } else {
            GosValue::new(self.typ, self.data.copy_semantic(self.typ, gcv))
        }
    }

    #[inline]
    pub fn try_as_struct(&self) -> Option<&(RefCell<StructObj>, RCount)> {
        match &self.typ {
            ValueType::Struct => Some(self.as_struct()),
            _ => None,
        }
    }

    #[inline]
    pub fn cast_copyable(&mut self, from: ValueType, to: ValueType) {
        assert!(from.copyable());
        self.data.cast_copyable(from, to)
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
        let iface = self.as_some_interface()?;
        Ok(iface.borrow().underlying_value().map(|x| x.clone()))
    }

    pub fn identical(&self, other: &GosValue) -> bool {
        self.typ() == other.typ() && self == other
    }

    #[inline]
    pub fn load_index(&self, ind: &GosValue, gcv: &GcoVec) -> RuntimeResult<GosValue> {
        match self.typ {
            ValueType::Map => Ok(self.as_some_map()?.0.get(&ind, gcv).0.clone()),
            ValueType::Slice => {
                let index = ind.as_index();
                self.as_some_slice()?
                    .0
                    .get(index)
                    .map_or_else(|| Err(format!("index {} out of range", index)), |x| Ok(x))
            }
            ValueType::Str => {
                let index = ind.as_index();
                self.as_str().get_byte(index).map_or_else(
                    || Err(format!("index {} out of range", index)),
                    |x| Ok(GosValue::new_int(*x as isize)),
                )
            }
            ValueType::Array => {
                let index = ind.as_index();
                self.as_array()
                    .0
                    .get(index)
                    .map_or_else(|| Err(format!("index {} out of range", index)), |x| Ok(x))
            }
            _ => unreachable!(),
        }
    }

    #[inline]
    pub fn load_index_int(&self, i: usize, gcv: &GcoVec) -> RuntimeResult<GosValue> {
        match self.typ {
            ValueType::Slice => self
                .as_some_slice()?
                .0
                .get(i)
                .map_or_else(|| Err(format!("index {} out of range", i)), |x| Ok(x)),
            ValueType::Map => {
                let ind = GosValue::new_int(i as isize);
                Ok(self.as_some_map()?.0.get(&ind, gcv).0.clone())
            }
            ValueType::Str => self.as_str().get_byte(i).map_or_else(
                || Err(format!("index {} out of range", i)),
                |x| Ok(GosValue::new_int((*x).into())),
            ),
            ValueType::Array => self
                .as_array()
                .0
                .get(i)
                .map_or_else(|| Err(format!("index {} out of range", i)), |x| Ok(x)),
            _ => {
                unreachable!();
            }
        }
    }

    #[inline]
    pub fn load_field(&self, ind: &GosValue, objs: &VMObjects) -> GosValue {
        match self.typ {
            ValueType::Struct => self.as_struct().0.borrow().fields[*ind.as_int() as usize].clone(),
            ValueType::Package => {
                let pkg = &objs.packages[*self.as_package()];
                pkg.member(*ind.as_int() as OpIndex).clone()
            }
            _ => unreachable!(),
        }
    }

    /// for gc
    pub fn ref_sub_one(&self) {
        match &self.typ {
            ValueType::Pointer => {
                // todo: use inspect
                self.as_pointer().map(|p| p.ref_sub_one());
            }
            ValueType::UnsafePtr => {
                self.as_unsafe_ptr().map(|p| p.ref_sub_one());
            }
            ValueType::Interface => {
                self.as_interface().map(|x| x.borrow().ref_sub_one());
            }
            ValueType::Array => self.as_array().1.set(self.as_array().1.get() - 1),
            ValueType::Struct => self.as_struct().1.set(self.as_struct().1.get() - 1),
            ValueType::Closure => {
                self.as_closure().map(|x| x.1.set(x.1.get() - 1));
            }
            ValueType::Slice => {
                self.as_slice().map(|x| x.1.set(x.1.get() - 1));
            }
            ValueType::Map => {
                self.as_map().map(|x| x.1.set(x.1.get() - 1));
            }
            _ => {}
        };
    }

    /// for gc
    pub fn mark_dirty(&self, queue: &mut RCQueue) {
        match &self.typ {
            ValueType::Array => rcount_mark_and_queue(&self.as_array().1, queue),
            ValueType::Pointer => {
                self.as_pointer().map(|x| x.mark_dirty(queue));
            }
            ValueType::UnsafePtr => {
                self.as_unsafe_ptr().map(|x| x.mark_dirty(queue));
            }
            ValueType::Closure => {
                self.as_closure()
                    .map(|x| rcount_mark_and_queue(&x.1, queue));
            }
            ValueType::Slice => {
                self.as_slice().map(|x| rcount_mark_and_queue(&x.1, queue));
            }
            ValueType::Map => {
                self.as_map().map(|x| rcount_mark_and_queue(&x.1, queue));
            }
            ValueType::Interface => {
                self.as_interface().map(|x| x.borrow().mark_dirty(queue));
            }
            ValueType::Struct => rcount_mark_and_queue(&self.as_struct().1, queue),
            _ => {}
        };
    }

    #[inline]
    pub fn rc(&self) -> IRC {
        self.data.rc(self.typ).unwrap().get()
    }

    #[inline]
    pub fn set_rc(&self, rc: IRC) {
        self.data.rc(self.typ).unwrap().set(rc)
    }

    #[inline]
    pub fn drop_as_copyable(self) {
        debug_assert!(self.typ.copyable());
        drop(self);
    }

    #[inline]
    fn new(typ: ValueType, data: ValueData) -> GosValue {
        GosValue {
            typ: typ,
            data: data,
        }
    }
}

impl Drop for GosValue {
    #[inline]
    fn drop(&mut self) {
        if !self.typ.copyable() {
            self.data.drop_as_ptr(self.typ);
        }
    }
}

impl Clone for GosValue {
    #[inline]
    fn clone(&self) -> Self {
        if self.typ.copyable() {
            GosValue::new(self.typ, self.data.copy())
        } else {
            GosValue::new(self.typ, self.data.clone(self.typ))
        }
    }
}

impl Eq for GosValue {}

impl PartialEq for GosValue {
    #[inline]
    fn eq(&self, b: &GosValue) -> bool {
        match (self.typ, b.typ) {
            _ if self.typ.copyable() => self.as_uint() == b.as_uint(), //todo: does this work ok with float?
            (ValueType::Metadata, ValueType::Metadata) => self.as_metadata() == b.as_metadata(),
            (ValueType::Complex128, ValueType::Complex128) => {
                let x = self.as_complex128();
                let y = b.as_complex128();
                x.0 == y.0 && x.1 == y.1
            }
            (ValueType::Str, ValueType::Str) => self.as_str() == b.as_str(),
            (ValueType::Array, ValueType::Array) => self.as_array().0 == b.as_array().0,
            (ValueType::Struct, ValueType::Struct) => {
                StructObj::eq(&self.as_struct().0.borrow(), &b.as_struct().0.borrow())
            }
            (ValueType::Pointer, ValueType::Pointer) => self.as_pointer() == b.as_pointer(),
            (ValueType::UnsafePtr, ValueType::UnsafePtr) => {
                rc_ptr_eq!(self.as_unsafe_ptr(), b.as_unsafe_ptr())
            }
            (ValueType::Closure, ValueType::Closure) => {
                ref_ptr_eq(self.as_closure(), b.as_closure())
            }
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
                .map_or(b.is_nil(), |x| x.borrow().equals_value(b)),
            (_, ValueType::Interface) => b
                .as_interface()
                .map_or(self.is_nil(), |x| x.borrow().equals_value(self)),
            _ => false,
        }
    }
}

impl PartialOrd for GosValue {
    #[inline]
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Hash for GosValue {
    fn hash<H: Hasher>(&self, state: &mut H) {
        match &self.typ {
            _ if self.typ.copyable() => self.as_uint().hash(state),
            ValueType::Str => self.as_str().as_str().hash(state),
            ValueType::Array => self.as_array().0.hash(state),
            ValueType::Complex128 => {
                let c = self.as_complex128();
                c.0.hash(state);
                c.1.hash(state);
            }
            ValueType::Struct => {
                self.as_struct().0.borrow().hash(state);
            }
            ValueType::Interface => match self.as_interface() {
                Some(iface) => iface.borrow().hash(state),
                None => 0.hash(state),
            },
            ValueType::Pointer => match self.as_pointer() {
                Some(p) => PointerObj::hash(&p, state),
                None => 0.hash(state),
            },
            ValueType::UnsafePtr => match self.as_unsafe_ptr() {
                Some(p) => Rc::as_ptr(&p).hash(state),
                None => 0.hash(state),
            },
            _ => unreachable!(),
        }
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
            (ValueType::Str, ValueType::Str) => self.as_str().cmp(b.as_str()),
            _ => {
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
                write!(f, "({}, {})", c.0, c.1)
            }
            ValueType::Function => f.write_str("<function>"),
            ValueType::Package => f.write_str("<package>"),
            ValueType::Metadata => f.write_str("<metadata>"),
            ValueType::Complex128 => {
                let c = self.as_complex128();
                write!(f, "({}, {})", c.0, c.1)
            }
            ValueType::Str => f.write_str(self.as_str().as_str()),
            ValueType::Array => write!(f, "{:#?}", self.as_array().0),
            ValueType::Struct => write!(f, "{}", self.as_struct().0.borrow()),
            ValueType::Pointer => match self.as_pointer() {
                Some(p) => p.fmt(f),
                None => f.write_str("<nil(pointer)>"),
            },
            ValueType::UnsafePtr => match self.as_unsafe_ptr() {
                Some(p) => write!(f, "{:p}", Rc::as_ptr(p)),
                None => f.write_str("<nil(unsafe pointer)>"),
            },
            ValueType::Closure => match self.as_closure() {
                Some(_) => f.write_str("<closure>"),
                None => f.write_str("<nil(closure)>"),
            },
            ValueType::Slice => match self.as_slice() {
                Some(s) => write!(f, "{}", s.0),
                None => f.write_str("<nil(slice)>"),
            },
            ValueType::Map => match self.as_map() {
                Some(m) => write!(f, "{}", m.0),
                None => f.write_str("<nil(map)>"),
            },
            ValueType::Interface => match self.as_interface() {
                Some(i) => write!(f, "{}", i.borrow()),
                None => f.write_str("<nil(interface)>"),
            },
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
        self.data.fmt_debug(self.typ, f)
    }
}

#[cfg(test)]
mod test {
    use super::super::value::*;
    use std::collections::HashMap;
    use std::mem;

    #[test]
    fn test_size() {
        dbg!(mem::size_of::<HashMap<GosValue, GosValue>>());
        dbg!(mem::size_of::<String>());
        dbg!(mem::size_of::<Rc<String>>());
        dbg!(mem::size_of::<Rc<dyn UnsafePtr>>());
        dbg!(mem::size_of::<Box<Rc<dyn UnsafePtr>>>());
        dbg!(mem::size_of::<SliceObj>());
        dbg!(mem::size_of::<RefCell<GosValue>>());
        dbg!(mem::size_of::<GosValue>());
        dbg!(mem::size_of::<ValueData>());
        dbg!(mem::size_of::<Meta>());
        dbg!(mem::size_of::<Box<Meta>>());
        dbg!(mem::size_of::<OptionBox<Meta>>());

        dbg!(mem::size_of::<Option<bool>>());
        dbg!(mem::size_of::<ValueData>());

        let s = GosValue::new_str("aaa".to_owned());
        dbg!(s.data());
        let s2 = s.clone().into_str();
        dbg!(s2);
    }
}
