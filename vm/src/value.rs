#![allow(dead_code)]
#![macro_use]
pub use super::ds::{
    ChannelVal, ClosureVal, FunctionVal, InterfaceVal, MapVal, PackageVal, SliceVal, StringVal,
    StructVal,
};
use super::opcode::OpIndex;
use slotmap::{new_key_type, DenseSlotMap};
use std::cmp::Ordering;
use std::collections::HashMap;
use std::fmt;
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

new_key_type! { pub struct InterfaceKey; }
new_key_type! { pub struct ClosureKey; }
new_key_type! { pub struct StringKey; }
new_key_type! { pub struct SliceKey; }
new_key_type! { pub struct MapKey; }
new_key_type! { pub struct StructKey; }
new_key_type! { pub struct ChannelKey; }
new_key_type! { pub struct BoxedKey; }
new_key_type! { pub struct FunctionKey; }
new_key_type! { pub struct PackageKey; }
new_key_type! { pub struct MetaKey; }
new_key_type! { pub struct IterKey; }

pub type InterfaceObjs = DenseSlotMap<InterfaceKey, InterfaceVal>;
pub type ClosureObjs = DenseSlotMap<ClosureKey, ClosureVal>;
pub type StringObjs = DenseSlotMap<StringKey, StringVal>;
pub type SliceObjs = Vec<Weak<SliceVal>>;
pub type MapObjs = DenseSlotMap<MapKey, MapVal>;
pub type StructObjs = DenseSlotMap<StructKey, StructVal>;
pub type ChannelObjs = DenseSlotMap<ChannelKey, ChannelVal>;
pub type BoxedObjs = DenseSlotMap<BoxedKey, GosValue>;
pub type FunctionObjs = DenseSlotMap<FunctionKey, FunctionVal>;
pub type PackageObjs = DenseSlotMap<PackageKey, PackageVal>;
pub type MetaObjs = DenseSlotMap<MetaKey, Metadata>;

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
    pub metas: MetaObjs,
    pub basic_types: HashMap<&'static str, GosValue>,
    pub default_sig_meta: Option<GosValue>,
    pub str_zero_val: Rc<StringVal>,
    pub slice_zero_val: Rc<SliceVal>,
}

impl VMObjects {
    pub fn new() -> VMObjects {
        let str_zero_val = Rc::new(StringVal::with_str("".to_string()));
        let slice_zero_val = Rc::new(SliceVal::new(0, 0, &GosValue::Nil));
        let mut objs = VMObjects {
            interfaces: new_objects!(),
            closures: new_objects!(),
            slices: vec![],
            maps: new_objects!(),
            structs: new_objects!(),
            channels: new_objects!(),
            boxed: new_objects!(),
            functions: new_objects!(),
            packages: new_objects!(),
            metas: new_objects!(),
            basic_types: HashMap::new(),
            default_sig_meta: None,
            str_zero_val: str_zero_val,
            slice_zero_val: slice_zero_val,
        };
        let btype = Metadata::new_bool(&mut objs);
        objs.basic_types.insert("bool", btype);
        let itype = Metadata::new_int(&mut objs);
        objs.basic_types.insert("int", itype);
        let ftype = Metadata::new_float64(&mut objs);
        objs.basic_types.insert("float", ftype.clone());
        objs.basic_types.insert("float64", ftype);
        let stype = Metadata::new_str(&mut objs);
        objs.basic_types.insert("string", stype);
        // default_sig_meta is used by manually assembiled functions
        objs.default_sig_meta = Some(Metadata::new_sig(None, vec![], vec![], false, &mut objs));
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

pub fn new_map(objs: &mut VMObjects, default_val: GosValue) -> GosValue {
    let val = MapVal::new(&objs, default_val);
    GosValue::Map(objs.maps.insert(val))
}

pub fn new_function(
    objs: &mut VMObjects,
    package: PackageKey,
    typ: MetaKey,
    variadic: bool,
    ctor: bool,
) -> GosValue {
    let val = FunctionVal::new(objs, package, typ, variadic, ctor);
    GosValue::Function(objs.functions.insert(val))
}

pub fn new_meta(metas: &mut MetaObjs, t: Metadata) -> GosValue {
    let key = metas.insert(t);
    GosValue::Meta(key)
}

// ----------------------------------------------------------------------------
// GosValue
#[derive(Clone, Debug, PartialEq)]
pub enum GosValue {
    Nil,
    Bool(bool),
    Int(isize),
    Float64(f64), // becasue in Go there is no "float", just float64
    Complex64(f32, f32),
    Str(Rc<StringVal>), // "String" is taken
    Boxed(BoxedKey),
    Closure(ClosureKey),
    Slice(Rc<SliceVal>),
    Map(MapKey),
    Interface(InterfaceKey),
    Struct(StructKey),
    Channel(ChannelKey),
    // below are not visible to users, they are "values" not "variables"
    Function(FunctionKey),
    Package(PackageKey),
    // use arena instead of Rc for better copying performance
    // at the expense of accessing performance
    Meta(MetaKey),
}

impl GosValue {
    #[inline]
    pub fn new_str(s: String) -> GosValue {
        new_str(s)
    }

    #[inline]
    pub fn new_slice(len: usize, cap: usize, dval: &GosValue, o: &mut SliceObjs) -> GosValue {
        new_slice(o, len, cap, dval)
    }

    #[inline]
    pub fn with_slice_val(val: Vec<GosValue>, o: &mut SliceObjs) -> GosValue {
        with_slice_val(o, val)
    }

    #[inline]
    pub fn new_map(default: GosValue, objs: &mut VMObjects) -> GosValue {
        new_map(objs, default)
    }

    #[inline]
    pub fn new_function(
        package: PackageKey,
        typ: MetaKey,
        variadic: bool,
        ctor: bool,
        objs: &mut VMObjects,
    ) -> GosValue {
        new_function(objs, package, typ, variadic, ctor)
    }

    #[inline]
    pub fn new_meta(t: Metadata, o: &mut MetaObjs) -> GosValue {
        new_meta(o, t)
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
    pub fn as_map(&self) -> &MapKey {
        unwrap_gos_val!(Map, self)
    }

    #[inline]
    pub fn as_function(&self) -> &FunctionKey {
        unwrap_gos_val!(Function, self)
    }

    #[inline]
    pub fn as_meta(&self) -> &MetaKey {
        unwrap_gos_val!(Meta, self)
    }

    #[inline]
    pub fn as_boxed(&self) -> &BoxedKey {
        unwrap_gos_val!(Boxed, self)
    }

    pub fn get_meta_val<'a>(&self, objs: &'a VMObjects) -> &'a Metadata {
        let key = self.as_meta();
        &objs.metas[*key]
    }

    pub fn get_meta_val_mut<'a>(&self, objs: &'a mut VMObjects) -> &'a mut Metadata {
        let key = self.as_meta();
        &mut objs.metas[*key]
    }

    pub fn eq(&self, b: &GosValue, _objs: &VMObjects) -> bool {
        match (self, b) {
            // todo: not the "correct" implementation yet,
            (GosValue::Nil, GosValue::Nil) => true,
            (GosValue::Bool(x), GosValue::Bool(y)) => x == y,
            (GosValue::Int(x), GosValue::Int(y)) => x == y,
            (GosValue::Float64(x), GosValue::Float64(y)) => x == y,
            (GosValue::Complex64(xr, xi), GosValue::Complex64(yr, yi)) => xr == yr && xi == yi,
            (GosValue::Str(sa), GosValue::Str(sb)) => *sa == *sb,
            _ => false,
        }
    }
    pub fn cmp(&self, b: &GosValue, _objs: &VMObjects) -> Ordering {
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
            (GosValue::Complex64(_, _), GosValue::Complex64(_, _)) => unreachable!(),
            (GosValue::Str(sa), GosValue::Str(sb)) => sa.cmp(&*sb),
            (GosValue::Slice(_), GosValue::Slice(_)) => unreachable!(),
            _ => {
                dbg!(self, b);
                unimplemented!()
            }
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

#[derive(Clone, Debug)]
pub struct Metadata {
    pub zero_val: GosValue,
    pub typ: MetadataType,
}

impl Metadata {
    pub fn new_bool(objs: &mut VMObjects) -> GosValue {
        let typ = Metadata {
            zero_val: GosValue::Bool(false),
            typ: MetadataType::None,
        };
        GosValue::Meta(objs.metas.insert(typ))
    }

    pub fn new_int(objs: &mut VMObjects) -> GosValue {
        let typ = Metadata {
            zero_val: GosValue::Int(0),
            typ: MetadataType::None,
        };
        GosValue::Meta(objs.metas.insert(typ))
    }

    pub fn new_float64(objs: &mut VMObjects) -> GosValue {
        let typ = Metadata {
            zero_val: GosValue::Float64(0.0),
            typ: MetadataType::None,
        };
        GosValue::Meta(objs.metas.insert(typ))
    }

    pub fn new_str(objs: &mut VMObjects) -> GosValue {
        let typ = Metadata {
            zero_val: GosValue::Str(objs.str_zero_val.clone()),
            typ: MetadataType::None,
        };
        GosValue::Meta(objs.metas.insert(typ))
    }

    pub fn new_sig(
        recv: Option<GosValue>,
        params: Vec<GosValue>,
        results: Vec<GosValue>,
        variadic: bool,
        objs: &mut VMObjects,
    ) -> GosValue {
        let typ = Metadata {
            zero_val: GosValue::Closure(null_key!()),
            typ: MetadataType::Signature(SigMetadata {
                recv: recv,
                params: params,
                results: results,
                variadic: variadic,
            }),
        };
        GosValue::Meta(objs.metas.insert(typ))
    }

    pub fn new_slice(vtype: GosValue, objs: &mut VMObjects) -> GosValue {
        let typ = Metadata {
            zero_val: GosValue::Slice(objs.slice_zero_val.clone()),
            typ: MetadataType::Slice(vtype),
        };
        GosValue::Meta(objs.metas.insert(typ))
    }

    pub fn new_map(ktype: GosValue, vtype: GosValue, objs: &mut VMObjects) -> GosValue {
        let typ = Metadata {
            zero_val: GosValue::Map(null_key!()),
            typ: MetadataType::Map(ktype, vtype),
        };
        GosValue::Meta(objs.metas.insert(typ))
    }

    pub fn new_interface(fields: Vec<GosValue>, objs: &mut VMObjects) -> GosValue {
        let typ = Metadata {
            zero_val: GosValue::Interface(null_key!()),
            typ: MetadataType::Interface(fields),
        };
        GosValue::Meta(objs.metas.insert(typ))
    }

    pub fn new_struct(
        fields: Vec<GosValue>,
        meta: HashMap<String, OpIndex>,
        objs: &mut VMObjects,
    ) -> GosValue {
        let field_zeros: Vec<GosValue> = fields
            .iter()
            .map(|x| x.get_meta_val(objs).zero_val().clone())
            .collect();
        let struct_val = StructVal {
            dark: false,
            meta: null_key!(),
            fields: field_zeros,
        };
        let struct_key = objs.structs.insert(struct_val);
        let meta = Metadata {
            zero_val: GosValue::Struct(struct_key),
            typ: MetadataType::Struct(fields, meta),
        };
        let meta_key = objs.metas.insert(meta);
        objs.structs[struct_key].meta = meta_key;
        GosValue::Meta(meta_key)
    }

    pub fn new_channel(vtype: GosValue, objs: &mut VMObjects) -> GosValue {
        let typ = Metadata {
            zero_val: GosValue::Channel(null_key!()),
            typ: MetadataType::Channel(vtype),
        };
        GosValue::Meta(objs.metas.insert(typ))
    }

    pub fn new_boxed(inner: GosValue, objs: &mut VMObjects) -> GosValue {
        let typ = Metadata {
            zero_val: GosValue::Boxed(null_key!()),
            typ: MetadataType::Boxed(inner),
        };
        GosValue::Meta(objs.metas.insert(typ))
    }
    pub fn zero_val(&self) -> &GosValue {
        &self.zero_val
    }

    pub fn sig_metadata(&self) -> &SigMetadata {
        if let MetadataType::Signature(stdata) = &self.typ {
            stdata
        } else {
            unreachable!();
        }
    }

    pub fn add_struct_member(&mut self, name: String, val: GosValue) {
        if let MetadataType::Struct(members, mapping) = &mut self.typ {
            members.push(val);
            mapping.insert(name, members.len() as OpIndex - 1);
        } else {
            unreachable!();
        }
    }

    pub fn get_struct_member(&self, index: OpIndex) -> &GosValue {
        if let MetadataType::Struct(members, _) = &self.typ {
            &members[index as usize]
        } else {
            unreachable!()
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
            GosValue::Closure(k) => self.objs.closures[*k].fmt(f),
            GosValue::Slice(k) => k.fmt(f),
            GosValue::Map(k) => self.objs.maps[*k].fmt(f),
            GosValue::Interface(k) => self.objs.interfaces[*k].fmt(f),
            GosValue::Struct(k) => self.objs.structs[*k].fmt(f),
            GosValue::Channel(k) => self.objs.channels[*k].fmt(f),
            GosValue::Boxed(k) => self.objs.boxed[*k].fmt(f),
            GosValue::Function(k) => self.objs.functions[*k].fmt(f),
            GosValue::Package(k) => self.objs.packages[*k].fmt(f),
            GosValue::Meta(k) => self.objs.metas[*k].fmt(f),
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
