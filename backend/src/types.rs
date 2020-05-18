#![allow(dead_code)]
#![macro_use]
pub use super::code_gen::{Function, Package};
use super::opcode::OpIndex;
pub use super::values::{ChannelVal, InterfaceVal, MapVal, SliceVal, StringVal, StructVal};
pub use super::vm::ClosureVal;
use slotmap::{new_key_type, DenseSlotMap};
use std::cmp::Ordering;
use std::collections::HashMap;
use std::fmt;

const DEFAULT_CAPACITY: usize = 128;

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
new_key_type! { pub struct TypeKey; }
new_key_type! { pub struct IterKey; }

pub type InterfaceObjs = DenseSlotMap<InterfaceKey, InterfaceVal>;
pub type ClosureObjs = DenseSlotMap<ClosureKey, ClosureVal>;
pub type StringObjs = DenseSlotMap<StringKey, StringVal>;
pub type SliceObjs = DenseSlotMap<SliceKey, SliceVal>;
pub type MapObjs = DenseSlotMap<MapKey, MapVal>;
pub type StructObjs = DenseSlotMap<StructKey, StructVal>;
pub type ChannelObjs = DenseSlotMap<ChannelKey, ChannelVal>;
pub type BoxedObjs = DenseSlotMap<BoxedKey, GosValue>;
pub type FunctionObjs = DenseSlotMap<FunctionKey, Function>;
pub type PackageObjs = DenseSlotMap<PackageKey, Package>;
pub type TypeObjs = DenseSlotMap<TypeKey, GosType>;

#[derive(Clone, Debug)]
pub struct VMObjects {
    pub interfaces: InterfaceObjs,
    pub closures: ClosureObjs,
    pub strings: StringObjs,
    pub slices: SliceObjs,
    pub maps: MapObjs,
    pub structs: StructObjs,
    pub channels: ChannelObjs,
    pub boxed: BoxedObjs,
    pub functions: FunctionObjs,
    pub packages: PackageObjs,
    pub types: TypeObjs,
    pub basic_types: HashMap<&'static str, GosValue>,
    pub default_closure_type: Option<GosValue>,
}

impl VMObjects {
    pub fn new() -> VMObjects {
        let mut objs = VMObjects {
            interfaces: new_objects!(),
            closures: new_objects!(),
            strings: new_objects!(),
            slices: new_objects!(),
            maps: new_objects!(),
            structs: new_objects!(),
            channels: new_objects!(),
            boxed: new_objects!(),
            functions: new_objects!(),
            packages: new_objects!(),
            types: new_objects!(),
            basic_types: HashMap::new(),
            default_closure_type: None,
        };
        let btype = GosType::new_bool(&mut objs);
        objs.basic_types.insert("bool", btype);
        let itype = GosType::new_int(&mut objs);
        objs.basic_types.insert("int", itype);
        let ftype = GosType::new_float64(&mut objs);
        objs.basic_types.insert("float", ftype);
        objs.basic_types.insert("float64", ftype);
        let stype = GosType::new_str(&mut objs);
        objs.basic_types.insert("string", stype);
        // default_closure_type is used by manually assembiled functions
        objs.default_closure_type = Some(GosType::new_closure(vec![], vec![], &mut objs));
        objs
    }

    pub fn basic_type(&self, name: &str) -> Option<&GosValue> {
        self.basic_types.get(name)
    }
}

pub fn new_str(strings: &mut StringObjs, s: String) -> GosValue {
    GosValue::Str(strings.insert(StringVal::with_str(s)))
}

pub fn new_slice(slices: &mut SliceObjs, len: usize, cap: usize, dval: &GosValue) -> GosValue {
    GosValue::Slice(slices.insert(SliceVal::new(len, cap, dval)))
}

pub fn with_slice_val(slices: &mut SliceObjs, val: Vec<GosValue>) -> GosValue {
    GosValue::Slice(slices.insert(SliceVal::with_data(val)))
}

pub fn new_map(objs: &mut VMObjects, default_val: GosValue) -> GosValue {
    let val = MapVal::new(unsafe { std::mem::transmute(&objs) }, default_val);
    GosValue::Map(objs.maps.insert(val))
}

pub fn new_function(
    objs: &mut VMObjects,
    package: PackageKey,
    typ: TypeKey,
    variadic: bool,
    ctor: bool,
) -> GosValue {
    let val = Function::new(objs, package, typ, variadic, ctor);
    GosValue::Function(objs.functions.insert(val))
}

pub fn new_type(types: &mut TypeObjs, t: GosType) -> GosValue {
    let key = types.insert(t);
    GosValue::Type(key)
}

// ----------------------------------------------------------------------------
// GosValue
#[derive(Clone, Copy, Debug)]
pub enum GosValue {
    Nil,
    Bool(bool),
    Int(isize),
    Float64(f64), // becasue in Go there is no "float", just float64
    Complex64(f32, f32),
    Str(StringKey), // "String" is taken
    Boxed(BoxedKey),
    Closure(ClosureKey),
    Slice(SliceKey),
    Map(MapKey),
    Interface(InterfaceKey),
    Struct(StructKey),
    Channel(ChannelKey),
    // below are not visible to users, they are "values" not "variables"
    Function(FunctionKey),
    Package(PackageKey),
    Type(TypeKey),
}

impl GosValue {
    #[inline]
    pub fn new_str(s: String, o: &mut StringObjs) -> GosValue {
        new_str(o, s)
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
        typ: TypeKey,
        variadic: bool,
        ctor: bool,
        objs: &mut VMObjects,
    ) -> GosValue {
        new_function(objs, package, typ, variadic, ctor)
    }

    #[inline]
    pub fn new_type(t: GosType, o: &mut TypeObjs) -> GosValue {
        new_type(o, t)
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
    pub fn as_slice(&self) -> &SliceKey {
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
    pub fn as_type(&self) -> &TypeKey {
        unwrap_gos_val!(Type, self)
    }

    #[inline]
    pub fn as_boxed(&self) -> &BoxedKey {
        unwrap_gos_val!(Boxed, self)
    }

    pub fn get_type_val<'a>(&self, objs: &'a VMObjects) -> &'a GosType {
        let tkey = self.as_type();
        &objs.types[*tkey]
    }

    pub fn get_type_val_mut<'a>(&self, objs: &'a mut VMObjects) -> &'a mut GosType {
        let tkey = self.as_type();
        &mut objs.types[*tkey]
    }

    pub fn eq(&self, b: &GosValue, objs: &VMObjects) -> bool {
        match (self, b) {
            // todo: not the "correct" implementation yet,
            (GosValue::Nil, GosValue::Nil) => true,
            (GosValue::Bool(x), GosValue::Bool(y)) => x == y,
            (GosValue::Int(x), GosValue::Int(y)) => x == y,
            (GosValue::Float64(x), GosValue::Float64(y)) => x == y,
            (GosValue::Complex64(xr, xi), GosValue::Complex64(yr, yi)) => xr == yr && xi == yi,
            (GosValue::Str(sa), GosValue::Str(sb)) => &objs.strings[*sa] == &objs.strings[*sb],
            _ => false,
        }
    }
    pub fn cmp(&self, b: &GosValue, objs: &VMObjects) -> Ordering {
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
            (GosValue::Str(sa), GosValue::Str(sb)) => objs.strings[*sa].cmp(&objs.strings[*sb]),
            (GosValue::Slice(_), GosValue::Slice(_)) => unreachable!(),
            _ => unimplemented!(),
        }
    }
}

// ----------------------------------------------------------------------------
// GosType
#[derive(Clone, Debug)]
pub enum GosTypeData {
    None,
    Closure(Vec<GosValue>, Vec<GosValue>),
    Slice(GosValue),
    Map(GosValue, GosValue),
    Interface(Vec<GosValue>),
    // the hasmap maps field name to field index, negative index indicates
    // it's a method
    Struct(Vec<GosValue>, HashMap<String, OpIndex>),
    Channel(GosValue),
    Boxed(GosValue),
    Variadic(GosValue),
}

#[derive(Clone, Debug)]
pub struct GosType {
    pub zero_val: GosValue,
    pub data: GosTypeData,
}

impl GosType {
    pub fn new_bool(objs: &mut VMObjects) -> GosValue {
        let typ = GosType {
            zero_val: GosValue::Bool(false),
            data: GosTypeData::None,
        };
        GosValue::Type(objs.types.insert(typ))
    }

    pub fn new_int(objs: &mut VMObjects) -> GosValue {
        let typ = GosType {
            zero_val: GosValue::Int(0),
            data: GosTypeData::None,
        };
        GosValue::Type(objs.types.insert(typ))
    }

    pub fn new_float64(objs: &mut VMObjects) -> GosValue {
        let typ = GosType {
            zero_val: GosValue::Float64(0.0),
            data: GosTypeData::None,
        };
        GosValue::Type(objs.types.insert(typ))
    }

    pub fn new_str(objs: &mut VMObjects) -> GosValue {
        let typ = GosType {
            zero_val: GosValue::Str(null_key!()),
            data: GosTypeData::None,
        };
        GosValue::Type(objs.types.insert(typ))
    }

    pub fn new_closure(
        params: Vec<GosValue>,
        results: Vec<GosValue>,
        objs: &mut VMObjects,
    ) -> GosValue {
        let typ = GosType {
            zero_val: GosValue::Closure(null_key!()),
            data: GosTypeData::Closure(params, results),
        };
        GosValue::Type(objs.types.insert(typ))
    }

    pub fn new_slice(vtype: GosValue, objs: &mut VMObjects) -> GosValue {
        let typ = GosType {
            zero_val: GosValue::Slice(null_key!()),
            data: GosTypeData::Slice(vtype),
        };
        GosValue::Type(objs.types.insert(typ))
    }

    pub fn new_map(ktype: GosValue, vtype: GosValue, objs: &mut VMObjects) -> GosValue {
        let typ = GosType {
            zero_val: GosValue::Map(null_key!()),
            data: GosTypeData::Map(ktype, vtype),
        };
        GosValue::Type(objs.types.insert(typ))
    }

    pub fn new_interface(fields: Vec<GosValue>, objs: &mut VMObjects) -> GosValue {
        let typ = GosType {
            zero_val: GosValue::Interface(null_key!()),
            data: GosTypeData::Interface(fields),
        };
        GosValue::Type(objs.types.insert(typ))
    }

    pub fn new_struct(
        fields: Vec<GosValue>,
        meta: HashMap<String, OpIndex>,
        objs: &mut VMObjects,
    ) -> GosValue {
        let field_zeros: Vec<GosValue> = fields
            .iter()
            .map(|x| x.get_type_val(objs).zero_val().clone())
            .collect();
        let struct_val = StructVal {
            dark: false,
            typ: null_key!(),
            fields: field_zeros,
        };
        let struct_key = objs.structs.insert(struct_val);
        let typ = GosType {
            zero_val: GosValue::Struct(struct_key),
            data: GosTypeData::Struct(fields, meta),
        };
        let typ_key = objs.types.insert(typ);
        objs.structs[struct_key].typ = typ_key;
        GosValue::Type(typ_key)
    }

    pub fn new_channel(vtype: GosValue, objs: &mut VMObjects) -> GosValue {
        let typ = GosType {
            zero_val: GosValue::Channel(null_key!()),
            data: GosTypeData::Channel(vtype),
        };
        GosValue::Type(objs.types.insert(typ))
    }

    pub fn new_boxed(inner: GosValue, objs: &mut VMObjects) -> GosValue {
        let typ = GosType {
            zero_val: GosValue::Boxed(null_key!()),
            data: GosTypeData::Boxed(inner),
        };
        GosValue::Type(objs.types.insert(typ))
    }

    pub fn new_variadic(elt: GosValue, objs: &mut VMObjects) -> GosValue {
        let typ = GosType {
            // what does zero_val for this mean exactly?
            zero_val: GosValue::Slice(null_key!()),
            data: GosTypeData::Variadic(elt),
        };
        GosValue::Type(objs.types.insert(typ))
    }

    pub fn zero_val(&self) -> &GosValue {
        &self.zero_val
    }

    pub fn closure_type_data(&self) -> (&Vec<GosValue>, &Vec<GosValue>) {
        if let GosTypeData::Closure(params, returns) = &self.data {
            (params, returns)
        } else {
            unreachable!();
        }
    }

    pub fn add_struct_member(&mut self, name: String, val: GosValue) {
        if let GosTypeData::Struct(members, mapping) = &mut self.data {
            members.push(val);
            mapping.insert(name, members.len() as OpIndex - 1);
        } else {
            unreachable!();
        }
    }

    pub fn get_struct_member(&self, index: OpIndex) -> &GosValue {
        if let GosTypeData::Struct(members, _) = &self.data {
            &members[index as usize]
        } else {
            unreachable!()
        }
    }

    pub fn get_closure_params(&self) -> &Vec<GosValue> {
        if let GosTypeData::Closure(params, _) = &self.data {
            &params
        } else {
            unreachable!()
        }
    }

    pub fn is_variadic(&self) -> bool {
        if let GosTypeData::Variadic(_) = &self.data {
            true
        } else {
            false
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
            GosValue::Str(k) => self.objs.strings[*k].fmt(f),
            GosValue::Closure(k) => self.objs.closures[*k].fmt(f),
            GosValue::Slice(k) => self.objs.slices[*k].fmt(f),
            GosValue::Map(k) => self.objs.maps[*k].fmt(f),
            GosValue::Interface(k) => self.objs.interfaces[*k].fmt(f),
            GosValue::Struct(k) => self.objs.structs[*k].fmt(f),
            GosValue::Channel(k) => self.objs.channels[*k].fmt(f),
            GosValue::Boxed(k) => self.objs.boxed[*k].fmt(f),
            GosValue::Function(k) => self.objs.functions[*k].fmt(f),
            GosValue::Package(k) => self.objs.packages[*k].fmt(f),
            GosValue::Type(k) => self.objs.types[*k].fmt(f),
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_gosvalue_debug() {
        let mut o = VMObjects::new();
        let s = new_str(&mut o.strings, "test_string".to_string());
        let slice = new_slice(&mut o.slices, 10, 10, &GosValue::Int(0));
        dbg!(
            GosValueDebug::new(&s, &o),
            GosValueDebug::new(&GosValue::Nil, &o),
            GosValueDebug::new(&GosValue::Int(1), &o),
            GosValueDebug::new(&slice, &o),
        );
    }

    /*
        trait Float {
        type Bits: Hash;
        fn float_is_nan(&self) -> bool;
        fn float_to_bits(&self) -> Self::Bits;
    }

    impl Float for f32 {
        type Bits = u32;
        fn float_is_nan(&self) -> bool {
            self.is_nan()
        }
        fn float_to_bits(&self) -> u32 {
            self.to_bits()
        }
    }

    impl Float for f64 {
        type Bits = u64;
        fn float_is_nan(&self) -> bool {
            self.is_nan()
        }
        fn float_to_bits(&self) -> u64 {
            self.to_bits()
        }
    }

    fn float_eq<T: Float + PartialEq>(x: &T, y: &T) -> bool {
        match (x, y) {
            (a, _) if a.float_is_nan() => false,
            (_, b) if b.float_is_nan() => false,
            (a, b) => a == b,
        }
    }

    fn float_hash<T: Float, H: Hasher>(f: &T, state: &mut H) {
        match f {
            x if x.float_is_nan() => {
                "NAN".hash(state);
            }
            x => {
                x.float_to_bits().hash(state);
            }
        }
    }

        */

    #[test]
    fn test_types() {
        let mut o = VMObjects::new();
        let _t1: Vec<GosValue> = vec![
            GosValue::new_str("Norway".to_string(), &mut o.strings),
            GosValue::Int(100),
            GosValue::new_str("Denmark".to_string(), &mut o.strings),
            GosValue::Int(10),
        ];

        let _t2: Vec<GosValue> = vec![
            GosValue::new_str("Norway".to_string(), &mut o.strings),
            GosValue::Int(100),
            GosValue::new_str("Denmark".to_string(), &mut o.strings),
            GosValue::Int(10),
        ];
        /*let a = GosValue::new_slice(t1);
        let b = GosValue::new_slice(t2);
        let c = GosValue::new_slice(vec![a.clone(), b.clone(), GosValue::Int(999)]);
        let d = GosValue::new_slice(vec![a.clone(), b.clone(), GosValue::Int(999)]);

        //let c = b.clone();

        println!("types {}-{}-{}\n",
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
