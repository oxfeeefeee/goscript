#![allow(dead_code)]
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
    Nil,
    Bool(bool),
    Int(isize),
    Float64(f64), // becasue in Go there is no "float", just float64
    Complex64(f32, f32),
    Str(Rc<StringVal>), // "String" is taken
    Boxed(Rc<RefCell<GosValue>>),
    Closure(Rc<RefCell<ClosureVal>>),
    Slice(Rc<SliceVal>),
    Map(Rc<MapVal>),
    Interface(Rc<RefCell<InterfaceVal>>),
    Struct(Rc<RefCell<StructVal>>),
    Channel(Rc<RefCell<ChannelVal>>),
    // below are not visible to users, they are "values" not "variables"
    // they are static data, don't use Rc for better performance
    Meta(MetadataKey),
    Function(FunctionKey),
    Package(PackageKey),
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
    pub fn new_meta(t: MetadataVal, metas: &mut MetadataObjs) -> GosValue {
        GosValue::Meta(metas.insert(t))
    }

    #[inline]
    pub fn is_nil(&self) -> bool {
        if let GosValue::Nil = self {
            true
        } else {
            false
        }
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
    pub fn as_slice(&self) -> &Rc<SliceVal> {
        unwrap_gos_val!(Slice, self)
    }

    #[inline]
    pub fn as_map(&self) -> &Rc<MapVal> {
        unwrap_gos_val!(Map, self)
    }

    #[inline]
    pub fn as_function(&self) -> &FunctionKey {
        unwrap_gos_val!(Function, self)
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
        unwrap_gos_val!(Meta, self)
    }

    #[inline]
    pub fn as_boxed(&self) -> &Rc<RefCell<GosValue>> {
        unwrap_gos_val!(Boxed, self)
    }
}

impl Clone for GosValue {
    fn clone(&self) -> Self {
        match self {
            GosValue::Nil => GosValue::Nil,
            GosValue::Bool(b) => GosValue::Bool(*b),
            GosValue::Int(i) => GosValue::Int(*i),
            GosValue::Float64(f) => GosValue::Float64(*f),
            GosValue::Complex64(f1, f2) => GosValue::Complex64(*f1, *f2),
            GosValue::Str(s) => GosValue::Str(Rc::clone(s)),
            GosValue::Boxed(b) => GosValue::Boxed(Rc::clone(b)),
            GosValue::Closure(c) => GosValue::Closure(Rc::clone(c)),
            GosValue::Slice(s) => GosValue::Slice(Rc::clone(s)),
            GosValue::Map(m) => GosValue::Map(Rc::clone(m)),
            GosValue::Interface(i) => GosValue::Interface(Rc::clone(i)),
            GosValue::Struct(s) => GosValue::Struct(Rc::clone(s)),
            GosValue::Channel(c) => GosValue::Channel(Rc::clone(c)),
            GosValue::Function(k) => GosValue::Function(*k),
            GosValue::Package(k) => GosValue::Package(*k),
            GosValue::Meta(m) => GosValue::Meta(*m),
        }
    }
}

impl Eq for GosValue {}

impl PartialEq for GosValue {
    fn eq(&self, b: &GosValue) -> bool {
        match (self, b) {
            // todo: not the "correct" implementation yet,
            (GosValue::Nil, GosValue::Nil) => true,
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
            (GosValue::Nil, GosValue::Nil) => Ordering::Equal,
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
            GosValue::Nil => 0.hash(state),
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
            GosValue::Nil
            | GosValue::Bool(_)
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
            GosValue::Meta(k) => self.objs.metas[*k].fmt(f),
            //_ => unreachable!(),
        }
    }
}

// ----------------------------------------------------------------------------
// GosValue32

pub enum Value32Type {
    Bool,
    Int,
    Float64,
    Complex64,
    Str,
    Boxed,
    Closure,
    Slice,
    Map,
    Interface,
    Struct,
    Channel,
    Function,
    Package,
    Meta,
}

union Value32Union {
    ubool: bool,
    uint: isize,
    ufloat64: f64,
    ucomplex64: (f32, f32),
    ustr: *const String,
    uboxed: *const RefCell<GosValue>,
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
            GosValueDebug::new(&GosValue::Nil, &o),
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
