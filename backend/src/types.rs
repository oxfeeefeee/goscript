#![allow(dead_code)]
#![macro_use]
use slotmap::{new_key_type, DenseSlotMap};
use std::cell::RefCell;
use std::collections::HashMap;
use std::fmt;
use std::hash::{Hash, Hasher};
use std::rc::Rc;

pub use super::code_gen::{Function, Package};
use super::opcode::OpIndex;
pub use super::vm::ClosureVal;

const DEFAULT_CAPACITY: usize = 128;

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
pub struct Objects {
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

impl Objects {
    pub fn new() -> Objects {
        let mut objs = Objects {
            interfaces: DenseSlotMap::with_capacity_and_key(DEFAULT_CAPACITY),
            closures: DenseSlotMap::with_capacity_and_key(DEFAULT_CAPACITY),
            strings: DenseSlotMap::with_capacity_and_key(DEFAULT_CAPACITY),
            slices: DenseSlotMap::with_capacity_and_key(DEFAULT_CAPACITY),
            maps: DenseSlotMap::with_capacity_and_key(DEFAULT_CAPACITY),
            structs: DenseSlotMap::with_capacity_and_key(DEFAULT_CAPACITY),
            channels: DenseSlotMap::with_capacity_and_key(DEFAULT_CAPACITY),
            boxed: DenseSlotMap::with_capacity_and_key(DEFAULT_CAPACITY),
            functions: DenseSlotMap::with_capacity_and_key(DEFAULT_CAPACITY),
            packages: DenseSlotMap::with_capacity_and_key(DEFAULT_CAPACITY),
            types: DenseSlotMap::with_capacity_and_key(DEFAULT_CAPACITY),
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
    GosValue::Str(strings.insert(StringVal {
        dark: false,
        data: s,
    }))
}

pub fn new_slice(slices: &mut SliceObjs, cap: usize) -> GosValue {
    let s = SliceVal {
        dark: false,
        begin: 0,
        end: 0,
        soft_cap: cap,
        vec: Rc::new(RefCell::new(Vec::with_capacity(cap))),
    };
    let key = slices.insert(s);
    GosValue::Slice(key)
}

pub fn new_map(objs: &mut Objects, default_val: GosValue) -> GosValue {
    let val = MapVal {
        dark: false,
        objs: unsafe { std::mem::transmute(&objs) },
        default_val: default_val,
        data: HashMap::new(),
    };
    let key = objs.maps.insert(val);
    GosValue::Map(key)
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
    Float64(f64),   // becasue in Go there is no "float", just float64
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

impl PartialEq for GosValue {
    fn eq(&self, other: &GosValue) -> bool {
        match (self, other) {
            (GosValue::Nil, GosValue::Nil) => true,
            (GosValue::Bool(x), GosValue::Bool(y)) => x == y,
            (GosValue::Int(x), GosValue::Int(y)) => x == y,
            (GosValue::Float64(x), GosValue::Float64(y)) => x == y,
            (GosValue::Str(x), GosValue::Str(y)) => x == y,
            (GosValue::Slice(x), GosValue::Slice(y)) => x == y,
            _ => false,
        }
    }
}

impl Default for GosValue {
    fn default() -> Self {
        GosValue::Nil
    }
}

impl Eq for GosValue {}

impl GosValue {
    pub fn new_str(s: String, o: &mut StringObjs) -> GosValue {
        new_str(o, s)
    }

    pub fn new_slice(cap: usize, o: &mut SliceObjs) -> GosValue {
        new_slice(o, cap)
    }

    pub fn new_map(default: GosValue, objs: &mut Objects) -> GosValue {
        new_map(objs, default)
    }

    pub fn new_type(t: GosType, o: &mut TypeObjs) -> GosValue {
        new_type(o, t)
    }

    #[inline]
    pub fn get_bool(&self) -> bool {
        if let GosValue::Bool(b) = self {
            *b
        } else {
            unreachable!();
        }
    }

    #[inline]
    pub fn get_int(&self) -> isize {
        if let GosValue::Int(i) = self {
            *i
        } else {
            unreachable!();
        }
    }

    #[inline]
    pub fn get_slice(&self) -> &SliceKey {
        if let GosValue::Slice(s) = self {
            s
        } else {
            unreachable!();
        }
    }

    #[inline]
    pub fn get_map(&self) -> &MapKey {
        if let GosValue::Map(m) = self {
            m
        } else {
            unreachable!();
        }
    }

    #[inline]
    pub fn get_function(&self) -> &FunctionKey {
        if let GosValue::Function(f) = self {
            f
        } else {
            unreachable!();
        }
    }

    #[inline]
    pub fn get_type(&self) -> &TypeKey {
        if let GosValue::Type(t) = self {
            t
        } else {
            unreachable!();
        }
    }

    #[inline]
    pub fn get_boxed(&self) -> &BoxedKey {
        if let GosValue::Boxed(b) = self {
            b
        } else {
            unreachable!();
        }
    }

    pub fn get_type_val<'a>(&self, objs: &'a Objects) -> &'a GosType {
        let tkey = self.get_type();
        &objs.types[*tkey]
    }
}

/// A helper struct for printing debug info of GosValue, we cannot rely on GosValue::Debug
/// because it requires 'objs' to access the inner data
pub struct GosValueDebug<'a> {
    val: &'a GosValue,
    objs: &'a Objects,
}

impl<'a> GosValueDebug<'a> {
    pub fn new(val: &'a GosValue, objs: &'a Objects) -> GosValueDebug<'a> {
        GosValueDebug {
            val: val,
            objs: objs,
        }
    }
}

impl<'a> fmt::Debug for GosValueDebug<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.val {
            GosValue::Nil | GosValue::Bool(_) | GosValue::Int(_) | GosValue::Float64(_) => {
                self.val.fmt(f)
            }
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

// ----------------------------------------------------------------------------
// GosType
#[derive(Clone, Debug)]
pub enum GosTypeData {
    None,
    Closure(Vec<GosValue>, Vec<GosValue>),
    Slice(GosValue),
    Map(GosValue, GosValue),
    Interface(Vec<GosValue>),
    // the hasmap maps field name to field index
    Struct(Vec<GosValue>, HashMap<String, usize>),
    Channel(GosValue),
    Boxed(GosValue),
}

impl GosTypeData {
    pub fn closure_type_data(&self) -> (&Vec<GosValue>, &Vec<GosValue>) {
        if let GosTypeData::Closure(params, returns) = self {
            (params, returns)
        } else {
            unreachable!();
        }
    }
}

#[derive(Clone, Debug)]
pub struct GosType {
    pub zero_val: GosValue,
    pub data: GosTypeData,
}

impl GosType {
    pub fn new_bool(objs: &mut Objects) -> GosValue {
        let typ = GosType {
            zero_val: GosValue::Bool(false),
            data: GosTypeData::None,
        };
        GosValue::Type(objs.types.insert(typ))
    }

    pub fn new_int(objs: &mut Objects) -> GosValue {
        let typ = GosType {
            zero_val: GosValue::Int(0),
            data: GosTypeData::None,
        };
        GosValue::Type(objs.types.insert(typ))
    }

    pub fn new_float64(objs: &mut Objects) -> GosValue {
        let typ = GosType {
            zero_val: GosValue::Float64(0.0),
            data: GosTypeData::None,
        };
        GosValue::Type(objs.types.insert(typ))
    }

    pub fn new_str(objs: &mut Objects) -> GosValue {
        let typ = GosType {
            zero_val: GosValue::Str(slotmap::Key::null()),
            data: GosTypeData::None,
        };
        GosValue::Type(objs.types.insert(typ))
    }

    pub fn new_closure(
        params: Vec<GosValue>,
        results: Vec<GosValue>,
        objs: &mut Objects,
    ) -> GosValue {
        let typ = GosType {
            zero_val: GosValue::Closure(slotmap::Key::null()),
            data: GosTypeData::Closure(params, results),
        };
        GosValue::Type(objs.types.insert(typ))
    }

    pub fn new_slice(vtype: GosValue, objs: &mut Objects) -> GosValue {
        let typ = GosType {
            zero_val: GosValue::Slice(slotmap::Key::null()),
            data: GosTypeData::Slice(vtype),
        };
        GosValue::Type(objs.types.insert(typ))
    }

    pub fn new_map(ktype: GosValue, vtype: GosValue, objs: &mut Objects) -> GosValue {
        let typ = GosType {
            zero_val: GosValue::Map(slotmap::Key::null()),
            data: GosTypeData::Map(ktype, vtype),
        };
        GosValue::Type(objs.types.insert(typ))
    }

    pub fn new_interface(fields: Vec<GosValue>, objs: &mut Objects) -> GosValue {
        let typ = GosType {
            zero_val: GosValue::Interface(slotmap::Key::null()),
            data: GosTypeData::Interface(fields),
        };
        GosValue::Type(objs.types.insert(typ))
    }

    pub fn new_struct(
        fields: Vec<GosValue>,
        meta: HashMap<String, usize>,
        objs: &mut Objects,
    ) -> GosValue {
        let field_zeros: Vec<GosValue> = fields
            .iter()
            .map(|x| x.get_type_val(objs).zero_val().clone())
            .collect();
        let struct_val = StructVal {
            dark: false,
            typ: slotmap::Key::null(),
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

    pub fn new_channel(vtype: GosValue, objs: &mut Objects) -> GosValue {
        let typ = GosType {
            zero_val: GosValue::Channel(slotmap::Key::null()),
            data: GosTypeData::Channel(vtype),
        };
        GosValue::Type(objs.types.insert(typ))
    }

    pub fn new_boxed(inner: GosValue, objs: &mut Objects) -> GosValue {
        let typ = GosType {
            zero_val: GosValue::Boxed(slotmap::Key::null()),
            data: GosTypeData::Boxed(inner),
        };
        GosValue::Type(objs.types.insert(typ))
    }

    pub fn zero_val(&self) -> &GosValue {
        &self.zero_val
    }
}

// ----------------------------------------------------------------------------
// InterfaceVal

#[derive(Clone, Debug)]
pub struct InterfaceVal {}

// ----------------------------------------------------------------------------
// StringVal

#[derive(Clone, Debug)]
pub struct StringVal {
    pub dark: bool,
    pub data: String,
}

impl PartialEq for StringVal {
    fn eq(&self, other: &StringVal) -> bool {
        self.data == other.data
    }
}

// ----------------------------------------------------------------------------
// MapVal

#[derive(Clone)]
pub struct HashKey {
    pub val: GosValue,
    pub objs: &'static Objects,
}

impl Eq for HashKey {}

impl PartialEq for HashKey {
    fn eq(&self, other: &HashKey) -> bool {
        self.val == other.val
    }
}

impl fmt::Debug for HashKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.val.fmt(f)
    }
}

impl Hash for HashKey {
    fn hash<H: Hasher>(&self, state: &mut H) {
        match &self.val {
            GosValue::Nil => 0.hash(state),
            GosValue::Bool(b) => b.hash(state),
            GosValue::Int(i) => i.hash(state),
            GosValue::Float64(f) => f.to_bits().hash(state),
            GosValue::Str(s) => self.objs.strings[*s].data.hash(state),
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

#[derive(Clone)]
pub struct MapVal {
    pub dark: bool,
    objs: &'static Objects,
    default_val: GosValue,
    data: HashMap<HashKey, GosValue>,
}

impl fmt::Debug for MapVal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MapVal")
            .field("dark", &self.dark)
            .field("default_val", &self.default_val)
            .field("data", &self.data)
            .finish()
    }
}

impl MapVal {
    pub fn insert(&mut self, key: GosValue, val: GosValue) -> Option<GosValue> {
        let hk = HashKey {
            val: key,
            objs: self.objs,
        };
        self.data.insert(hk, val)
    }

    pub fn get(&self, key: &GosValue) -> &GosValue {
        let hk = HashKey {
            val: *key,
            objs: self.objs,
        };
        match self.data.get(&hk) {
            Some(v) => v,
            None => &self.default_val,
        }
    }
}

// ----------------------------------------------------------------------------
// StructVal

#[derive(Clone, Debug)]
pub struct StructVal {
    pub dark: bool,
    pub typ: TypeKey,
    pub fields: Vec<GosValue>,
}

impl StructVal {
    pub fn field_index(&self, name: &String, objs: &Objects) -> OpIndex {
        let t = &objs.types[self.typ];
        if let GosTypeData::Struct(_, map) = &t.data {
            map[name] as OpIndex
        } else {
            unreachable!()
        }
    }
}

// ----------------------------------------------------------------------------
// ChannelVal

#[derive(Clone, Debug)]
pub struct ChannelVal {
    pub dark: bool,
}

// ----------------------------------------------------------------------------
// SliceVal

#[derive(Clone, Debug)]
pub struct SliceVal {
    pub dark: bool,
    begin: usize,
    end: usize,
    soft_cap: usize, // <= self.vec.capacity()
    vec: Rc<RefCell<Vec<GosValue>>>,
}

impl<'a> SliceVal {
    pub fn len(&self) -> usize {
        self.end - self.begin
    }

    pub fn cap(&self) -> usize {
        self.soft_cap - self.begin
    }

    pub fn push(&mut self, val: GosValue) {
        self.try_grow_vec(self.len() + 1);
        self.vec.borrow_mut().push(val);
        self.end += 1;
    }

    pub fn append(&mut self, vals: &mut Vec<GosValue>) {
        let new_len = self.len() + vals.len();
        self.try_grow_vec(new_len);
        self.vec.borrow_mut().append(vals);
        self.end = self.begin + new_len;
    }

    pub fn iter(&'a self) -> SliceValIter<'a> {
        SliceValIter {
            slice: self,
            cur: self.begin,
        }
    }

    pub fn get(&self, i: usize) -> Option<GosValue> {
        if let Some(val) = self.vec.borrow().get(i) {
            Some(val.clone())
        } else {
            None
        }
    }

    pub fn set(&self, i: usize, val: GosValue) {
        self.vec.borrow_mut()[i] = val;
    }

    fn try_grow_vec(&mut self, len: usize) {
        let mut cap = self.cap();
        assert!(cap >= self.len());
        if cap >= len {
            return;
        }

        while cap < len {
            if cap < 1024 {
                cap *= 2
            } else {
                cap = (cap as f32 * 1.25) as usize
            }
        }
        let mut new_vec: Vec<GosValue> = Vec::with_capacity(cap);
        new_vec.copy_from_slice(&self.vec.borrow()[self.begin..self.end]);
        self.vec = Rc::new(RefCell::new(new_vec));
        self.begin = 0;
        self.end = cap;
        self.soft_cap = cap;
    }
}

pub struct SliceValIter<'a> {
    slice: &'a SliceVal,
    cur: usize,
}

impl<'a> Iterator for SliceValIter<'a> {
    type Item = GosValue;

    fn next(&mut self) -> Option<GosValue> {
        if self.cur < self.slice.end {
            self.cur += 1;
            Some(self.slice.get(self.cur - 1).unwrap())
        } else {
            None
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use std::collections::hash_map::DefaultHasher;
    fn calculate_hash<T: Hash>(t: &T) -> u64 {
        let mut s = DefaultHasher::new();
        t.hash(&mut s);
        s.finish()
    }

    #[test]
    fn test_float() {
        dbg!("1000000000000000000000001e10".parse::<f64>().unwrap());
    }

    #[test]
    fn test_gosvalue_debug() {
        let mut o = Objects::new();
        let s = new_str(&mut o.strings, "test_string".to_string());
        let slice = new_slice(&mut o.slices, 10);
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
        let mut o = Objects::new();
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
