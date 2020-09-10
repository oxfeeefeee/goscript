//#![allow(dead_code)]
use super::instruction::{Opcode, ValueType};
pub use super::objects::*;
use ordered_float;
use std::cell::RefCell;
use std::cmp::Ordering;
use std::fmt;
use std::hash::{Hash, Hasher};
use std::rc::Rc;

type F32 = ordered_float::OrderedFloat<f32>;
type F64 = ordered_float::OrderedFloat<f64>;

pub const C_PLACE_HOLDER: GosValue64 = GosValue64 {
    data: V64Union { uint: 9527 },
};
pub const RC_PLACE_HOLDER: GosValue = GosValue::Int(9528);

macro_rules! unwrap_gos_val {
    ($name:tt, $self_:ident) => {
        if let GosValue::$name(k) = $self_ {
            k
        } else {
            unreachable!();
        }
    };
}

macro_rules! union_op {
    ($a:ident, $b:ident, $name:tt, $op:tt, $t:expr) => {
        GosValue64{
            data: V64Union {
            $name: $a.data.$name $op $b.data.$name,
        }}
    };
}

macro_rules! union_cmp {
    ($a:ident, $b:ident, $name:tt, $op:tt, $t:expr) => {
        $a.data.$name $op $b.data.$name
    };
}

macro_rules! binary_op_int_float {
    ($t:ident, $a:ident, $b:ident, $op:tt) => {
        match $t {
            ValueType::Int => union_op!($a, $b, uint, $op, ValueType::Int),
            ValueType::Float64 => union_op!($a, $b, ufloat64, $op, ValueType::Float64),
            _ => unreachable!(),
        }
    };
}

macro_rules! cmp_bool_int_float {
    ($t:ident, $a:ident, $b:ident, $op:tt) => {
        match $t {
            ValueType::Bool => union_cmp!($a, $b, ubool, $op, ValueType::Bool),
            ValueType::Int => union_cmp!($a, $b, uint, $op, ValueType::Int),
            ValueType::Float64 => union_cmp!($a, $b, ufloat64, $op, ValueType::Float64),
            _ => unreachable!(),
        }
    };
}

macro_rules! cmp_int_float {
    ($t:ident, $a:ident, $b:ident, $op:tt) => {
        match $t {
            ValueType::Int => union_cmp!($a, $b, uint, $op, ValueType::Int),
            ValueType::Float64 => union_cmp!($a, $b, ufloat64, $op, ValueType::Float64),
            _ => unreachable!(),
        }
    };
}

// ----------------------------------------------------------------------------
// GosValue
#[derive(Debug, Clone)]
pub enum GosValue {
    Bool(bool),
    Int(isize),
    Float64(F64), // becasue in Go there is no "float", just float64
    Complex64(F32, F32),

    // the 3 below are not visible to users, they are "values" not "variables"
    // they are static data, don't use Rc for better performance
    Function(FunctionKey),
    Package(PackageKey),
    Metadata(MetadataKey),

    Str(Rc<StringVal>), // "String" is taken
    Boxed(Rc<RefCell<GosValue>>),
    Closure(Rc<ClosureVal>),
    Slice(Rc<SliceVal>),
    Map(Rc<MapVal>),
    Interface(Rc<RefCell<InterfaceVal>>),
    Struct(Rc<RefCell<StructVal>>),
    Channel(Rc<RefCell<ChannelVal>>),
}

impl GosValue {
    #[inline]
    pub fn new_str(s: String) -> GosValue {
        GosValue::Str(Rc::new(StringVal::with_str(s)))
    }

    #[inline]
    pub fn new_boxed(v: GosValue) -> GosValue {
        GosValue::Boxed(Rc::new(RefCell::new(v)))
    }

    #[inline]
    pub fn new_slice(len: usize, cap: usize, dval: &GosValue, slices: &mut SliceObjs) -> GosValue {
        let s = Rc::new(SliceVal::new(len, cap, dval));
        slices.push(Rc::downgrade(&s));
        GosValue::Slice(s)
    }

    #[inline]
    pub fn with_slice_val(val: Vec<GosValue>, slices: &mut SliceObjs) -> GosValue {
        let s = Rc::new(SliceVal::with_data(val));
        slices.push(Rc::downgrade(&s));
        GosValue::Slice(s)
    }

    #[inline]
    pub fn new_map(default_val: GosValue, maps: &mut MapObjs) -> GosValue {
        let val = Rc::new(MapVal::new(default_val));
        maps.push(Rc::downgrade(&val));
        GosValue::Map(val)
    }

    #[inline]
    pub fn new_function(
        package: PackageKey,
        meta: GosValue,
        variadic: Option<ValueType>,
        ctor: bool,
        objs: &mut VMObjects,
    ) -> GosValue {
        let val = FunctionVal::new(package, meta, variadic, ctor);
        GosValue::Function(objs.functions.insert(val))
    }

    #[inline]
    pub fn new_closure(fkey: FunctionKey, upvalues: Option<Vec<UpValue>>) -> GosValue {
        let val = ClosureVal::new(fkey, None, upvalues);
        GosValue::Closure(Rc::new(val))
    }

    #[inline]
    pub fn new_iface(meta: GosValue, ifaces: &mut InterfaceObjs) -> GosValue {
        let val = Rc::new(RefCell::new(InterfaceVal::new(meta)));
        ifaces.push(Rc::downgrade(&val));
        GosValue::Interface(val)
    }

    #[inline]
    pub fn new_meta(t: MetadataVal, metas: &mut MetadataObjs) -> GosValue {
        GosValue::Metadata(metas.insert(t))
    }

    #[inline]
    pub fn as_bool(&self) -> &bool {
        unwrap_gos_val!(Bool, self)
    }

    #[inline]
    pub fn as_int(&self) -> &isize {
        unwrap_gos_val!(Int, self)
    }

    #[inline]
    pub fn as_int_mut(&mut self) -> &mut isize {
        unwrap_gos_val!(Int, self)
    }

    #[inline]
    pub fn as_float(&self) -> &f64 {
        unwrap_gos_val!(Float64, self)
    }

    #[inline]
    pub fn as_str(&self) -> &Rc<StringVal> {
        unwrap_gos_val!(Str, self)
    }

    #[inline]
    pub fn as_slice(&self) -> &Rc<SliceVal> {
        unwrap_gos_val!(Slice, self)
    }

    #[inline]
    pub fn as_map(&self) -> &Rc<MapVal> {
        unwrap_gos_val!(Map, self)
    }

    #[inline]
    pub fn as_interface(&self) -> &Rc<RefCell<InterfaceVal>> {
        unwrap_gos_val!(Interface, self)
    }

    #[inline]
    pub fn as_channel(&self) -> &Rc<RefCell<ChannelVal>> {
        unwrap_gos_val!(Channel, self)
    }

    #[inline]
    pub fn as_function(&self) -> &FunctionKey {
        unwrap_gos_val!(Function, self)
    }

    #[inline]
    pub fn as_package(&self) -> &PackageKey {
        unwrap_gos_val!(Package, self)
    }

    #[inline]
    pub fn as_struct(&self) -> &Rc<RefCell<StructVal>> {
        unwrap_gos_val!(Struct, self)
    }

    #[inline]
    pub fn as_closure(&self) -> &Rc<ClosureVal> {
        unwrap_gos_val!(Closure, self)
    }

    #[inline]
    pub fn as_meta(&self) -> &MetadataKey {
        unwrap_gos_val!(Metadata, self)
    }

    #[inline]
    pub fn as_boxed(&self) -> &Rc<RefCell<GosValue>> {
        unwrap_gos_val!(Boxed, self)
    }

    #[inline]
    pub fn meta_get_value_type(&self, metas: &MetadataObjs) -> ValueType {
        metas[*self.as_meta()].get_value_type(metas)
    }

    #[inline]
    pub fn get_type(&self) -> ValueType {
        match self {
            GosValue::Bool(_) => ValueType::Bool,
            GosValue::Int(_) => ValueType::Int,
            GosValue::Float64(_) => ValueType::Float64,
            GosValue::Complex64(_, _) => ValueType::Complex64,
            GosValue::Str(_) => ValueType::Str,
            GosValue::Boxed(_) => ValueType::Boxed,
            GosValue::Closure(_) => ValueType::Closure,
            GosValue::Slice(_) => ValueType::Slice,
            GosValue::Map(_) => ValueType::Map,
            GosValue::Interface(_) => ValueType::Interface,
            GosValue::Struct(_) => ValueType::Struct,
            GosValue::Channel(_) => ValueType::Channel,
            GosValue::Function(_) => ValueType::Function,
            GosValue::Package(_) => ValueType::Package,
            GosValue::Metadata(_) => ValueType::Metadata,
        }
    }

    #[inline]
    pub fn copy_semantic(&self, nil: Option<(&ZeroVal, ValueType)>) -> GosValue {
        match self {
            GosValue::Int(i) => {
                // this is nil
                assert_eq!(*i, 9528);
                let (zv, t) = nil.unwrap();
                zv.nil_zero_val(t).clone()
            }
            GosValue::Slice(s) => GosValue::Slice(Rc::new(SliceVal::clone(s))),
            GosValue::Map(m) => GosValue::Map(Rc::new(MapVal::clone(m))),
            GosValue::Struct(s) => GosValue::Struct(Rc::new(RefCell::clone(s))),
            _ => self.clone(),
        }
    }

    #[inline]
    pub fn add_str(a: &GosValue, b: &GosValue) -> GosValue {
        let mut s = a.as_str().as_str().to_string();
        s.push_str(b.as_str().as_str());
        GosValue::new_str(s)
    }
}

impl Eq for GosValue {}

impl PartialEq for GosValue {
    #[inline]
    fn eq(&self, b: &GosValue) -> bool {
        match (self, b) {
            // todo: not the "correct" implementation yet,
            (GosValue::Bool(x), GosValue::Bool(y)) => x == y,
            (GosValue::Int(x), GosValue::Int(y)) => x == y,
            (GosValue::Float64(x), GosValue::Float64(y)) => x == y,
            (GosValue::Complex64(xr, xi), GosValue::Complex64(yr, yi)) => xr == yr && xi == yi,
            (GosValue::Str(sa), GosValue::Str(sb)) => *sa == *sb,
            (GosValue::Closure(sa), GosValue::Closure(sb)) => Rc::ptr_eq(sa, sb),
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

impl Ord for GosValue {
    #[inline]
    fn cmp(&self, b: &Self) -> Ordering {
        match (self, b) {
            // todo: not the "correct" implementation yet,
            (GosValue::Bool(x), GosValue::Bool(y)) => x.cmp(y),
            (GosValue::Int(x), GosValue::Int(y)) => x.cmp(y),
            (GosValue::Float64(x), GosValue::Float64(y)) => {
                match x.partial_cmp(y) {
                    Some(order) => order,
                    None => Ordering::Less, // todo: not "correct" implementation
                }
            }
            //(GosValue::Complex64(_, _), GosValue::Complex64(_, _)) => unreachable!(),
            (GosValue::Str(sa), GosValue::Str(sb)) => sa.cmp(&*sb),
            //(GosValue::Slice(_), GosValue::Slice(_)) => unreachable!(),
            _ => {
                dbg!(self, b);
                unimplemented!()
            }
        }
    }
}

impl Hash for GosValue {
    fn hash<H: Hasher>(&self, state: &mut H) {
        match &self {
            GosValue::Bool(b) => b.hash(state),
            GosValue::Int(i) => i.hash(state),
            GosValue::Float64(f) => f.to_bits().hash(state),
            GosValue::Str(s) => s.as_str().hash(state),
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

/// A helper struct for printing debug info of GosValue, we cannot rely on GosValue::Debug
/// because it requires 'objs' to access the inner data
pub struct GosValueDebug<'a> {
    val: &'a GosValue,
    objs: &'a VMObjects,
}

impl<'a> GosValueDebug<'a> {
    pub fn new(val: &'a GosValue, objs: &'a VMObjects) -> GosValueDebug<'a> {
        GosValueDebug {
            val: val,
            objs: objs,
        }
    }
}

impl<'a> fmt::Debug for GosValueDebug<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.val {
            GosValue::Bool(_)
            | GosValue::Int(_)
            | GosValue::Float64(_)
            | GosValue::Complex64(_, _) => self.val.fmt(f),
            GosValue::Str(k) => k.fmt(f),
            GosValue::Closure(k) => k.fmt(f),
            GosValue::Slice(k) => k.fmt(f),
            GosValue::Map(k) => k.fmt(f),
            GosValue::Interface(k) => k.fmt(f),
            GosValue::Struct(k) => k.fmt(f),
            GosValue::Channel(k) => k.fmt(f),
            GosValue::Boxed(k) => k.fmt(f),
            GosValue::Function(k) => self.objs.functions[*k].fmt(f),
            GosValue::Package(k) => self.objs.packages[*k].fmt(f),
            GosValue::Metadata(k) => self.objs.metas[*k].fmt(f),
            //_ => unreachable!(),
        }
    }
}

// ----------------------------------------------------------------------------
// GosValue64
// nil is only allowed on the stack as a rhs value
// never as a lhs var, because when it's assigned to
// we wouldn't know we should release it or not
#[derive(Copy, Clone)]
pub union V64Union {
    unil: (),
    ubool: bool,
    uint: isize,
    ufloat64: F64,
    ucomplex64: (F32, F32),
    umetadata: MetadataKey,
    ufunction: FunctionKey,
    upackage: PackageKey,
}

/// GosValue64 is a 64bit struct for VM stack to get better performance, when converting
/// to GosValue64, the type info is lost, Opcode is responsible for providing type info
/// when converting back to GosValue
#[derive(Copy, Clone)]
pub struct GosValue64 {
    data: V64Union,
    //pub debug_type: ValueType, // to be removed in release build
}

impl GosValue64 {
    #[inline]
    pub fn from_v128(v: &GosValue) -> (GosValue64, ValueType) {
        let (data, typ) = match v {
            GosValue::Bool(b) => (V64Union { ubool: *b }, ValueType::Bool),
            GosValue::Int(i) => (V64Union { uint: *i }, ValueType::Int),
            GosValue::Float64(f) => (V64Union { ufloat64: *f }, ValueType::Float64),
            GosValue::Complex64(f1, f2) => (
                V64Union {
                    ucomplex64: (*f1, *f2),
                },
                ValueType::Complex64,
            ),
            GosValue::Function(k) => (V64Union { ufunction: *k }, ValueType::Function),
            GosValue::Package(k) => (V64Union { upackage: *k }, ValueType::Package),
            GosValue::Metadata(k) => (V64Union { umetadata: *k }, ValueType::Metadata),
            _ => unreachable!(),
        };
        (
            GosValue64 {
                data: data,
                //debug_type: typ,
            },
            typ,
        )
    }

    #[inline]
    pub fn nil() -> GosValue64 {
        GosValue64 {
            data: V64Union { unil: () },
            //debug_type: ValueType::Nil,
        }
    }

    #[inline]
    pub fn from_bool(b: bool) -> GosValue64 {
        GosValue64 {
            data: V64Union { ubool: b },
            //debug_type: ValueType::Bool,
        }
    }

    #[inline]
    pub fn from_int(i: isize) -> GosValue64 {
        GosValue64 {
            data: V64Union { uint: i },
            //debug_type: ValueType::Int,
        }
    }

    #[inline]
    pub fn from_float64(f: F64) -> GosValue64 {
        GosValue64 {
            data: V64Union { ufloat64: f },
            //debug_type: ValueType::Float64,
        }
    }

    #[inline]
    pub fn from_complex64(r: F32, i: F32) -> GosValue64 {
        GosValue64 {
            data: V64Union { ucomplex64: (r, i) },
            //debug_type: ValueType::Complex64,
        }
    }

    /// returns GosValue and increases RC
    #[inline]
    pub fn get_v128(&self, t: ValueType) -> GosValue {
        //debug_assert!(t == self.debug_type);
        unsafe {
            match t {
                ValueType::Bool => GosValue::Bool(self.data.ubool),
                ValueType::Int => GosValue::Int(self.data.uint),
                ValueType::Float64 => GosValue::Float64(self.data.ufloat64),
                ValueType::Complex64 => {
                    GosValue::Complex64(self.data.ucomplex64.0, self.data.ucomplex64.1)
                }
                ValueType::Function => GosValue::Function(self.data.ufunction),
                ValueType::Package => GosValue::Package(self.data.upackage),
                ValueType::Metadata => GosValue::Metadata(self.data.umetadata),
                _ => unreachable!(),
            }
        }
    }

    #[inline]
    pub fn get_bool(&self) -> bool {
        //debug_assert_eq!(self.debug_type, ValueType::Bool);
        unsafe { self.data.ubool }
    }

    #[inline]
    pub fn get_int(&self) -> isize {
        //debug_assert_eq!(self.debug_type, ValueType::Int);
        unsafe { self.data.uint }
    }

    #[inline]
    pub fn get_float64(&self) -> F64 {
        //debug_assert_eq!(self.debug_type, ValueType::Float64);
        unsafe { self.data.ufloat64 }
    }

    #[inline]
    pub fn get_complex64(&self) -> (F32, F32) {
        //debug_assert_eq!(self.debug_type, ValueType::Complex64);
        unsafe { self.data.ucomplex64 }
    }

    /// returns GosValue without increasing RC
    #[inline]
    pub fn into_v128(&self, t: ValueType) -> GosValue {
        //dbg!(t, self.debug_type);
        //debug_assert!(t == self.debug_type);
        unsafe {
            match t {
                ValueType::Bool => GosValue::Bool(self.data.ubool),
                ValueType::Int => GosValue::Int(self.data.uint),
                ValueType::Float64 => GosValue::Float64(self.data.ufloat64),
                ValueType::Complex64 => {
                    GosValue::Complex64(self.data.ucomplex64.0, self.data.ucomplex64.1)
                }
                ValueType::Function => GosValue::Function(self.data.ufunction),
                ValueType::Package => GosValue::Package(self.data.upackage),
                ValueType::Metadata => GosValue::Metadata(self.data.umetadata),
                _ => unreachable!(),
            }
        }
    }

    #[inline]
    pub fn unary_negate(&mut self, t: ValueType) {
        match t {
            ValueType::Int => self.data.uint = -unsafe { self.data.uint },
            ValueType::Float64 => self.data.ufloat64 = -unsafe { self.data.ufloat64 },
            _ => unreachable!(),
        }
    }

    #[inline]
    pub fn unary_xor(&mut self, t: ValueType) {
        debug_assert!(t == ValueType::Int);
        self.data.uint = unsafe { -1 ^ self.data.uint };
    }

    #[inline]
    pub fn unary_not(&mut self, t: ValueType) {
        debug_assert!(t == ValueType::Bool);
        self.data.ubool = unsafe { !self.data.ubool };
    }

    #[inline]
    pub fn binary_op_add(a: &GosValue64, b: &GosValue64, t: ValueType) -> GosValue64 {
        unsafe { binary_op_int_float!(t, a, b, +) }
    }

    #[inline]
    pub fn binary_op_sub(a: &GosValue64, b: &GosValue64, t: ValueType) -> GosValue64 {
        unsafe { binary_op_int_float!(t, a, b, -) }
    }

    #[inline]
    pub fn binary_op_mul(a: &GosValue64, b: &GosValue64, t: ValueType) -> GosValue64 {
        unsafe { binary_op_int_float!(t, a, b, *) }
    }

    #[inline]
    pub fn binary_op_quo(a: &GosValue64, b: &GosValue64, t: ValueType) -> GosValue64 {
        unsafe { binary_op_int_float!(t, a, b, /) }
    }

    #[inline]
    pub fn binary_op_rem(a: &GosValue64, b: &GosValue64, _t: ValueType) -> GosValue64 {
        unsafe { union_op!(a, b, uint, %, t) }
    }

    #[inline]
    pub fn binary_op_and(a: &GosValue64, b: &GosValue64, _t: ValueType) -> GosValue64 {
        unsafe { union_op!(a, b, uint, &, t) }
    }

    #[inline]
    pub fn binary_op_or(a: &GosValue64, b: &GosValue64, _t: ValueType) -> GosValue64 {
        unsafe { union_op!(a, b, uint, |, t) }
    }

    #[inline]
    pub fn binary_op_xor(a: &GosValue64, b: &GosValue64, _t: ValueType) -> GosValue64 {
        unsafe { union_op!(a, b, uint, ^, t) }
    }

    #[inline]
    pub fn binary_op_shl(a: &GosValue64, b: &GosValue64, _t: ValueType) -> GosValue64 {
        unsafe { union_op!(a, b, uint, <<, t) }
    }

    #[inline]
    pub fn binary_op_shr(a: &GosValue64, b: &GosValue64, _t: ValueType) -> GosValue64 {
        unsafe { union_op!(a, b, uint, >>, t) }
    }

    #[inline]
    pub fn binary_op_and_not(a: &GosValue64, b: &GosValue64, _t: ValueType) -> GosValue64 {
        GosValue64 {
            //debug_type: t,
            data: unsafe {
                V64Union {
                    uint: a.data.uint & !b.data.uint,
                }
            },
        }
    }

    #[inline]
    pub fn binary_op(a: &GosValue64, b: &GosValue64, t: ValueType, op: Opcode) -> GosValue64 {
        match op {
            Opcode::ADD => GosValue64::binary_op_add(a, b, t),
            Opcode::SUB => GosValue64::binary_op_sub(a, b, t),
            Opcode::MUL => GosValue64::binary_op_mul(a, b, t),
            Opcode::QUO => GosValue64::binary_op_quo(a, b, t),
            Opcode::REM => GosValue64::binary_op_rem(a, b, t),
            Opcode::AND => GosValue64::binary_op_and(a, b, t),
            Opcode::OR => GosValue64::binary_op_or(a, b, t),
            Opcode::XOR => GosValue64::binary_op_xor(a, b, t),
            Opcode::SHL => GosValue64::binary_op_shl(a, b, t),
            Opcode::SHR => GosValue64::binary_op_shr(a, b, t),
            Opcode::AND_NOT => GosValue64::binary_op_and_not(a, b, t),
            _ => unreachable!(),
        }
    }

    #[inline]
    pub fn compare_eql(a: &GosValue64, b: &GosValue64, t: ValueType) -> bool {
        unsafe { cmp_bool_int_float!(t, a, b, ==) }
    }

    #[inline]
    pub fn compare_neq(a: &GosValue64, b: &GosValue64, t: ValueType) -> bool {
        unsafe { cmp_bool_int_float!(t, a, b, !=) }
    }

    #[inline]
    pub fn compare_lss(a: &GosValue64, b: &GosValue64, t: ValueType) -> bool {
        unsafe { cmp_int_float!(t, a, b, <) }
    }

    #[inline]
    pub fn compare_gtr(a: &GosValue64, b: &GosValue64, t: ValueType) -> bool {
        unsafe { cmp_int_float!(t, a, b, >) }
    }

    #[inline]
    pub fn compare_leq(a: &GosValue64, b: &GosValue64, t: ValueType) -> bool {
        unsafe { cmp_int_float!(t, a, b, <=) }
    }

    #[inline]
    pub fn compare_geq(a: &GosValue64, b: &GosValue64, t: ValueType) -> bool {
        unsafe { cmp_int_float!(t, a, b, >=) }
    }
}

#[cfg(test)]
mod test {
    use super::super::value::*;
    use std::collections::HashMap;
    use std::mem;

    #[test]
    fn test_gosvalue_debug() {
        let mut o = VMObjects::new();
        let s = GosValue::new_str("test_string".to_string());
        let slice = GosValue::new_slice(10, 10, &GosValue::Int(0), &mut o.slices);
        dbg!(
            GosValueDebug::new(&s, &o),
            //GosValueDebug::new(&GosValue::Nil, &o),
            GosValueDebug::new(&GosValue::Int(1), &o),
            GosValueDebug::new(&slice, &o),
        );
    }

    #[test]
    fn test_types() {
        let _t1: Vec<GosValue> = vec![
            GosValue::new_str("Norway".to_string()),
            GosValue::Int(100),
            GosValue::new_str("Denmark".to_string()),
            GosValue::Int(10),
        ];

        let _t2: Vec<GosValue> = vec![
            GosValue::new_str("Norway".to_string()),
            GosValue::Int(100),
            GosValue::new_str("Denmark".to_string()),
            GosValue::Int(10),
        ];
    }

    #[test]
    fn test_size() {
        dbg!(mem::size_of::<HashMap<GosValue, GosValue>>());
        dbg!(mem::size_of::<String>());
        dbg!(mem::size_of::<Rc<String>>());
        dbg!(mem::size_of::<SliceVal>());
        dbg!(mem::size_of::<RefCell<GosValue>>());
        dbg!(mem::size_of::<GosValue>());
        dbg!(mem::size_of::<GosValue64>());

        let mut h: HashMap<isize, isize> = HashMap::new();
        h.insert(0, 1);
        let mut h2 = h.clone();
        h2.insert(0, 3);
        dbg!(h[&0]);
        dbg!(h2[&0]);
    }
}
