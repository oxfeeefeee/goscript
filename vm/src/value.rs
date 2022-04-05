use super::gc::{GcWeak, GcoVec};
use super::instruction::{OpIndex, ValueType};
use super::metadata::*;
pub use super::objects::*;
use super::stack::Stack;
use crate::channel::Channel;
use ordered_float;
use std::cell::{Cell, RefCell};
use std::cmp::Ordering;
use std::collections::VecDeque;
use std::fmt::{self, Display};
use std::hash::{Hash, Hasher};
use std::num::Wrapping;
use std::rc::Rc;
use std::result;

type F32 = ordered_float::OrderedFloat<f32>;
type F64 = ordered_float::OrderedFloat<f64>;
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

macro_rules! unwrap_gos_val {
    ($name:tt, $self_:ident) => {
        if let GosValue::$name(k) = $self_ {
            k
        } else {
            unreachable!();
        }
    };
}

macro_rules! union_op_wrap {
    ($a:ident, $b:ident, $name:tt, $op:tt) => {
        Value64{
            data: V64Union {
            $name: (Wrapping($a.data.$name) $op Wrapping($b.data.$name)).0,
        }}
    };
}

macro_rules! union_op {
    ($a:ident, $b:ident, $name:tt, $op:tt) => {
        Value64{
            data: V64Union {
            $name: $a.data.$name $op $b.data.$name,
        }}
    };
}

macro_rules! union_shift {
    ($a:ident, $b:ident, $name:tt, $op:tt) => {
        Value64 {
            data: V64Union {
                $name: $a.data.$name.$op(*$b).unwrap_or(0),
            },
        }
    };
}

macro_rules! union_cmp {
    ($a:ident, $b:ident, $name:tt, $op:tt) => {
        $a.data.$name $op $b.data.$name
    };
}

macro_rules! binary_op_int_float {
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
            ValueType::Bool => union_cmp!($a, $b, ubool, $op),
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
                ValueType::Uint => $val.data.$d_type = $val.data.uint as $typ,
                ValueType::UintPtr => $val.data.$d_type = $val.data.uint_ptr as $typ,
                ValueType::Uint8 => $val.data.$d_type = $val.data.uint8 as $typ,
                ValueType::Uint16 => $val.data.$d_type = $val.data.uint16 as $typ,
                ValueType::Uint32 => $val.data.$d_type = $val.data.uint32 as $typ,
                ValueType::Uint64 => $val.data.$d_type = $val.data.uint64 as $typ,
                ValueType::Int => $val.data.$d_type = $val.data.int as $typ,
                ValueType::Int8 => $val.data.$d_type = $val.data.int8 as $typ,
                ValueType::Int16 => $val.data.$d_type = $val.data.int16 as $typ,
                ValueType::Int32 => $val.data.$d_type = $val.data.int32 as $typ,
                ValueType::Int64 => $val.data.$d_type = $val.data.int64 as $typ,
                ValueType::Float32 => $val.data.$d_type = f32::from($val.data.float32) as $typ,
                ValueType::Float64 => $val.data.$d_type = f64::from($val.data.float64) as $typ,
                _ => unreachable!(),
            }
        }
    }};
}

macro_rules! convert_to_float {
    ($val:expr, $vt:expr, $d_type:tt, $f_type:tt, $typ:tt) => {{
        unsafe {
            match $vt {
                ValueType::Uint => $val.data.$d_type = $f_type::from($val.data.uint as $typ),
                ValueType::UintPtr => $val.data.$d_type = $f_type::from($val.data.uint_ptr as $typ),
                ValueType::Uint8 => $val.data.$d_type = $f_type::from($val.data.uint8 as $typ),
                ValueType::Uint16 => $val.data.$d_type = $f_type::from($val.data.uint16 as $typ),
                ValueType::Uint32 => $val.data.$d_type = $f_type::from($val.data.uint32 as $typ),
                ValueType::Uint64 => $val.data.$d_type = $f_type::from($val.data.uint64 as $typ),
                ValueType::Int => $val.data.$d_type = $f_type::from($val.data.int as $typ),
                ValueType::Int8 => $val.data.$d_type = $f_type::from($val.data.int8 as $typ),
                ValueType::Int16 => $val.data.$d_type = $f_type::from($val.data.int16 as $typ),
                ValueType::Int32 => $val.data.$d_type = $f_type::from($val.data.int32 as $typ),
                ValueType::Int64 => $val.data.$d_type = $f_type::from($val.data.int64 as $typ),
                ValueType::Float32 => {
                    $val.data.$d_type = $f_type::from(f32::from($val.data.float32) as $typ)
                }
                ValueType::Float64 => {
                    $val.data.$d_type = $f_type::from(f64::from($val.data.float64) as $typ)
                }
                _ => unreachable!(),
            }
        }
    }};
}

pub type RuntimeResult<T> = result::Result<T, String>;

// ----------------------------------------------------------------------------
// GosValue

#[derive(Debug)]
#[repr(C, u8)]
pub enum GosValue {
    Bool(Value64),
    Int(Value64),
    Int8(Value64),
    Int16(Value64),
    Int32(Value64),
    Int64(Value64),
    Uint(Value64),
    UintPtr(Value64),
    Uint8(Value64),
    Uint16(Value64),
    Uint32(Value64),
    Uint64(Value64),
    Float32(Value64),
    Float64(Value64),
    Complex64(Value64),
    Function(Value64), // not visible to users
    Package(Value64),  // not visible to users

    Metadata(Meta), // not visible to users
    Nil(Option<Meta>),
    Complex128(Box<(F64, F64)>),
    Str(Rc<StringObj>), // "String" is taken
    Array(Rc<(ArrayObj, RCount)>),
    Pointer(Box<PointerObj>),
    UnsafePtr(Rc<dyn UnsafePtr>),
    Closure(Rc<(RefCell<ClosureObj>, RCount)>),
    Slice(Rc<(SliceObj, Meta, RCount)>),
    Map(Rc<(MapObj, RCount)>),
    Interface(Rc<RefCell<InterfaceObj>>),
    Struct(Rc<(RefCell<StructObj>, RCount)>),
    Channel(Rc<ChannelObj>),

    Named(Box<(GosValue, Meta)>),
}

impl GosValue {
    #[inline]
    pub fn new_nil() -> GosValue {
        GosValue::Nil(None)
    }

    #[inline]
    pub fn new_bool(b: bool) -> GosValue {
        GosValue::Bool(Value64::from_bool(b))
    }

    #[inline]
    pub fn new_int(i: isize) -> GosValue {
        GosValue::Int(Value64::from_int(i))
    }

    #[inline]
    pub fn new_int8(i: i8) -> GosValue {
        GosValue::Int8(Value64::from_int8(i))
    }

    #[inline]
    pub fn new_int16(i: i16) -> GosValue {
        GosValue::Int16(Value64::from_int16(i))
    }

    #[inline]
    pub fn new_int32(i: i32) -> GosValue {
        GosValue::Int32(Value64::from_int32(i))
    }

    #[inline]
    pub fn new_int64(i: i64) -> GosValue {
        GosValue::Int64(Value64::from_int64(i))
    }

    #[inline]
    pub fn new_uint(u: usize) -> GosValue {
        GosValue::Uint(Value64::from_uint(u))
    }

    #[inline]
    pub fn new_uint_ptr(u: usize) -> GosValue {
        GosValue::UintPtr(Value64::from_uint_ptr(u))
    }

    #[inline]
    pub fn new_uint8(u: u8) -> GosValue {
        GosValue::Uint8(Value64::from_uint8(u))
    }

    #[inline]
    pub fn new_uint16(u: u16) -> GosValue {
        GosValue::Uint16(Value64::from_uint16(u))
    }

    #[inline]
    pub fn new_uint32(u: u32) -> GosValue {
        GosValue::Uint32(Value64::from_uint32(u))
    }

    #[inline]
    pub fn new_uint64(u: u64) -> GosValue {
        GosValue::Uint64(Value64::from_uint64(u))
    }

    #[inline]
    pub fn new_float32(f: F32) -> GosValue {
        GosValue::Float32(Value64::from_float32(f))
    }

    #[inline]
    pub fn new_float64(f: F64) -> GosValue {
        GosValue::Float64(Value64::from_float64(f))
    }

    #[inline]
    pub fn new_complex64(r: F32, i: F32) -> GosValue {
        GosValue::Float64(Value64::from_complex64(r, i))
    }

    #[inline]
    pub fn new_function(f: FunctionKey) -> GosValue {
        GosValue::Function(Value64::from_function(f))
    }

    #[inline]
    pub fn new_str(s: String) -> GosValue {
        GosValue::Str(Rc::new(StringObj::with_str(s)))
    }

    #[inline]
    pub fn new_pointer(v: PointerObj) -> GosValue {
        GosValue::Pointer(Box::new(v))
    }

    pub fn int32_as(i: i32, t: ValueType) -> GosValue {
        match t {
            ValueType::Int => GosValue::Int(Value64 {
                data: V64Union { int: i as isize },
            }),
            ValueType::Int8 => GosValue::Int8(Value64 {
                data: V64Union { int8: i as i8 },
            }),
            ValueType::Int16 => GosValue::Int16(Value64 {
                data: V64Union { int16: i as i16 },
            }),
            ValueType::Int32 => GosValue::Int32(Value64 {
                data: V64Union { int32: i as i32 },
            }),
            ValueType::Int64 => GosValue::Int64(Value64 {
                data: V64Union { int64: i as i64 },
            }),
            ValueType::Uint => GosValue::Uint(Value64 {
                data: V64Union { uint: i as usize },
            }),
            ValueType::UintPtr => GosValue::UintPtr(Value64 {
                data: V64Union {
                    uint_ptr: i as usize,
                },
            }),
            ValueType::Uint8 => GosValue::Uint8(Value64 {
                data: V64Union { uint8: i as u8 },
            }),
            ValueType::Uint16 => GosValue::Uint16(Value64 {
                data: V64Union { uint16: i as u16 },
            }),
            ValueType::Uint32 => GosValue::Uint32(Value64 {
                data: V64Union { uint32: i as u32 },
            }),
            ValueType::Uint64 => GosValue::Uint64(Value64 {
                data: V64Union { uint64: i as u64 },
            }),
            ValueType::Float32 => GosValue::Float32(Value64 {
                data: V64Union {
                    float32: F32::from(i as f32),
                },
            }),
            ValueType::Float64 => GosValue::Float64(Value64 {
                data: V64Union {
                    float64: F64::from(i as f64),
                },
            }),
            _ => unreachable!(),
        }
    }

    #[inline]
    pub fn array_with_size(size: usize, val: &GosValue, meta: Meta, gcobjs: &GcoVec) -> GosValue {
        let arr = Rc::new((ArrayObj::with_size(size, val, meta, gcobjs), Cell::new(0)));
        let v = GosValue::Array(arr);
        gcobjs.add(&v);
        v
    }

    #[inline]
    pub fn array_with_val(val: Vec<GosValue>, meta: Meta, gcobjs: &GcoVec) -> GosValue {
        let arr = Rc::new((ArrayObj::with_data(val, meta), Cell::new(0)));
        let v = GosValue::Array(arr);
        gcobjs.add(&v);
        v
    }

    #[inline]
    pub fn new_slice(
        len: usize,
        cap: usize,
        meta: Meta,
        dval: Option<&GosValue>,
        gcobjs: &GcoVec,
    ) -> GosValue {
        let s = Rc::new((SliceObj::new(len, cap, dval), meta, Cell::new(0)));
        let v = GosValue::Slice(s);
        gcobjs.add(&v);
        v
    }

    #[inline]
    pub fn slice_with_obj(obj: SliceObj, meta: Meta, gcobjs: &GcoVec) -> GosValue {
        let s = Rc::new((obj, meta, Cell::new(0)));
        let v = GosValue::Slice(s);
        gcobjs.add(&v);
        v
    }

    #[inline]
    pub fn slice_with_val(val: Vec<GosValue>, meta: Meta, gcobjs: &GcoVec) -> GosValue {
        let s = Rc::new((SliceObj::with_data(val), meta, Cell::new(0)));
        let v = GosValue::Slice(s);
        gcobjs.add(&v);
        v
    }

    #[inline]
    pub fn slice_with_array(
        arr: &GosValue,
        begin: isize,
        end: isize,
        meta: Meta,
        gcobjs: &GcoVec,
    ) -> GosValue {
        let s = Rc::new((
            SliceObj::with_array(&arr.as_array().0, begin, end),
            meta,
            Cell::new(0),
        ));
        let v = GosValue::Slice(s);
        gcobjs.add(&v);
        v
    }

    #[inline]
    pub fn new_map(meta: Meta, default_val: GosValue, gcobjs: &GcoVec) -> GosValue {
        let val = Rc::new((MapObj::new(meta, default_val), Cell::new(0)));
        let v = GosValue::Map(val);
        gcobjs.add(&v);
        v
    }

    #[inline]
    pub fn new_struct(obj: StructObj, gcobjs: &GcoVec) -> GosValue {
        let val = Rc::new((RefCell::new(obj), Cell::new(0)));
        let v = GosValue::Struct(val);
        gcobjs.add(&v);
        v
    }

    #[inline]
    pub fn new_function_create(
        package: PackageKey,
        meta: Meta,
        objs: &mut VMObjects,
        gcv: &GcoVec,
        flag: FuncFlag,
    ) -> GosValue {
        let val = FunctionVal::new(package, meta, objs, gcv, flag);
        GosValue::new_function(objs.functions.insert(val))
    }

    #[inline]
    pub fn new_static_closure(fkey: FunctionKey, fobjs: &FunctionObjs) -> GosValue {
        let val = ClosureObj::new_gos(fkey, fobjs, None);
        GosValue::Closure(Rc::new((RefCell::new(val), Cell::new(0))))
    }

    #[inline]
    pub fn new_closure(clsobj: ClosureObj, gcobjs: &GcoVec) -> GosValue {
        let v = GosValue::Closure(Rc::new((RefCell::new(clsobj), Cell::new(0))));
        gcobjs.add(&v);
        v
    }

    #[inline]
    pub fn new_iface(meta: Meta, underlying: IfaceUnderlying) -> GosValue {
        let val = Rc::new(RefCell::new(InterfaceObj::new(meta, underlying)));
        GosValue::Interface(val)
    }

    #[inline]
    pub fn new_empty_iface(mdata: &StaticMeta, underlying: GosValue) -> GosValue {
        let val = Rc::new(RefCell::new(InterfaceObj::new(
            mdata.empty_iface,
            IfaceUnderlying::Gos(underlying, None),
        )));
        GosValue::Interface(val)
    }

    #[inline]
    pub fn new_channel(meta: Meta, cap: usize) -> GosValue {
        GosValue::Channel(Rc::new(ChannelObj::new(meta, cap)))
    }

    #[inline]
    pub fn channel_with_chan(meta: Meta, chan: Channel) -> GosValue {
        GosValue::Channel(Rc::new(ChannelObj::with_chan(meta, chan)))
    }

    #[inline]
    pub fn new_meta(t: MetadataType, metas: &mut MetadataObjs) -> GosValue {
        GosValue::Metadata(Meta::with_type(t, metas))
    }

    #[inline]
    pub fn as_bool(&self) -> &bool {
        unwrap_gos_val!(Bool, self).as_bool()
    }

    #[inline]
    pub fn as_int(&self) -> &isize {
        unwrap_gos_val!(Int, self).as_int()
    }

    #[inline]
    pub fn as_uint8(&self) -> &u8 {
        unwrap_gos_val!(Uint8, self).as_uint8()
    }

    #[inline]
    pub fn as_uint32(&self) -> &u32 {
        unwrap_gos_val!(Uint32, self).as_uint32()
    }

    #[inline]
    pub fn as_uint64(&self) -> &u64 {
        unwrap_gos_val!(Uint64, self).as_uint64()
    }

    #[inline]
    pub fn as_int32(&self) -> &i32 {
        unwrap_gos_val!(Int32, self).as_int32()
    }

    #[inline]
    pub fn as_int64(&self) -> &i64 {
        unwrap_gos_val!(Int64, self).as_int64()
    }

    #[inline]
    pub fn as_int_mut(&mut self) -> &mut isize {
        unwrap_gos_val!(Int, self).as_int_mut()
    }

    #[inline]
    pub fn as_float32(&self) -> &f32 {
        unwrap_gos_val!(Float32, self).as_float32()
    }

    #[inline]
    pub fn as_float64(&self) -> &f64 {
        unwrap_gos_val!(Float64, self).as_float64()
    }

    #[inline]
    pub fn as_complex128(&self) -> &Box<(F64, F64)> {
        unwrap_gos_val!(Complex128, self)
    }

    #[inline]
    pub fn as_str(&self) -> &Rc<StringObj> {
        unwrap_gos_val!(Str, self)
    }

    #[inline]
    pub fn as_array(&self) -> &Rc<(ArrayObj, RCount)> {
        unwrap_gos_val!(Array, self)
    }

    #[inline]
    pub fn as_slice(&self) -> &Rc<(SliceObj, Meta, RCount)> {
        unwrap_gos_val!(Slice, self)
    }

    #[inline]
    pub fn as_map(&self) -> &Rc<(MapObj, RCount)> {
        unwrap_gos_val!(Map, self)
    }

    #[inline]
    pub fn as_interface(&self) -> &Rc<RefCell<InterfaceObj>> {
        unwrap_gos_val!(Interface, self)
    }

    #[inline]
    pub fn as_channel(&self) -> &Rc<ChannelObj> {
        unwrap_gos_val!(Channel, self)
    }

    #[inline]
    pub fn as_function(&self) -> &FunctionKey {
        unwrap_gos_val!(Function, self).as_function()
    }

    #[inline]
    pub fn as_package(&self) -> &PackageKey {
        unwrap_gos_val!(Package, self).as_package()
    }

    #[inline]
    pub fn as_struct(&self) -> &Rc<(RefCell<StructObj>, RCount)> {
        unwrap_gos_val!(Struct, self)
    }

    #[inline]
    pub fn as_closure(&self) -> &Rc<(RefCell<ClosureObj>, RCount)> {
        unwrap_gos_val!(Closure, self)
    }

    #[inline]
    pub fn as_meta(&self) -> &Meta {
        unwrap_gos_val!(Metadata, self)
    }

    #[inline]
    pub fn as_pointer(&self) -> &Box<PointerObj> {
        unwrap_gos_val!(Pointer, self)
    }

    #[inline]
    pub fn as_unsafe_ptr(&self) -> &Rc<dyn UnsafePtr> {
        unwrap_gos_val!(UnsafePtr, self)
    }

    #[inline]
    pub fn as_named(&self) -> &Box<(GosValue, Meta)> {
        unwrap_gos_val!(Named, self)
    }

    #[inline]
    pub fn is_nil(&self) -> bool {
        match &self {
            GosValue::Nil(_) => true,
            _ => false,
        }
    }

    #[inline]
    pub fn try_as_interface(&self) -> Option<&Rc<RefCell<InterfaceObj>>> {
        match &self {
            GosValue::Named(n) => Some(n.0.as_interface()),
            GosValue::Interface(_) => Some(self.as_interface()),
            _ => None,
        }
    }

    #[inline]
    pub fn try_as_struct(&self) -> Option<&Rc<(RefCell<StructObj>, RCount)>> {
        match &self {
            GosValue::Named(n) => Some(n.0.as_struct()),
            GosValue::Struct(_) => Some(self.as_struct()),
            _ => None,
        }
    }

    #[inline]
    pub fn try_as_map(&self) -> Option<&Rc<(MapObj, RCount)>> {
        match &self {
            GosValue::Named(n) => Some(n.0.as_map()),
            GosValue::Map(_) => Some(self.as_map()),
            _ => None,
        }
    }

    #[inline]
    pub fn unwrap_named(self) -> GosValue {
        match self {
            GosValue::Named(n) => n.0,
            _ => self,
        }
    }

    #[inline]
    pub fn unwrap_named_ref(&self) -> &GosValue {
        match self {
            GosValue::Named(n) => &n.0,
            _ => &self,
        }
    }

    #[inline]
    pub fn iface_underlying(&self) -> Option<GosValue> {
        match &self {
            GosValue::Named(n) => match &n.0 {
                GosValue::Nil(_) => Some(n.0.clone()),
                GosValue::Interface(i) => {
                    let b = i.borrow();
                    b.underlying_value().map(|x| x.clone())
                }
                _ => unreachable!(),
            },
            GosValue::Interface(v) => {
                let b = v.borrow();
                b.underlying_value().map(|x| x.clone())
            }
            _ => unreachable!(),
        }
    }

    #[inline]
    pub fn equals_nil(&self) -> bool {
        match &self {
            GosValue::Nil(_) => true,
            GosValue::Named(n) => n.0.is_nil(),
            GosValue::Interface(iface) => iface.borrow().is_nil(),
            _ => false,
        }
    }

    #[inline]
    pub fn typ(&self) -> ValueType {
        match self {
            GosValue::Nil(_) => ValueType::Nil,
            GosValue::Bool(_) => ValueType::Bool,
            GosValue::Int(_) => ValueType::Int,
            GosValue::Int8(_) => ValueType::Int8,
            GosValue::Int16(_) => ValueType::Int16,
            GosValue::Int32(_) => ValueType::Int32,
            GosValue::Int64(_) => ValueType::Int64,
            GosValue::Uint(_) => ValueType::Uint,
            GosValue::UintPtr(_) => ValueType::UintPtr,
            GosValue::Uint8(_) => ValueType::Uint8,
            GosValue::Uint16(_) => ValueType::Uint16,
            GosValue::Uint32(_) => ValueType::Uint32,
            GosValue::Uint64(_) => ValueType::Uint64,
            GosValue::Float32(_) => ValueType::Float32,
            GosValue::Float64(_) => ValueType::Float64,
            GosValue::Complex64(_) => ValueType::Complex64,
            GosValue::Complex128(_) => ValueType::Complex128,
            GosValue::Str(_) => ValueType::Str,
            GosValue::Array(_) => ValueType::Array,
            GosValue::Pointer(_) => ValueType::Pointer,
            GosValue::UnsafePtr(_) => ValueType::UnsafePtr,
            GosValue::Closure(_) => ValueType::Closure,
            GosValue::Slice(_) => ValueType::Slice,
            GosValue::Map(_) => ValueType::Map,
            GosValue::Interface(_) => ValueType::Interface,
            GosValue::Struct(_) => ValueType::Struct,
            GosValue::Channel(_) => ValueType::Channel,
            GosValue::Function(_) => ValueType::Function,
            GosValue::Package(_) => ValueType::Package,
            GosValue::Metadata(_) => ValueType::Metadata,
            GosValue::Named(_) => ValueType::Named,
        }
    }

    pub fn identical(&self, other: &GosValue) -> bool {
        self.typ() == other.typ() && self == other
    }

    pub fn meta(&self, objs: &VMObjects, stack: &Stack) -> Meta {
        match self {
            GosValue::Nil(m) => m.unwrap_or(objs.s_meta.none),
            GosValue::Bool(_) => objs.s_meta.mbool,
            GosValue::Int(_) => objs.s_meta.mint,
            GosValue::Int8(_) => objs.s_meta.mint8,
            GosValue::Int16(_) => objs.s_meta.mint16,
            GosValue::Int32(_) => objs.s_meta.mint32,
            GosValue::Int64(_) => objs.s_meta.mint64,
            GosValue::Uint(_) => objs.s_meta.muint,
            GosValue::UintPtr(_) => objs.s_meta.muint_ptr,
            GosValue::Uint8(_) => objs.s_meta.muint8,
            GosValue::Uint16(_) => objs.s_meta.muint16,
            GosValue::Uint32(_) => objs.s_meta.muint32,
            GosValue::Uint64(_) => objs.s_meta.muint64,
            GosValue::Float32(_) => objs.s_meta.mfloat32,
            GosValue::Float64(_) => objs.s_meta.mfloat64,
            GosValue::Complex64(_) => objs.s_meta.mcomplex64,
            GosValue::Complex128(_) => objs.s_meta.mcomplex128,
            GosValue::Str(_) => objs.s_meta.mstr,
            GosValue::Array(a) => a.0.meta,
            GosValue::Pointer(p) => p.point_to_meta(objs, stack).ptr_to(),
            GosValue::UnsafePtr(_) => objs.s_meta.unsafe_ptr,
            GosValue::Closure(c) => c.0.borrow().meta,
            GosValue::Slice(s) => s.1,
            GosValue::Map(m) => m.0.meta,
            GosValue::Interface(i) => i.borrow().meta,
            GosValue::Struct(s) => s.0.borrow().meta,
            GosValue::Channel(_) => unimplemented!(),
            GosValue::Function(_) => unimplemented!(),
            GosValue::Package(_) => unimplemented!(),
            GosValue::Metadata(_) => unimplemented!(),
            GosValue::Named(v) => v.1,
        }
    }

    #[inline]
    pub fn copy_semantic(&self, gcv: &GcoVec) -> GosValue {
        match self {
            GosValue::Slice(s) => {
                let rc = Rc::new((SliceObj::clone(&s.0), s.1, Cell::new(0)));
                gcv.add_weak(GcWeak::Slice(Rc::downgrade(&rc)));
                GosValue::Slice(rc)
            }
            GosValue::Map(m) => {
                let rc = Rc::new((MapObj::clone(&m.0), Cell::new(0)));
                gcv.add_weak(GcWeak::Map(Rc::downgrade(&rc)));
                GosValue::Map(rc)
            }
            GosValue::Struct(s) => {
                let rc = Rc::new((RefCell::clone(&s.0), Cell::new(0)));
                gcv.add_weak(GcWeak::Struct(Rc::downgrade(&rc)));
                GosValue::Struct(rc)
            }
            GosValue::Named(v) => GosValue::Named(Box::new((v.0.copy_semantic(gcv), v.1))),
            _ => self.clone(),
        }
    }

    #[inline]
    pub fn as_index(&self) -> usize {
        match self {
            GosValue::Int(i) => *i.as_int() as usize,
            GosValue::Int8(i) => *i.as_int8() as usize,
            GosValue::Int16(i) => *i.as_int16() as usize,
            GosValue::Int32(i) => *i.as_int32() as usize,
            GosValue::Int64(i) => *i.as_int64() as usize,
            GosValue::Uint(i) => *i.as_uint() as usize,
            GosValue::Uint8(i) => *i.as_uint8() as usize,
            GosValue::Uint16(i) => *i.as_uint16() as usize,
            GosValue::Uint32(i) => *i.as_uint32() as usize,
            GosValue::Uint64(i) => *i.as_uint64() as usize,
            GosValue::Named(n) => n.0.as_index(),
            _ => unreachable!(),
        }
    }

    #[inline]
    pub fn add_str(a: &GosValue, b: &GosValue) -> GosValue {
        let mut s = a.as_str().as_str().to_string();
        s.push_str(b.as_str().as_str());
        GosValue::new_str(s)
    }

    #[inline(always)]
    pub fn load_index(&self, ind: &GosValue) -> RuntimeResult<GosValue> {
        match self {
            GosValue::Map(map) => Ok(map.0.get(&ind).clone()),
            GosValue::Slice(slice) => {
                let index = ind.as_index();
                slice
                    .0
                    .get(index)
                    .map_or_else(|| Err(format!("index {} out of range", index)), |x| Ok(x))
            }
            GosValue::Str(s) => {
                let index = ind.as_index();
                s.get_byte(index).map_or_else(
                    || Err(format!("index {} out of range", index)),
                    |x| Ok(GosValue::new_int(*x as isize)),
                )
            }
            GosValue::Array(arr) => {
                let index = ind.as_index();
                arr.0
                    .get(index)
                    .map_or_else(|| Err(format!("index {} out of range", index)), |x| Ok(x))
            }
            _ => unreachable!(),
        }
    }

    #[inline]
    pub fn load_index_int(&self, i: usize) -> RuntimeResult<GosValue> {
        match self {
            GosValue::Slice(slice) => slice
                .0
                .get(i)
                .map_or_else(|| Err(format!("index {} out of range", i)), |x| Ok(x)),
            GosValue::Map(map) => {
                let ind = GosValue::new_int(i as isize);
                Ok(map.0.get(&ind).clone())
            }
            GosValue::Str(s) => s.get_byte(i).map_or_else(
                || Err(format!("index {} out of range", i)),
                |x| Ok(GosValue::new_int((*x).into())),
            ),
            GosValue::Array(arr) => arr
                .0
                .get(i)
                .map_or_else(|| Err(format!("index {} out of range", i)), |x| Ok(x)),
            GosValue::Named(n) => n.0.load_index_int(i),
            _ => {
                dbg!(self);
                unreachable!();
            }
        }
    }

    #[inline]
    pub fn load_field(&self, ind: &GosValue, objs: &VMObjects) -> GosValue {
        match self {
            GosValue::Struct(sval) => match &ind {
                GosValue::Int(i) => sval.0.borrow().fields[*i.as_int() as usize].clone(),
                _ => unreachable!(),
            },
            GosValue::Package(v) => {
                let pkg = &objs.packages[*v.as_package()];
                pkg.member(*ind.as_int() as OpIndex).clone()
            }
            _ => unreachable!(),
        }
    }

    /// for gc
    pub fn ref_sub_one(&self) {
        match &self {
            GosValue::Array(obj) => obj.1.set(obj.1.get() - 1),
            GosValue::Pointer(obj) => obj.ref_sub_one(),
            GosValue::UnsafePtr(obj) => obj.ref_sub_one(),
            GosValue::Closure(obj) => obj.1.set(obj.1.get() - 1),
            GosValue::Slice(obj) => obj.2.set(obj.2.get() - 1),
            GosValue::Map(obj) => obj.1.set(obj.1.get() - 1),
            GosValue::Interface(obj) => obj.borrow().ref_sub_one(),
            GosValue::Struct(obj) => obj.1.set(obj.1.get() - 1),
            GosValue::Named(obj) => obj.0.ref_sub_one(),
            _ => {}
        };
    }

    /// for gc
    pub fn mark_dirty(&self, queue: &mut RCQueue) {
        match &self {
            GosValue::Array(obj) => rcount_mark_and_queue(&obj.1, queue),
            GosValue::Pointer(obj) => obj.mark_dirty(queue),
            GosValue::UnsafePtr(obj) => obj.mark_dirty(queue),
            GosValue::Closure(obj) => rcount_mark_and_queue(&obj.1, queue),
            GosValue::Slice(obj) => rcount_mark_and_queue(&obj.2, queue),
            GosValue::Map(obj) => rcount_mark_and_queue(&obj.1, queue),
            GosValue::Interface(obj) => obj.borrow().mark_dirty(queue),
            GosValue::Struct(obj) => rcount_mark_and_queue(&obj.1, queue),
            GosValue::Named(obj) => obj.0.mark_dirty(queue),
            _ => {}
        };
    }

    pub fn rc(&self) -> IRC {
        match &self {
            GosValue::Array(obj) => obj.1.get(),
            GosValue::Closure(obj) => obj.1.get(),
            GosValue::Slice(obj) => obj.2.get(),
            GosValue::Map(obj) => obj.1.get(),
            GosValue::Struct(obj) => obj.1.get(),
            _ => unreachable!(),
        }
    }

    pub fn set_rc(&self, rc: IRC) {
        match &self {
            GosValue::Array(obj) => obj.1.set(rc),
            GosValue::Closure(obj) => obj.1.set(rc),
            GosValue::Slice(obj) => obj.2.set(rc),
            GosValue::Map(obj) => obj.1.set(rc),
            GosValue::Struct(obj) => obj.1.set(rc),
            _ => unreachable!(),
        }
    }
}

impl Clone for GosValue {
    #[inline(always)]
    fn clone(&self) -> Self {
        match self {
            GosValue::Nil(m) => GosValue::Nil(*m),
            GosValue::Bool(v) => GosValue::Bool(*v),
            GosValue::Int(v) => GosValue::Int(*v),
            GosValue::Int8(v) => GosValue::Int8(*v),
            GosValue::Int16(v) => GosValue::Int16(*v),
            GosValue::Int32(v) => GosValue::Int32(*v),
            GosValue::Int64(v) => GosValue::Int64(*v),
            GosValue::Uint(v) => GosValue::Uint(*v),
            GosValue::UintPtr(v) => GosValue::UintPtr(*v),
            GosValue::Uint8(v) => GosValue::Uint8(*v),
            GosValue::Uint16(v) => GosValue::Uint16(*v),
            GosValue::Uint32(v) => GosValue::Uint32(*v),
            GosValue::Uint64(v) => GosValue::Uint64(*v),
            GosValue::Float32(v) => GosValue::Float32(*v),
            GosValue::Float64(v) => GosValue::Float64(*v),
            GosValue::Complex64(v) => GosValue::Complex64(*v),
            GosValue::Complex128(v) => GosValue::Complex128(v.clone()),
            GosValue::Str(v) => GosValue::Str(v.clone()),
            GosValue::Array(v) => GosValue::Array(v.clone()),
            GosValue::Pointer(v) => GosValue::Pointer(v.clone()),
            GosValue::UnsafePtr(v) => GosValue::UnsafePtr(v.clone()),
            GosValue::Closure(v) => GosValue::Closure(v.clone()),
            GosValue::Slice(v) => GosValue::Slice(v.clone()),
            GosValue::Map(v) => GosValue::Map(v.clone()),
            GosValue::Interface(v) => GosValue::Interface(v.clone()),
            GosValue::Struct(v) => GosValue::Struct(v.clone()),
            GosValue::Channel(v) => GosValue::Channel(v.clone()),
            GosValue::Function(v) => GosValue::Function(*v),
            GosValue::Package(v) => GosValue::Package(*v),
            GosValue::Metadata(v) => GosValue::Metadata(*v),
            GosValue::Named(v) => GosValue::Named(v.clone()),
        }
    }
}

impl Eq for GosValue {}

impl PartialEq for GosValue {
    #[inline]
    fn eq(&self, b: &GosValue) -> bool {
        match (self, b) {
            (Self::Nil(_), Self::Nil(_)) => true,
            (Self::Bool(x), Self::Bool(y)) => x.as_bool() == y.as_bool(),
            (Self::Int(x), Self::Int(y)) => x.as_int() == y.as_int(),
            (Self::Int8(x), Self::Int8(y)) => x.as_int8() == y.as_int8(),
            (Self::Int16(x), Self::Int16(y)) => x.as_int16() == y.as_int16(),
            (Self::Int32(x), Self::Int32(y)) => x.as_int32() == y.as_int32(),
            (Self::Int64(x), Self::Int64(y)) => x.as_int64() == y.as_int64(),
            (Self::Uint(x), Self::Uint(y)) => x.as_uint() == y.as_uint(),
            (Self::UintPtr(x), Self::UintPtr(y)) => x.as_uint_ptr() == y.as_uint_ptr(),
            (Self::Uint8(x), Self::Uint8(y)) => x.as_uint8() == y.as_uint8(),
            (Self::Uint16(x), Self::Uint16(y)) => x.as_uint16() == y.as_uint16(),
            (Self::Uint32(x), Self::Uint32(y)) => x.as_uint32() == y.as_uint32(),
            (Self::Uint64(x), Self::Uint64(y)) => x.as_uint64() == y.as_uint64(),
            (Self::Float32(x), Self::Float32(y)) => x.as_float32() == y.as_float32(),
            (Self::Float64(x), Self::Float64(y)) => x.as_float64() == y.as_float64(),
            (Self::Complex64(x), Self::Complex64(y)) => {
                let vx = x.as_complex64();
                let vy = y.as_complex64();
                vx.0 == vy.0 && vx.1 == vy.1
            }
            (Self::Complex128(x), Self::Complex128(y)) => x.0 == y.0 && x.1 == y.1,
            (Self::Function(x), Self::Function(y)) => x.as_function() == y.as_function(),
            (Self::Package(x), Self::Package(y)) => x.as_package() == y.as_package(),
            (Self::Metadata(x), Self::Metadata(y)) => x == y,
            (Self::Str(x), Self::Str(y)) => *x == *y,
            (Self::Array(x), Self::Array(y)) => x.0 == y.0,
            (Self::Pointer(x), Self::Pointer(y)) => x == y,
            (Self::UnsafePtr(x), Self::UnsafePtr(y)) => x.eq(&**y),
            (Self::Closure(x), Self::Closure(y)) => Rc::ptr_eq(x, y),
            (Self::Slice(x), Self::Slice(y)) => Rc::ptr_eq(x, y),
            (Self::Map(x), Self::Map(y)) => Rc::ptr_eq(x, y),
            (Self::Interface(x), Self::Interface(y)) => InterfaceObj::eq(&x.borrow(), &y.borrow()),
            (Self::Struct(x), Self::Struct(y)) => StructObj::eq(&x.0.borrow(), &y.0.borrow()),
            (Self::Channel(x), Self::Channel(y)) => Rc::ptr_eq(x, y),
            (Self::Named(x), Self::Named(y)) => x.0 == y.0,
            (Self::Nil(_), nil) | (nil, Self::Nil(_)) => nil.equals_nil(),
            (Self::Interface(iface), val) | (val, Self::Interface(iface)) => {
                match iface.borrow().underlying_value() {
                    Some(v) => v == val,
                    None => false,
                }
            }
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
        match &self {
            GosValue::Bool(b) => b.as_bool().hash(state),
            GosValue::Int(i) => i.as_int().hash(state),
            GosValue::Int8(i) => i.as_int8().hash(state),
            GosValue::Int16(i) => i.as_int16().hash(state),
            GosValue::Int32(i) => i.as_int32().hash(state),
            GosValue::Int64(i) => i.as_int64().hash(state),
            GosValue::Uint(i) => i.as_uint().hash(state),
            GosValue::UintPtr(i) => i.as_uint_ptr().hash(state),
            GosValue::Uint8(i) => i.as_uint8().hash(state),
            GosValue::Uint16(i) => i.as_uint16().hash(state),
            GosValue::Uint32(i) => i.as_uint32().hash(state),
            GosValue::Uint64(i) => i.as_uint64().hash(state),
            GosValue::Float32(f) => f.as_float32().to_bits().hash(state),
            GosValue::Float64(f) => f.as_float64().to_bits().hash(state),
            GosValue::Str(s) => s.as_str().hash(state),
            GosValue::Array(a) => a.0.hash(state),
            GosValue::Complex64(c) => {
                let cv = c.as_complex64();
                cv.0.hash(state);
                cv.1.hash(state);
            }
            GosValue::Complex128(c) => {
                c.0.hash(state);
                c.1.hash(state);
            }
            GosValue::Struct(s) => {
                s.0.borrow().hash(state);
            }
            GosValue::Interface(i) => {
                i.borrow().hash(state);
            }
            GosValue::Pointer(p) => {
                PointerObj::hash(&p, state);
            }
            GosValue::UnsafePtr(p) => Rc::as_ptr(p).hash(state),
            GosValue::Named(n) => n.0.hash(state),
            _ => unreachable!(),
        }
    }
}

impl Ord for GosValue {
    fn cmp(&self, b: &Self) -> Ordering {
        match (self, b) {
            (Self::Bool(x), Self::Bool(y)) => x.as_bool().cmp(y.as_bool()),
            (Self::Int(x), Self::Int(y)) => x.as_int().cmp(y.as_int()),
            (Self::Int8(x), Self::Int8(y)) => x.as_int8().cmp(y.as_int8()),
            (Self::Int16(x), Self::Int16(y)) => x.as_int16().cmp(y.as_int16()),
            (Self::Int32(x), Self::Int32(y)) => x.as_int32().cmp(y.as_int32()),
            (Self::Int64(x), Self::Int64(y)) => x.as_int64().cmp(y.as_int64()),
            (Self::Uint(x), Self::Uint(y)) => x.as_uint().cmp(y.as_uint()),
            (Self::UintPtr(x), Self::UintPtr(y)) => x.as_uint_ptr().cmp(y.as_uint_ptr()),
            (Self::Uint8(x), Self::Uint8(y)) => x.as_uint8().cmp(y.as_uint8()),
            (Self::Uint16(x), Self::Uint16(y)) => x.as_uint16().cmp(y.as_uint16()),
            (Self::Uint32(x), Self::Uint32(y)) => x.as_uint32().cmp(y.as_uint32()),
            (Self::Uint64(x), Self::Uint64(y)) => x.as_uint64().cmp(y.as_uint64()),
            (Self::Float32(x), Self::Float32(y)) => x.as_float32().cmp(y.as_float32()),
            (Self::Float64(x), Self::Float64(y)) => x.as_float64().cmp(y.as_float64()),
            (Self::Str(x), Self::Str(y)) => x.cmp(y),
            _ => {
                dbg!(self, b);
                unreachable!()
            }
        }
    }
}

impl Display for GosValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            GosValue::Nil(_) => f.write_str("<nil>"),
            GosValue::Bool(b) => write!(f, "{}", b.as_bool()),
            GosValue::Int(i) => write!(f, "{}", i.as_int()),
            GosValue::Int8(i) => write!(f, "{}", i.as_int8()),
            GosValue::Int16(i) => write!(f, "{}", i.as_int16()),
            GosValue::Int32(i) => write!(f, "{}", i.as_int32()),
            GosValue::Int64(i) => write!(f, "{}", i.as_int64()),
            GosValue::Uint(i) => write!(f, "{}", i.as_uint()),
            GosValue::UintPtr(i) => write!(f, "{}", i.as_uint_ptr()),
            GosValue::Uint8(i) => write!(f, "{}", i.as_uint8()),
            GosValue::Uint16(i) => write!(f, "{}", i.as_uint16()),
            GosValue::Uint32(i) => write!(f, "{}", i.as_uint32()),
            GosValue::Uint64(i) => write!(f, "{}", i.as_uint64()),
            GosValue::Float32(fl) => write!(f, "{}", fl.as_float32()),
            GosValue::Float64(fl) => write!(f, "{}", fl.as_float64()),
            GosValue::Complex64(c) => write!(f, "({}, {})", c.as_complex64().0, c.as_complex64().1),
            GosValue::Complex128(b) => write!(f, "({}, {})", b.0, b.1),
            GosValue::Str(s) => f.write_str(s.as_ref().as_str()),
            GosValue::Array(a) => write!(f, "{}", a.0),
            GosValue::Pointer(p) => p.fmt(f),
            GosValue::UnsafePtr(p) => write!(f, "{:p}", Rc::as_ptr(&p)),
            GosValue::Closure(_) => f.write_str("<closure>"),
            GosValue::Slice(s) => write!(f, "{}", s.0),
            GosValue::Map(m) => write!(f, "{}", m.0),
            GosValue::Interface(i) => write!(f, "{}", i.borrow()),
            GosValue::Struct(s) => write!(f, "{}", s.0.borrow()),
            GosValue::Channel(_) => f.write_str("<channel>"),
            GosValue::Function(_) => f.write_str("<function>"),
            GosValue::Package(_) => f.write_str("<package>"),
            GosValue::Metadata(_) => f.write_str("<metadata>"),
            GosValue::Named(v) => write!(f, "{}", v.0),
        }
    }
}

// ----------------------------------------------------------------------------
// Value64
// nil is only allowed on the stack as a rhs value
// never as a lhs var, because when it's assigned to
// we wouldn't know we should release it or not
#[derive(Copy, Clone)]
#[repr(C, align(8))]
pub union V64Union {
    ubool: bool,
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
}

impl fmt::Debug for V64Union {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_fmt(format_args!("{:x}", unsafe { self.uint64 }))
    }
}

/// Value64 is a 64bit struct for VM stack to get better performance, when converting
/// to Value64, the type info is lost, Opcode is responsible for providing type info
/// when converting back to GosValue
#[derive(Copy, Clone, Debug)]
#[repr(C)]
pub struct Value64 {
    data: V64Union,
}

impl Value64 {
    #[inline]
    pub fn from_v128(v: &GosValue) -> Option<(Value64, ValueType)> {
        match v {
            GosValue::Bool(b) => Some((*b, ValueType::Bool)),
            GosValue::Int(i) => Some((*i, ValueType::Int)),
            GosValue::Int8(i) => Some((*i, ValueType::Int8)),
            GosValue::Int16(i) => Some((*i, ValueType::Int16)),
            GosValue::Int32(i) => Some((*i, ValueType::Int32)),
            GosValue::Int64(i) => Some((*i, ValueType::Int64)),
            GosValue::Uint(i) => Some((*i, ValueType::Uint)),
            GosValue::UintPtr(i) => Some((*i, ValueType::UintPtr)),
            GosValue::Uint8(i) => Some((*i, ValueType::Uint8)),
            GosValue::Uint16(i) => Some((*i, ValueType::Uint16)),
            GosValue::Uint32(i) => Some((*i, ValueType::Uint32)),
            GosValue::Uint64(i) => Some((*i, ValueType::Uint64)),
            GosValue::Float32(f) => Some((*f, ValueType::Float32)),
            GosValue::Float64(f) => Some((*f, ValueType::Float64)),
            GosValue::Complex64(c) => Some((*c, ValueType::Complex64)),
            GosValue::Function(k) => Some((*k, ValueType::Function)),
            GosValue::Package(k) => Some((*k, ValueType::Package)),
            _ => None,
        }
    }

    #[inline]
    pub fn from_bool(b: bool) -> Value64 {
        Value64 {
            data: V64Union { ubool: b },
        }
    }

    #[inline]
    pub fn from_int(i: isize) -> Value64 {
        Value64 {
            data: V64Union { int: i },
        }
    }

    #[inline]
    pub fn from_int8(i: i8) -> Value64 {
        Value64 {
            data: V64Union { int8: i },
        }
    }

    #[inline]
    pub fn from_int16(i: i16) -> Value64 {
        Value64 {
            data: V64Union { int16: i },
        }
    }

    #[inline]
    pub fn from_int32(i: i32) -> Value64 {
        Value64 {
            data: V64Union { int32: i },
        }
    }

    #[inline]
    pub fn from_int64(i: i64) -> Value64 {
        Value64 {
            data: V64Union { int64: i },
        }
    }

    #[inline]
    pub fn from_uint(u: usize) -> Value64 {
        Value64 {
            data: V64Union { uint: u },
        }
    }

    #[inline]
    pub fn from_uint_ptr(u: usize) -> Value64 {
        Value64 {
            data: V64Union { uint_ptr: u },
        }
    }

    #[inline]
    pub fn from_uint8(u: u8) -> Value64 {
        Value64 {
            data: V64Union { uint8: u },
        }
    }

    #[inline]
    pub fn from_uint16(u: u16) -> Value64 {
        Value64 {
            data: V64Union { uint16: u },
        }
    }

    #[inline]
    pub fn from_uint32(u: u32) -> Value64 {
        Value64 {
            data: V64Union { uint32: u },
        }
    }

    #[inline]
    pub fn from_uint64(u: u64) -> Value64 {
        Value64 {
            data: V64Union { uint64: u },
        }
    }

    #[inline]
    pub fn from_float32(f: F32) -> Value64 {
        Value64 {
            data: V64Union { float32: f },
        }
    }

    #[inline]
    pub fn from_float64(f: F64) -> Value64 {
        Value64 {
            data: V64Union { float64: f },
        }
    }

    #[inline]
    pub fn from_complex64(r: F32, i: F32) -> Value64 {
        Value64 {
            data: V64Union { complex64: (r, i) },
        }
    }

    #[inline]
    pub fn from_function(f: FunctionKey) -> Value64 {
        Value64 {
            data: V64Union { function: f },
        }
    }

    #[inline]
    pub fn from_package(p: PackageKey) -> Value64 {
        Value64 {
            data: V64Union { package: p },
        }
    }

    #[inline]
    pub fn from_int32_as(i: i32, t: ValueType) -> Value64 {
        let u = match t {
            ValueType::Int => V64Union { int: i as isize },
            ValueType::Int8 => V64Union { int8: i as i8 },
            ValueType::Int16 => V64Union { int16: i as i16 },
            ValueType::Int32 => V64Union { int32: i as i32 },
            ValueType::Int64 => V64Union { int64: i as i64 },
            ValueType::Uint => V64Union { uint: i as usize },
            ValueType::UintPtr => V64Union {
                uint_ptr: i as usize,
            },
            ValueType::Uint8 => V64Union { uint8: i as u8 },
            ValueType::Uint16 => V64Union { uint16: i as u16 },
            ValueType::Uint32 => V64Union { uint32: i as u32 },
            ValueType::Uint64 => V64Union { uint64: i as u64 },
            ValueType::Float32 => V64Union {
                float32: F32::from(i as f32),
            },
            ValueType::Float64 => V64Union {
                float64: F64::from(i as f64),
            },
            _ => unreachable!(),
        };
        Value64 { data: u }
    }

    #[inline]
    pub fn v128(self, t: ValueType) -> GosValue {
        match t {
            ValueType::Bool => GosValue::Bool(self),
            ValueType::Int => GosValue::Int(self),
            ValueType::Int8 => GosValue::Int8(self),
            ValueType::Int16 => GosValue::Int16(self),
            ValueType::Int32 => GosValue::Int32(self),
            ValueType::Int64 => GosValue::Int64(self),
            ValueType::Uint => GosValue::Uint(self),
            ValueType::UintPtr => GosValue::UintPtr(self),
            ValueType::Uint8 => GosValue::Uint8(self),
            ValueType::Uint16 => GosValue::Uint16(self),
            ValueType::Uint32 => GosValue::Uint32(self),
            ValueType::Uint64 => GosValue::Uint64(self),
            ValueType::Float32 => GosValue::Float32(self),
            ValueType::Float64 => GosValue::Float64(self),
            ValueType::Complex64 => GosValue::Complex64(self),
            ValueType::Function => GosValue::Function(self),
            ValueType::Package => GosValue::Package(self),
            _ => unreachable!(),
        }
    }

    #[inline]
    pub fn as_bool(&self) -> &bool {
        unsafe { &self.data.ubool }
    }

    #[inline]
    pub fn as_int(&self) -> &isize {
        unsafe { &self.data.int }
    }

    #[inline]
    pub fn as_int_mut(&mut self) -> &mut isize {
        unsafe { &mut self.data.int }
    }

    #[inline]
    pub fn as_int8(&self) -> &i8 {
        unsafe { &self.data.int8 }
    }

    #[inline]
    pub fn as_int16(&self) -> &i16 {
        unsafe { &self.data.int16 }
    }

    #[inline]
    pub fn as_int32(&self) -> &i32 {
        unsafe { &self.data.int32 }
    }

    #[inline]
    pub fn as_int64(&self) -> &i64 {
        unsafe { &self.data.int64 }
    }

    #[inline]
    pub fn as_uint(&self) -> &usize {
        unsafe { &self.data.uint }
    }

    #[inline]
    pub fn as_uint_ptr(&self) -> &usize {
        unsafe { &self.data.uint_ptr }
    }

    #[inline]
    pub fn as_uint8(&self) -> &u8 {
        unsafe { &self.data.uint8 }
    }

    #[inline]
    pub fn as_uint16(&self) -> &u16 {
        unsafe { &self.data.uint16 }
    }

    #[inline]
    pub fn as_uint32(&self) -> &u32 {
        unsafe { &self.data.uint32 }
    }

    #[inline]
    pub fn as_uint64(&self) -> &u64 {
        unsafe { &self.data.uint64 }
    }

    #[inline]
    pub fn as_float32(&self) -> &F32 {
        unsafe { &self.data.float32 }
    }

    #[inline]
    pub fn as_float64(&self) -> &F64 {
        unsafe { &self.data.float64 }
    }

    #[inline]
    pub fn as_complex64(&self) -> &(F32, F32) {
        unsafe { &self.data.complex64 }
    }

    #[inline]
    pub fn as_function(&self) -> &FunctionKey {
        unsafe { &self.data.function }
    }

    #[inline]
    pub fn as_package(&self) -> &PackageKey {
        unsafe { &self.data.package }
    }

    #[inline]
    pub fn to_uint(&mut self, t: ValueType) {
        convert_to_int!(self, t, uint, usize);
    }

    #[inline]
    pub fn to_uint_ptr(&mut self, t: ValueType) {
        convert_to_int!(self, t, uint_ptr, usize);
    }

    #[inline]
    pub fn to_uint8(&mut self, t: ValueType) {
        convert_to_int!(self, t, uint8, u8);
    }

    #[inline]
    pub fn to_uint16(&mut self, t: ValueType) {
        convert_to_int!(self, t, uint16, u16);
    }

    #[inline]
    pub fn to_uint32(&mut self, t: ValueType) {
        convert_to_int!(self, t, uint32, u32);
    }

    #[inline]
    pub fn to_uint64(&mut self, t: ValueType) {
        convert_to_int!(self, t, uint64, u64);
    }

    #[inline]
    pub fn to_int(&mut self, t: ValueType) {
        convert_to_int!(self, t, int, isize);
    }

    #[inline]
    pub fn to_int8(&mut self, t: ValueType) {
        convert_to_int!(self, t, int8, i8);
    }

    #[inline]
    pub fn to_int16(&mut self, t: ValueType) {
        convert_to_int!(self, t, int16, i16);
    }

    #[inline]
    pub fn to_int32(&mut self, t: ValueType) {
        convert_to_int!(self, t, int32, i32);
    }

    #[inline]
    pub fn to_int64(&mut self, t: ValueType) {
        convert_to_int!(self, t, int64, i64);
    }

    #[inline]
    pub fn to_float32(&mut self, t: ValueType) {
        convert_to_float!(self, t, float32, F32, f32);
    }

    #[inline]
    pub fn to_float64(&mut self, t: ValueType) {
        convert_to_float!(self, t, float64, F64, f64);
    }

    #[inline]
    pub fn unary_negate(&mut self, t: ValueType) {
        match t {
            ValueType::Int => self.data.int = -unsafe { self.data.int },
            ValueType::Int8 => self.data.int8 = -unsafe { self.data.int8 },
            ValueType::Int16 => self.data.int16 = -unsafe { self.data.int16 },
            ValueType::Int32 => self.data.int32 = -unsafe { self.data.int32 },
            ValueType::Int64 => self.data.int64 = -unsafe { self.data.int64 },
            ValueType::Float32 => self.data.float32 = -unsafe { self.data.float32 },
            ValueType::Float64 => self.data.float64 = -unsafe { self.data.float64 },
            ValueType::Uint => self.data.uint = unsafe { (!0) ^ self.data.uint } + 1,
            ValueType::Uint8 => self.data.uint8 = unsafe { (!0) ^ self.data.uint8 } + 1,
            ValueType::Uint16 => self.data.uint16 = unsafe { (!0) ^ self.data.uint16 } + 1,
            ValueType::Uint32 => self.data.uint32 = unsafe { (!0) ^ self.data.uint32 } + 1,
            ValueType::Uint64 => self.data.uint64 = unsafe { (!0) ^ self.data.uint64 } + 1,
            _ => unreachable!(),
        }
    }

    #[inline]
    pub fn unary_xor(&mut self, t: ValueType) {
        match t {
            ValueType::Uint => self.data.uint = unsafe { (!0) ^ self.data.uint },
            ValueType::Uint8 => self.data.uint8 = unsafe { (!0) ^ self.data.uint8 },
            ValueType::Uint16 => self.data.uint16 = unsafe { (!0) ^ self.data.uint16 },
            ValueType::Uint32 => self.data.uint32 = unsafe { (!0) ^ self.data.uint32 },
            ValueType::Uint64 => self.data.uint64 = unsafe { (!0) ^ self.data.uint64 },
            ValueType::Int => self.data.int = unsafe { -1 ^ self.data.int },
            ValueType::Int8 => self.data.int8 = unsafe { -1 ^ self.data.int8 },
            ValueType::Int16 => self.data.int16 = unsafe { -1 ^ self.data.int16 },
            ValueType::Int32 => self.data.int32 = unsafe { -1 ^ self.data.int32 },
            ValueType::Int64 => self.data.int64 = unsafe { -1 ^ self.data.int64 },
            _ => unreachable!(),
        }
    }

    #[inline]
    pub fn unary_not(&mut self, t: ValueType) {
        debug_assert!(t == ValueType::Bool);
        self.data.ubool = unsafe { !self.data.ubool };
    }

    #[inline]
    pub fn inc(&mut self, t: ValueType) {
        match t {
            ValueType::Int => self.data.int = unsafe { self.data.int } + 1,
            ValueType::Int8 => self.data.int8 = unsafe { self.data.int8 } + 1,
            ValueType::Int16 => self.data.int16 = unsafe { self.data.int16 } + 1,
            ValueType::Int32 => self.data.int32 = unsafe { self.data.int32 } + 1,
            ValueType::Int64 => self.data.int64 = unsafe { self.data.int64 } + 1,
            ValueType::Float32 => self.data.float32 = unsafe { self.data.float32 } + 1.0,
            ValueType::Float64 => self.data.float64 = unsafe { self.data.float64 } + 1.0,
            ValueType::Uint => self.data.uint = unsafe { self.data.uint } + 1,
            ValueType::Uint8 => self.data.uint8 = unsafe { self.data.uint8 } + 1,
            ValueType::Uint16 => self.data.uint16 = unsafe { self.data.uint16 } + 1,
            ValueType::Uint32 => self.data.uint32 = unsafe { self.data.uint32 } + 1,
            ValueType::Uint64 => self.data.uint64 = unsafe { self.data.uint64 } + 1,
            _ => unreachable!(),
        }
    }

    #[inline]
    pub fn dec(&mut self, t: ValueType) {
        match t {
            ValueType::Int => self.data.int = unsafe { self.data.int } - 1,
            ValueType::Int8 => self.data.int8 = unsafe { self.data.int8 } - 1,
            ValueType::Int16 => self.data.int16 = unsafe { self.data.int16 } - 1,
            ValueType::Int32 => self.data.int32 = unsafe { self.data.int32 } - 1,
            ValueType::Int64 => self.data.int64 = unsafe { self.data.int64 } - 1,
            ValueType::Float32 => self.data.float32 = unsafe { self.data.float32 } - 1.0,
            ValueType::Float64 => self.data.float64 = unsafe { self.data.float64 } - 1.0,
            ValueType::Uint => self.data.uint = unsafe { self.data.uint } - 1,
            ValueType::Uint8 => self.data.uint8 = unsafe { self.data.uint8 } - 1,
            ValueType::Uint16 => self.data.uint16 = unsafe { self.data.uint16 } - 1,
            ValueType::Uint32 => self.data.uint32 = unsafe { self.data.uint32 } - 1,
            ValueType::Uint64 => self.data.uint64 = unsafe { self.data.uint64 } - 1,
            _ => unreachable!(),
        }
    }

    #[inline]
    pub fn binary_op_add(&self, b: &Value64, t: ValueType) -> Value64 {
        unsafe { binary_op_int_float!(t, self, b, +) }
    }

    #[inline]
    pub fn binary_op_sub(&self, b: &Value64, t: ValueType) -> Value64 {
        unsafe { binary_op_int_float!(t, self, b, -) }
    }

    #[inline]
    pub fn binary_op_mul(&self, b: &Value64, t: ValueType) -> Value64 {
        unsafe { binary_op_int_float!(t, self, b, *) }
    }

    #[inline]
    pub fn binary_op_quo(&self, b: &Value64, t: ValueType) -> Value64 {
        unsafe { binary_op_int_float!(t, self, b, /) }
    }

    #[inline]
    pub fn binary_op_rem(&self, b: &Value64, t: ValueType) -> Value64 {
        unsafe { binary_op_int_no_wrap!(t, self, b, %) }
    }

    #[inline]
    pub fn binary_op_and(&self, b: &Value64, t: ValueType) -> Value64 {
        unsafe { binary_op_int_no_wrap!(t, self, b, &) }
    }

    #[inline]
    pub fn binary_op_or(&self, b: &Value64, t: ValueType) -> Value64 {
        unsafe { binary_op_int_no_wrap!(t, self, b, |) }
    }

    #[inline]
    pub fn binary_op_xor(&self, b: &Value64, t: ValueType) -> Value64 {
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
    pub fn binary_op_and_not(&self, b: &Value64, t: ValueType) -> Value64 {
        Value64 {
            //debug_type: t,
            data: unsafe {
                match t {
                    ValueType::Int => V64Union {
                        int: self.data.int & !b.data.int,
                    },
                    ValueType::Int8 => V64Union {
                        int8: self.data.int8 & !b.data.int8,
                    },
                    ValueType::Int16 => V64Union {
                        int16: self.data.int16 & !b.data.int16,
                    },
                    ValueType::Int32 => V64Union {
                        int32: self.data.int32 & !b.data.int32,
                    },
                    ValueType::Int64 => V64Union {
                        int64: self.data.int64 & !b.data.int64,
                    },
                    ValueType::Uint => V64Union {
                        uint: self.data.uint & !b.data.uint,
                    },
                    ValueType::Uint8 => V64Union {
                        uint8: self.data.uint8 & !b.data.uint8,
                    },
                    ValueType::Uint16 => V64Union {
                        uint16: self.data.uint16 & !b.data.uint16,
                    },
                    ValueType::Uint32 => V64Union {
                        uint32: self.data.uint32 & !b.data.uint32,
                    },
                    ValueType::Uint64 => V64Union {
                        uint64: self.data.uint64 & !b.data.uint64,
                    },
                    _ => unreachable!(),
                }
            },
        }
    }

    #[inline]
    pub fn compare_eql(a: &Value64, b: &Value64, t: ValueType) -> bool {
        unsafe { cmp_bool_int_float!(t, a, b, ==) }
    }

    #[inline]
    pub fn compare_neq(a: &Value64, b: &Value64, t: ValueType) -> bool {
        unsafe { cmp_bool_int_float!(t, a, b, !=) }
    }

    #[inline]
    pub fn compare_lss(a: &Value64, b: &Value64, t: ValueType) -> bool {
        unsafe { cmp_int_float!(t, a, b, <) }
    }

    #[inline]
    pub fn compare_gtr(a: &Value64, b: &Value64, t: ValueType) -> bool {
        unsafe { cmp_int_float!(t, a, b, >) }
    }

    #[inline]
    pub fn compare_leq(a: &Value64, b: &Value64, t: ValueType) -> bool {
        unsafe { cmp_int_float!(t, a, b, <=) }
    }

    #[inline]
    pub fn compare_geq(a: &Value64, b: &Value64, t: ValueType) -> bool {
        unsafe { cmp_int_float!(t, a, b, >=) }
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
        dbg!(mem::size_of::<SliceObj>());
        dbg!(mem::size_of::<RefCell<GosValue>>());
        dbg!(mem::size_of::<GosValue>());
        dbg!(mem::size_of::<Value64>());
        dbg!(mem::size_of::<Meta>());

        let mut h: HashMap<isize, isize> = HashMap::new();
        h.insert(0, 1);
        let mut h2 = h.clone();
        h2.insert(0, 3);
        dbg!(h[&0]);
        dbg!(h2[&0]);
    }
}
