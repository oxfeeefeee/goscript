#![allow(dead_code)]
#![macro_use]
pub use super::ds::{
    ChannelVal, ClosureVal, FunctionVal, InterfaceVal, MapVal, PackageVal, SliceVal, StringVal,
    StructVal,
};
use super::opcode::OpIndex;
use slotmap::{new_key_type, DenseSlotMap};
use std::cell::{Ref, RefCell, RefMut};
use std::cmp::Ordering;
use std::collections::HashMap;
use std::fmt;
use std::hash::{Hash, Hasher};
use std::rc::{Rc, Weak};

const DEFAULT_CAPACITY: usize = 128;

#[macro_export]
macro_rules! null_key {
    () => {
        slotmap::Key::null()
    };
}

macro_rules! new_objects {
    () => {
        DenseSlotMap::with_capacity_and_key(DEFAULT_CAPACITY)
    };
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

new_key_type! { pub struct FunctionKey; }
new_key_type! { pub struct PackageKey; }

pub type InterfaceObjs = Vec<Weak<RefCell<InterfaceVal>>>;
pub type ClosureObjs = Vec<Weak<RefCell<ClosureVal>>>;
pub type SliceObjs = Vec<Weak<SliceVal>>;
pub type MapObjs = Vec<Weak<MapVal>>;
pub type StructObjs = Vec<Weak<RefCell<StructVal>>>;
pub type ChannelObjs = Vec<Weak<RefCell<ChannelVal>>>;
pub type BoxedObjs = Vec<Weak<GosValue>>;
pub type FunctionObjs = DenseSlotMap<FunctionKey, FunctionVal>;
pub type PackageObjs = DenseSlotMap<PackageKey, PackageVal>;

#[derive(Clone, Debug)]
pub struct VMObjects {
    pub interfaces: InterfaceObjs,
    pub closures: ClosureObjs,
    pub slices: SliceObjs,
    pub maps: MapObjs,
    pub structs: StructObjs,
    pub channels: ChannelObjs,
    pub boxed: BoxedObjs,
    pub functions: FunctionObjs,
    pub packages: PackageObjs,
    pub basic_types: HashMap<&'static str, GosValue>,
    pub default_sig_meta: Option<GosValue>,
    pub str_zero_val: Rc<StringVal>,
    pub slice_zero_val: Rc<SliceVal>,
    pub map_zero_val: Rc<MapVal>,
}

impl VMObjects {
    pub fn new() -> VMObjects {
        let str_zero_val = Rc::new(StringVal::with_str("".to_string()));
        let slice_zero_val = Rc::new(SliceVal::new(0, 0, &GosValue::Nil));
        let map_zero_val = Rc::new(MapVal::new(GosValue::Nil));
        let mut objs = VMObjects {
            interfaces: vec![],
            closures: vec![],
            slices: vec![],
            maps: vec![],
            structs: vec![],
            channels: vec![],
            boxed: vec![],
            functions: new_objects!(),
            packages: new_objects!(),
            basic_types: HashMap::new(),
            default_sig_meta: None,
            str_zero_val: str_zero_val,
            slice_zero_val: slice_zero_val,
            map_zero_val: map_zero_val,
        };
        let btype = Metadata::new_bool();
        objs.basic_types.insert("bool", btype);
        let itype = Metadata::new_int();
        objs.basic_types.insert("int", itype);
        let ftype = Metadata::new_float64();
        objs.basic_types.insert("float", ftype.clone());
        objs.basic_types.insert("float64", ftype);
        let stype = Metadata::new_str(&mut objs);
        objs.basic_types.insert("string", stype);
        // default_sig_meta is used by manually assembiled functions
        objs.default_sig_meta = Some(Metadata::new_sig(None, vec![], vec![], false));
        objs
    }

    pub fn basic_type(&self, name: &str) -> Option<&GosValue> {
        self.basic_types.get(name)
    }
}

pub fn new_str(s: String) -> GosValue {
    GosValue::Str(Rc::new(StringVal::with_str(s)))
}

pub fn new_slice(slices: &mut SliceObjs, len: usize, cap: usize, dval: &GosValue) -> GosValue {
    let s = Rc::new(SliceVal::new(len, cap, dval));
    slices.push(Rc::downgrade(&s));
    GosValue::Slice(s)
}

pub fn with_slice_val(slices: &mut SliceObjs, val: Vec<GosValue>) -> GosValue {
    let s = Rc::new(SliceVal::with_data(val));
    slices.push(Rc::downgrade(&s));
    GosValue::Slice(s)
}

pub fn new_map(maps: &mut MapObjs, default_val: GosValue) -> GosValue {
    let val = Rc::new(MapVal::new(default_val));
    maps.push(Rc::downgrade(&val));
    GosValue::Map(val)
}

pub fn new_function(
    objs: &mut VMObjects,
    package: PackageKey,
    meta: GosValue,
    variadic: bool,
    ctor: bool,
) -> GosValue {
    let val = FunctionVal::new(objs, package, meta, variadic, ctor);
    GosValue::Function(objs.functions.insert(val))
}

pub fn new_meta(t: Metadata) -> GosValue {
    GosValue::Meta(Rc::new(t))
}

// ----------------------------------------------------------------------------
// GosValue
#[derive(Clone, Debug)]
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
    Meta(Rc<Metadata>),
    // Functions and Packages are static data, don't use Rc for better performance
    Function(FunctionKey),
    Package(PackageKey),
}

impl GosValue {
    #[inline]
    pub fn new_str(s: String) -> GosValue {
        new_str(s)
    }

    #[inline]
    pub fn new_boxed(v: GosValue) -> GosValue {
        GosValue::Boxed(Rc::new(RefCell::new(v)))
    }

    #[inline]
    pub fn new_slice(len: usize, cap: usize, dval: &GosValue, objs: &mut SliceObjs) -> GosValue {
        new_slice(objs, len, cap, dval)
    }

    #[inline]
    pub fn with_slice_val(val: Vec<GosValue>, objs: &mut SliceObjs) -> GosValue {
        with_slice_val(objs, val)
    }

    #[inline]
    pub fn new_map(default: GosValue, objs: &mut MapObjs) -> GosValue {
        new_map(objs, default)
    }

    #[inline]
    pub fn new_function(
        package: PackageKey,
        meta: GosValue,
        variadic: bool,
        ctor: bool,
        objs: &mut VMObjects,
    ) -> GosValue {
        new_function(objs, package, meta, variadic, ctor)
    }

    #[inline]
    pub fn new_meta(t: Metadata) -> GosValue {
        new_meta(t)
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
    pub fn as_meta(&self) -> &Rc<Metadata> {
        unwrap_gos_val!(Meta, self)
    }

    #[inline]
    pub fn as_boxed(&self) -> &Rc<RefCell<GosValue>> {
        unwrap_gos_val!(Boxed, self)
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

// ----------------------------------------------------------------------------
// Metadata
#[derive(Clone, Debug)]
pub struct SigMetadata {
    pub recv: Option<GosValue>,
    pub params: Vec<GosValue>,
    pub results: Vec<GosValue>,
    pub variadic: bool,
}

#[derive(Clone, Debug)]
pub enum MetadataType {
    None,
    Signature(SigMetadata),
    Slice(GosValue),
    Map(GosValue, GosValue),
    Interface(Vec<GosValue>),
    Struct(Vec<GosValue>, HashMap<String, OpIndex>),
    Channel(GosValue),
    Boxed(GosValue),
    Named(GosValue, Vec<String>), //(base_type, methods)
}

impl MetadataType {
    pub fn sig_metadata(&self) -> &SigMetadata {
        match self {
            MetadataType::Signature(stdata) => stdata,
            _ => unreachable!(),
        }
    }

    pub fn add_struct_member(&mut self, name: String, val: GosValue) {
        match self {
            MetadataType::Struct(members, mapping) => {
                members.push(val);
                mapping.insert(name, members.len() as OpIndex - 1);
            }
            _ => unreachable!(),
        }
    }

    pub fn get_struct_member(&self, index: OpIndex) -> GosValue {
        match self {
            MetadataType::Struct(members, _) => members[index as usize].clone(),
            _ => unreachable!(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct Metadata {
    zero_val: GosValue,
    typ: Rc<RefCell<MetadataType>>,
}

impl Metadata {
    pub fn new_bool() -> GosValue {
        let typ = Metadata {
            zero_val: GosValue::Bool(false),
            typ: Rc::new(RefCell::new(MetadataType::None)),
        };
        GosValue::Meta(Rc::new(typ))
    }

    pub fn new_int() -> GosValue {
        let typ = Metadata {
            zero_val: GosValue::Int(0),
            typ: Rc::new(RefCell::new(MetadataType::None)),
        };
        GosValue::Meta(Rc::new(typ))
    }

    pub fn new_float64() -> GosValue {
        let typ = Metadata {
            zero_val: GosValue::Float64(0.0),
            typ: Rc::new(RefCell::new(MetadataType::None)),
        };
        GosValue::Meta(Rc::new(typ))
    }

    pub fn new_str(objs: &VMObjects) -> GosValue {
        let typ = Metadata {
            zero_val: GosValue::Str(objs.str_zero_val.clone()),
            typ: Rc::new(RefCell::new(MetadataType::None)),
        };
        GosValue::Meta(Rc::new(typ))
    }

    pub fn new_sig(
        recv: Option<GosValue>,
        params: Vec<GosValue>,
        results: Vec<GosValue>,
        variadic: bool,
    ) -> GosValue {
        let typ = Metadata {
            zero_val: GosValue::Closure(Rc::new(RefCell::new(ClosureVal::new(
                null_key!(),
                vec![],
            )))),
            typ: Rc::new(RefCell::new(MetadataType::Signature(SigMetadata {
                recv: recv,
                params: params,
                results: results,
                variadic: variadic,
            }))),
        };
        GosValue::Meta(Rc::new(typ))
    }

    pub fn new_slice(vtype: GosValue, objs: &mut VMObjects) -> GosValue {
        let typ = Metadata {
            zero_val: GosValue::Slice(objs.slice_zero_val.clone()),
            typ: Rc::new(RefCell::new(MetadataType::Slice(vtype))),
        };
        GosValue::Meta(Rc::new(typ))
    }

    pub fn new_map(ktype: GosValue, vtype: GosValue, objs: &mut VMObjects) -> GosValue {
        let typ = Metadata {
            zero_val: GosValue::Map(objs.map_zero_val.clone()),
            typ: Rc::new(RefCell::new(MetadataType::Map(ktype, vtype))),
        };
        GosValue::Meta(Rc::new(typ))
    }

    pub fn new_interface(fields: Vec<GosValue>) -> GosValue {
        let typ = Metadata {
            zero_val: GosValue::Interface(Rc::new(RefCell::new(InterfaceVal {
                dark: false,
                meta: GosValue::Nil,
                obj: GosValue::Nil,
                obj_meta: None,
                mapping: vec![],
            }))),
            typ: Rc::new(RefCell::new(MetadataType::Interface(fields))),
        };
        GosValue::Meta(Rc::new(typ))
    }

    pub fn new_struct(fields: Vec<GosValue>, fields_index: HashMap<String, OpIndex>) -> GosValue {
        let field_zeros: Vec<GosValue> = fields
            .iter()
            .map(|x| x.as_meta().zero_val().clone())
            .collect();
        let struct_val = StructVal {
            dark: false,
            meta: GosValue::Nil,
            fields: field_zeros,
        };
        let meta = Metadata {
            zero_val: GosValue::Struct(Rc::new(RefCell::new(struct_val))),
            typ: Rc::new(RefCell::new(MetadataType::Struct(fields, fields_index))),
        };
        let m = Rc::new(meta);
        m.zero_val.as_struct().borrow_mut().meta = GosValue::Meta(m.clone());
        GosValue::Meta(m)
    }

    pub fn new_channel(vtype: GosValue) -> GosValue {
        let typ = Metadata {
            zero_val: GosValue::Channel(Rc::new(RefCell::new(ChannelVal { dark: false }))),
            typ: Rc::new(RefCell::new(MetadataType::Channel(vtype))),
        };
        GosValue::Meta(Rc::new(typ))
    }

    pub fn new_boxed(inner: GosValue) -> GosValue {
        let typ = Metadata {
            zero_val: GosValue::Boxed(Rc::new(RefCell::new(GosValue::Nil))),
            typ: Rc::new(RefCell::new(MetadataType::Boxed(inner))),
        };
        GosValue::Meta(Rc::new(typ))
    }
    pub fn zero_val(&self) -> &GosValue {
        &self.zero_val
    }

    pub fn borrow_type(&self) -> Ref<MetadataType> {
        self.typ.borrow()
    }

    pub fn borrow_type_mut(&self) -> RefMut<MetadataType> {
        self.typ.borrow_mut()
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
            GosValue::Meta(k) => k.fmt(f),
            //_ => unreachable!(),
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_gosvalue_debug() {
        let mut o = VMObjects::new();
        let s = new_str("test_string".to_string());
        let slice = new_slice(&mut o.slices, 10, 10, &GosValue::Int(0));
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
        /*let a = GosValue::new_slice(t1);
        let b = GosValue::new_slice(t2);
        let c = GosValue::new_slice(vec![a.clone(), b.clone(), GosValue::Int(999)]);
        let d = GosValue::new_slice(vec![a.clone(), b.clone(), GosValue::Int(999)]);

        //let c = b.clone();

        println!("metas {}-{}-{}\n",
            calculate_hash(&c),
            calculate_hash(&d),
            mem::size_of::<GosValue>());

        assert!((a == b) == (calculate_hash(&a) == calculate_hash(&b)));
        assert!((c == d) == (calculate_hash(&c) == calculate_hash(&d)));
        assert!(GosValue::Nil == GosValue::Nil);
        assert!(GosValue::Nil != a);
        assert!(GosValue::Int(1) == GosValue::Int(1));
        assert!(GosValue::Int(1) != GosValue::Int(2));
        assert!(GosValue::Float(1.0) == GosValue::Float(1.0));
        assert!(GosValue::Float(std::f64::NAN) == GosValue::Float(std::f64::NAN));
        assert!(GosValue::Float(std::f64::NAN) != GosValue::Float(1.0));
        assert!(GosValue::Float(0.0) == GosValue::Float(-0.0));
        assert!(GosValue::new_str("aaa".to_string()) == GosValue::new_str("aaa".to_string()));
        let s1 = GosValue::new_str("aaa".to_string());
        let s2 = s1.clone();
        assert!(s1 == s2);

        //let i = GosValue::Interface(Box::new(GosValue::Nil));
        */
    }
}
