#![allow(dead_code)]
use super::instruction::{ValueType, COPYABLE_END};
pub use super::objects::*;
use std::cell::RefCell;
use std::cmp::Ordering;
use std::fmt;
use std::hash::{Hash, Hasher};
use std::rc::Rc;

// ----------------------------------------------------------------------------
// GosValue
#[derive(Debug)]
pub enum GosValue {
    Bool(bool),
    Int(isize),
    Float64(f64), // becasue in Go there is no "float", just float64
    Complex64(f32, f32),

    // the 3 below are not visible to users, they are "values" not "variables"
    // they are static data, don't use Rc for better performance
    Function(FunctionKey),
    Package(PackageKey),
    Metadata(MetadataKey),

    Str(Rc<StringVal>), // "String" is taken
    Boxed(Rc<RefCell<GosValue>>),
    Closure(Rc<RefCell<ClosureVal>>),
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
        variadic: bool,
        ctor: bool,
        objs: &mut VMObjects,
    ) -> GosValue {
        let val = FunctionVal::new(package, meta, variadic, ctor);
        GosValue::Function(objs.functions.insert(val))
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
    pub fn as_closure(&self) -> &Rc<RefCell<ClosureVal>> {
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
    pub fn meta_get_value32_type(&self, metas: &MetadataObjs) -> ValueType {
        metas[*self.as_meta()].get_value32_type(metas)
    }

    pub fn get_v32_type(&self) -> ValueType {
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
}

impl Clone for GosValue {
    #[inline]
    fn clone(&self) -> Self {
        let (v, t) = GosValue64::from_v128_leak(self);
        v.into_v128_unleak(t)
    }
}

impl Eq for GosValue {}

impl PartialEq for GosValue {
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
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for GosValue {
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
union Value32Union {
    unil: (),
    ubool: bool,
    uint: isize,
    ufloat64: f64,
    ucomplex64: (f32, f32),
    umetadata: MetadataKey,
    ufunction: FunctionKey,
    upackage: PackageKey,
    ustr: *const StringVal,
    uboxed: *const RefCell<GosValue>,
    uclosure: *const RefCell<ClosureVal>,
    uslice: *const SliceVal,
    umap: *const MapVal,
    uinterface: *const RefCell<InterfaceVal>,
    ustruct: *const RefCell<StructVal>,
    uchannel: *const RefCell<ChannelVal>,
}

/// GosValue64 is a 64bit struct for VM stack to get better performance, when converting
/// to GosValue64, the type info is lost, Opcode is responsible for providing type info
/// when converting back to GosValue
pub struct GosValue64 {
    data: Value32Union,
    pub debug_type: ValueType, // to be removed in release build
}

impl GosValue64 {
    #[inline]
    pub fn from_v128_leak(v: &GosValue) -> (GosValue64, ValueType) {
        let (data, typ) = match v {
            GosValue::Bool(b) => (Value32Union { ubool: *b }, ValueType::Bool),
            GosValue::Int(i) => (Value32Union { uint: *i }, ValueType::Int),
            GosValue::Float64(f) => (Value32Union { ufloat64: *f }, ValueType::Float64),
            GosValue::Complex64(f1, f2) => (
                Value32Union {
                    ucomplex64: (*f1, *f2),
                },
                ValueType::Complex64,
            ),
            GosValue::Str(s) => (
                Value32Union {
                    ustr: Rc::into_raw(s.clone()),
                },
                ValueType::Str,
            ),
            GosValue::Boxed(b) => (
                Value32Union {
                    uboxed: Rc::into_raw(b.clone()),
                },
                ValueType::Boxed,
            ),
            GosValue::Closure(c) => (
                Value32Union {
                    uclosure: Rc::into_raw(c.clone()),
                },
                ValueType::Closure,
            ),
            GosValue::Slice(s) => (
                Value32Union {
                    uslice: Rc::into_raw(s.clone()),
                },
                ValueType::Slice,
            ),
            GosValue::Map(m) => (
                Value32Union {
                    umap: Rc::into_raw(m.clone()),
                },
                ValueType::Map,
            ),
            GosValue::Interface(i) => (
                Value32Union {
                    uinterface: Rc::into_raw(i.clone()),
                },
                ValueType::Interface,
            ),
            GosValue::Struct(s) => (
                Value32Union {
                    ustruct: Rc::into_raw(s.clone()),
                },
                ValueType::Struct,
            ),
            GosValue::Channel(c) => (
                Value32Union {
                    uchannel: Rc::into_raw(c.clone()),
                },
                ValueType::Channel,
            ),
            GosValue::Function(k) => (Value32Union { ufunction: *k }, ValueType::Function),
            GosValue::Package(k) => (Value32Union { upackage: *k }, ValueType::Package),
            GosValue::Metadata(k) => (Value32Union { umetadata: *k }, ValueType::Metadata),
        };
        (
            GosValue64 {
                data: data,
                debug_type: typ,
            },
            typ,
        )
    }

    #[inline]
    pub fn nil() -> GosValue64 {
        GosValue64 {
            data: Value32Union { unil: () },
            debug_type: ValueType::Nil,
        }
    }

    #[inline]
    pub fn from_bool(b: bool) -> GosValue64 {
        GosValue64 {
            data: Value32Union { ubool: b },
            debug_type: ValueType::Bool,
        }
    }

    #[inline]
    pub fn from_int(i: isize) -> GosValue64 {
        GosValue64 {
            data: Value32Union { uint: i },
            debug_type: ValueType::Int,
        }
    }

    #[inline]
    pub fn from_float64(f: f64) -> GosValue64 {
        GosValue64 {
            data: Value32Union { ufloat64: f },
            debug_type: ValueType::Float64,
        }
    }

    #[inline]
    pub fn from_complex64(r: f32, i: f32) -> GosValue64 {
        GosValue64 {
            data: Value32Union { ucomplex64: (r, i) },
            debug_type: ValueType::Complex64,
        }
    }

    /// returns GosValue and increases RC
    #[inline]
    pub fn get_v128(&self, t: ValueType) -> GosValue {
        debug_assert!(t == self.debug_type);
        unsafe {
            match t {
                ValueType::Bool => GosValue::Bool(self.data.ubool),
                ValueType::Int => GosValue::Int(self.data.uint),
                ValueType::Float64 => GosValue::Float64(self.data.ufloat64),
                ValueType::Complex64 => {
                    GosValue::Complex64(self.data.ucomplex64.0, self.data.ucomplex64.1)
                }
                ValueType::Str => GosValue::Str(GosValue64::get_rc_from_ptr(self.data.ustr)),
                ValueType::Boxed => GosValue::Boxed(GosValue64::get_rc_from_ptr(self.data.uboxed)),
                ValueType::Closure => {
                    GosValue::Closure(GosValue64::get_rc_from_ptr(self.data.uclosure))
                }
                ValueType::Slice => GosValue::Slice(GosValue64::get_rc_from_ptr(self.data.uslice)),
                ValueType::Map => GosValue::Map(GosValue64::get_rc_from_ptr(self.data.umap)),
                ValueType::Interface => {
                    GosValue::Interface(GosValue64::get_rc_from_ptr(self.data.uinterface))
                }
                ValueType::Struct => {
                    GosValue::Struct(GosValue64::get_rc_from_ptr(self.data.ustruct))
                }
                ValueType::Channel => {
                    GosValue::Channel(GosValue64::get_rc_from_ptr(self.data.uchannel))
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
        debug_assert_eq!(self.debug_type, ValueType::Bool);
        unsafe { self.data.ubool }
    }

    #[inline]
    pub fn get_int(&self) -> isize {
        debug_assert_eq!(self.debug_type, ValueType::Int);
        unsafe { self.data.uint }
    }

    #[inline]
    pub fn get_float64(&self) -> f64 {
        debug_assert_eq!(self.debug_type, ValueType::Float64);
        unsafe { self.data.ufloat64 }
    }

    #[inline]
    pub fn get_complex64(&self) -> (f32, f32) {
        debug_assert_eq!(self.debug_type, ValueType::Complex64);
        unsafe { self.data.ucomplex64 }
    }

    /// returns GosValue without increasing RC
    #[inline]
    pub fn into_v128_unleak(&self, t: ValueType) -> GosValue {
        //dbg!(t, self.debug_type);
        debug_assert!(t == self.debug_type);
        unsafe {
            match t {
                ValueType::Bool => GosValue::Bool(self.data.ubool),
                ValueType::Int => GosValue::Int(self.data.uint),
                ValueType::Float64 => GosValue::Float64(self.data.ufloat64),
                ValueType::Complex64 => {
                    GosValue::Complex64(self.data.ucomplex64.0, self.data.ucomplex64.1)
                }
                ValueType::Str => GosValue::Str(Rc::from_raw(self.data.ustr)),
                ValueType::Boxed => GosValue::Boxed(Rc::from_raw(self.data.uboxed)),
                ValueType::Closure => GosValue::Closure(Rc::from_raw(self.data.uclosure)),
                ValueType::Slice => GosValue::Slice(Rc::from_raw(self.data.uslice)),
                ValueType::Map => GosValue::Map(Rc::from_raw(self.data.umap)),
                ValueType::Interface => GosValue::Interface(Rc::from_raw(self.data.uinterface)),
                ValueType::Struct => GosValue::Struct(Rc::from_raw(self.data.ustruct)),
                ValueType::Channel => GosValue::Channel(Rc::from_raw(self.data.uchannel)),
                ValueType::Function => GosValue::Function(self.data.ufunction),
                ValueType::Package => GosValue::Package(self.data.upackage),
                ValueType::Metadata => GosValue::Metadata(self.data.umetadata),
                _ => unreachable!(),
            }
        }
    }

    #[inline]
    pub fn clone(&self, t: ValueType) -> GosValue64 {
        //let t = self.debug_type;
        let data = if t <= COPYABLE_END {
            self.data
        } else {
            unsafe {
                match t {
                    ValueType::Str => Value32Union {
                        ustr: GosValue64::clone_ptr(self.data.ustr),
                    },
                    ValueType::Boxed => Value32Union {
                        uboxed: GosValue64::clone_ptr(self.data.uboxed),
                    },
                    ValueType::Closure => Value32Union {
                        uclosure: GosValue64::clone_ptr(self.data.uclosure),
                    },
                    ValueType::Slice => Value32Union {
                        uslice: GosValue64::clone_ptr(self.data.uslice),
                    },
                    ValueType::Map => Value32Union {
                        umap: GosValue64::clone_ptr(self.data.umap),
                    },
                    ValueType::Interface => Value32Union {
                        uinterface: GosValue64::clone_ptr(self.data.uinterface),
                    },
                    ValueType::Struct => Value32Union {
                        ustruct: GosValue64::clone_ptr(self.data.ustruct),
                    },
                    ValueType::Channel => Value32Union {
                        uchannel: GosValue64::clone_ptr(self.data.uchannel),
                    },
                    _ => unreachable!(),
                }
            }
        };
        GosValue64 {
            data: data,
            debug_type: self.debug_type,
        }
    }
    /*
        #[inline]
        pub fn semantic_copy(&self, t: ValueType) -> GosValue64 {
            match self {
                GosValue::Bool(b) => GosValue::Bool(*b),
                GosValue::Int(i) => GosValue::Int(*i),
                GosValue::Float64(f) => GosValue::Float64(*f),
                GosValue::Complex64(f1, f2) => GosValue::Complex64(*f1, *f2),
                GosValue::Str(s) => GosValue::Str(Rc::clone(s)),
                GosValue::Boxed(b) => GosValue::Boxed(Rc::clone(b)),
                GosValue::Closure(c) => GosValue::Closure(Rc::clone(c)),
                GosValue::Slice(s) => GosValue::Slice(Rc::new((**s).clone())),
                GosValue::Map(m) => GosValue::Map(Rc::new((**m).clone())),
                GosValue::Interface(i) => GosValue::Interface(Rc::clone(i)),
                GosValue::Struct(s) => GosValue::Struct(Rc::new((**s).clone())),
                GosValue::Channel(c) => GosValue::Channel(Rc::clone(c)),
                GosValue::Function(k) => GosValue::Function(*k),
                GosValue::Package(k) => GosValue::Package(*k),
                GosValue::Metadata(m) => GosValue::Metadata(*m),
            }
        }
    */
    /// get_rc_from_ptr returns a Rc from raw pointer, without decreasing the ref counter
    #[inline]
    fn get_rc_from_ptr<T>(ptr: *const T) -> Rc<T> {
        unsafe {
            let rc = Rc::from_raw(ptr);
            let rc2 = Rc::clone(&rc);
            Rc::into_raw(rc);
            rc2
        }
    }

    #[inline]
    fn clone_ptr<T>(ptr: *const T) -> *const T {
        Rc::into_raw(GosValue64::get_rc_from_ptr(ptr))
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

        let mut h: HashMap<isize, isize> = HashMap::new();
        h.insert(0, 1);
        let mut h2 = h.clone();
        h2.insert(0, 3);
        dbg!(h[&0]);
        dbg!(h2[&0]);
    }
}
