#![macro_use]
use super::instruction::{Instruction, OpIndex, Opcode, Value32Type};
use super::value::GosValue;
use goscript_parser::objects::EntityKey;
use slotmap::{new_key_type, DenseSlotMap};
use std::cell::{Ref, RefCell, RefMut};
use std::cmp::Ordering;
use std::collections::HashMap;
use std::convert::TryInto;
use std::hash::Hash;
use std::iter::FromIterator;
use std::rc::{Rc, Weak};

const DEFAULT_CAPACITY: usize = 128;

#[macro_export]
macro_rules! null_key {
    () => {
        slotmap::Key::null()
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

new_key_type! { pub struct MetadataKey; }
new_key_type! { pub struct FunctionKey; }
new_key_type! { pub struct PackageKey; }

pub type InterfaceObjs = Vec<Weak<RefCell<InterfaceVal>>>;
pub type ClosureObjs = Vec<Weak<RefCell<ClosureVal>>>;
pub type SliceObjs = Vec<Weak<SliceVal>>;
pub type MapObjs = Vec<Weak<MapVal>>;
pub type StructObjs = Vec<Weak<RefCell<StructVal>>>;
pub type ChannelObjs = Vec<Weak<RefCell<ChannelVal>>>;
pub type BoxedObjs = Vec<Weak<GosValue>>;
pub type MetadataObjs = DenseSlotMap<MetadataKey, MetadataVal>;
pub type FunctionObjs = DenseSlotMap<FunctionKey, FunctionVal>;
pub type PackageObjs = DenseSlotMap<PackageKey, PackageVal>;

#[derive(Debug)]
pub struct VMObjects {
    pub interfaces: InterfaceObjs,
    pub closures: ClosureObjs,
    pub slices: SliceObjs,
    pub maps: MapObjs,
    pub structs: StructObjs,
    pub channels: ChannelObjs,
    pub boxed: BoxedObjs,
    pub metas: MetadataObjs,
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
            metas: DenseSlotMap::with_capacity_and_key(DEFAULT_CAPACITY),
            functions: DenseSlotMap::with_capacity_and_key(DEFAULT_CAPACITY),
            packages: DenseSlotMap::with_capacity_and_key(DEFAULT_CAPACITY),
            basic_types: HashMap::new(),
            default_sig_meta: None,
            str_zero_val: str_zero_val,
            slice_zero_val: slice_zero_val,
            map_zero_val: map_zero_val,
        };
        let btype = MetadataVal::new_bool(&mut objs);
        objs.basic_types.insert("bool", btype);
        let itype = MetadataVal::new_int(&mut objs);
        objs.basic_types.insert("int", itype);
        let ftype = MetadataVal::new_float64(&mut objs);
        objs.basic_types.insert("float64", ftype);
        let stype = MetadataVal::new_str(&mut objs);
        objs.basic_types.insert("string", stype);
        // default_sig_meta is used by manually assembiled functions
        objs.default_sig_meta = Some(MetadataVal::new_sig(None, vec![], vec![], false, &mut objs));
        objs
    }

    #[inline]
    pub fn metadata_bool(&self) -> GosValue {
        self.basic_type("bool").clone()
    }

    #[inline]
    pub fn metadata_int(&self) -> GosValue {
        self.basic_type("int").clone()
    }

    #[inline]
    pub fn metadata_float64(&self) -> GosValue {
        self.basic_type("float64").clone()
    }

    #[inline]
    pub fn metadata_string(&self) -> GosValue {
        self.basic_type("string").clone()
    }

    #[inline]
    fn basic_type(&self, name: &str) -> &GosValue {
        &self.basic_types[name]
    }
}

// ----------------------------------------------------------------------------
// StringVal

pub type StringIter<'a> = std::str::Chars<'a>;

pub type StringEnumIter<'a> = std::iter::Enumerate<std::str::Chars<'a>>;

#[derive(Debug)]
pub struct StringVal {
    data: Rc<String>,
    begin: usize,
    end: usize,
}

impl StringVal {
    #[inline]
    pub fn with_str(s: String) -> StringVal {
        let len = s.len();
        StringVal {
            data: Rc::new(s),
            begin: 0,
            end: len,
        }
    }

    #[inline]
    pub fn as_str(&self) -> &str {
        &self.data.as_ref()[self.begin..self.end]
    }

    #[inline]
    pub fn into_string(self) -> String {
        Rc::try_unwrap(self.data).unwrap()
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.end - self.begin
    }

    #[inline]
    pub fn get_byte(&self, i: usize) -> u8 {
        self.as_str().as_bytes()[i]
    }

    pub fn slice(&self, begin: Option<usize>, end: Option<usize>) -> StringVal {
        let self_len = self.len();
        let bi = begin.unwrap_or(0);
        let ei = end.unwrap_or(self_len);
        assert!(bi < self_len);
        assert!(bi <= ei && ei <= self_len);
        StringVal {
            data: Rc::clone(&self.data),
            begin: bi,
            end: ei,
        }
    }

    pub fn iter(&self) -> StringIter {
        self.as_str().chars()
    }
}

impl Clone for StringVal {
    #[inline]
    fn clone(&self) -> Self {
        StringVal {
            data: Rc::clone(&self.data),
            begin: self.begin,
            end: self.end,
        }
    }
}

impl PartialEq for StringVal {
    #[inline]
    fn eq(&self, other: &StringVal) -> bool {
        self.as_str().eq(other.as_str())
    }
}

impl Eq for StringVal {}

impl PartialOrd for StringVal {
    #[inline]
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for StringVal {
    #[inline]
    fn cmp(&self, other: &Self) -> Ordering {
        dbg!(self.as_str());
        dbg!(other.as_str());
        self.as_str().cmp(other.as_str())
    }
}

// ----------------------------------------------------------------------------
// MapVal

pub type GosHashMap = HashMap<GosValue, RefCell<GosValue>>;

#[derive(Debug)]
pub struct MapVal {
    pub dark: bool,
    default_val: RefCell<GosValue>,
    map: Rc<RefCell<GosHashMap>>,
}

impl MapVal {
    pub fn new(default_val: GosValue) -> MapVal {
        MapVal {
            dark: false,
            default_val: RefCell::new(default_val),
            map: Rc::new(RefCell::new(HashMap::new())),
        }
    }

    /// deep_clone creates a new MapVal with duplicated content of 'self.map'
    pub fn deep_clone(&self) -> MapVal {
        MapVal {
            dark: false,
            default_val: self.default_val.clone(),
            map: Rc::new(RefCell::new(self.map.borrow().clone())),
        }
    }

    #[inline]
    pub fn insert(&self, key: GosValue, val: GosValue) -> Option<GosValue> {
        self.map
            .borrow_mut()
            .insert(key, RefCell::new(val))
            .map(|x| x.into_inner())
    }

    #[inline]
    pub fn get(&self, key: &GosValue) -> GosValue {
        let mref = self.map.borrow();
        let cell = match mref.get(key) {
            Some(v) => v,
            None => &self.default_val,
        };
        cell.clone().into_inner()
    }

    /// touch_key makes sure there is a value for the 'key', a default value is set if
    /// the value is empty
    #[inline]
    pub fn touch_key(&self, key: &GosValue) {
        if self.map.borrow().get(&key).is_none() {
            self.map
                .borrow_mut()
                .insert(key.clone(), self.default_val.clone());
        }
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.map.borrow().len()
    }

    #[inline]
    pub fn borrow_data_mut(&self) -> RefMut<GosHashMap> {
        self.map.borrow_mut()
    }

    #[inline]
    pub fn borrow_data(&self) -> Ref<GosHashMap> {
        self.map.borrow()
    }

    #[inline]
    pub fn clone_inner(&self) -> Rc<RefCell<GosHashMap>> {
        Rc::clone(&self.map)
    }
}

impl Clone for MapVal {
    fn clone(&self) -> Self {
        MapVal {
            dark: false,
            default_val: self.default_val.clone(),
            map: Rc::clone(&self.map),
        }
    }
}

impl PartialEq for MapVal {
    fn eq(&self, _other: &MapVal) -> bool {
        unreachable!() //false
    }
}

impl Eq for MapVal {}

// ----------------------------------------------------------------------------
// SliceVal

pub type GosVec = Vec<RefCell<GosValue>>;

#[derive(Debug)]
pub struct SliceVal {
    pub dark: bool,
    begin: usize,
    end: usize,
    soft_cap: usize, // <= self.vec.capacity()
    vec: Rc<RefCell<GosVec>>,
}

impl<'a> SliceVal {
    pub fn new(len: usize, cap: usize, default_val: &GosValue) -> SliceVal {
        assert!(cap >= len);
        let mut val = SliceVal {
            dark: false,
            begin: 0,
            end: 0,
            soft_cap: cap,
            vec: Rc::new(RefCell::new(Vec::with_capacity(cap))),
        };
        for _ in 0..len {
            val.push(default_val.clone());
        }
        val
    }

    pub fn with_data(val: Vec<GosValue>) -> SliceVal {
        SliceVal {
            dark: false,
            begin: 0,
            end: val.len(),
            soft_cap: val.len(),
            vec: Rc::new(RefCell::new(
                val.into_iter().map(|x| RefCell::new(x)).collect(),
            )),
        }
    }

    /// deep_clone creates a new SliceVal with duplicated content of 'self.vec'
    pub fn deep_clone(&self) -> SliceVal {
        let vec = Vec::from_iter(self.vec.borrow()[self.begin..self.end].iter().cloned());
        SliceVal {
            dark: false,
            begin: 0,
            end: self.cap(),
            soft_cap: self.cap(),
            vec: Rc::new(RefCell::new(vec)),
        }
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.end - self.begin
    }

    #[inline]
    pub fn cap(&self) -> usize {
        self.soft_cap - self.begin
    }

    #[inline]
    pub fn borrow(&self) -> SliceRef {
        SliceRef::new(self)
    }

    #[inline]
    pub fn borrow_data_mut(&self) -> std::cell::RefMut<GosVec> {
        self.vec.borrow_mut()
    }

    #[inline]
    pub fn borrow_data(&self) -> std::cell::Ref<GosVec> {
        self.vec.borrow()
    }

    #[inline]
    pub fn push(&mut self, val: GosValue) {
        self.try_grow_vec(self.len() + 1);
        self.vec.borrow_mut().push(RefCell::new(val));
        self.end += 1;
    }

    #[inline]
    pub fn append(&mut self, vals: &mut GosVec) {
        let new_len = self.len() + vals.len();
        self.try_grow_vec(new_len);
        self.vec.borrow_mut().append(vals);
        self.end = self.begin + new_len;
    }

    #[inline]
    pub fn get(&self, i: usize) -> Option<GosValue> {
        self.vec
            .borrow()
            .get(self.begin + i)
            .map(|x| x.clone().into_inner())
    }

    #[inline]
    pub fn set(&self, i: usize, val: GosValue) {
        self.vec.borrow()[self.begin + i].replace(val);
    }

    #[inline]
    pub fn slice(&self, begin: Option<usize>, end: Option<usize>, max: Option<usize>) -> SliceVal {
        let self_len = self.len();
        let self_cap = self.cap();
        let bi = begin.unwrap_or(0);
        let ei = end.unwrap_or(self_len);
        let mi = max.unwrap_or(self_cap);
        assert!(bi < self_len);
        assert!(bi <= ei && ei <= self_len);
        assert!(ei <= mi && mi <= self_cap);
        SliceVal {
            dark: false,
            begin: self.begin + bi,
            end: self.begin + ei,
            soft_cap: self.begin + mi,
            vec: Rc::clone(&self.vec),
        }
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
        let data_len = self.len();
        let mut vec = Vec::from_iter(self.vec.borrow()[self.begin..self.end].iter().cloned());
        vec.reserve_exact(cap - vec.len());
        self.vec = Rc::new(RefCell::new(vec));
        self.begin = 0;
        self.end = data_len;
        self.soft_cap = cap;
    }
}

impl Clone for SliceVal {
    fn clone(&self) -> Self {
        SliceVal {
            dark: false,
            begin: self.begin,
            end: self.end,
            soft_cap: self.soft_cap,
            vec: Rc::clone(&self.vec),
        }
    }
}

pub struct SliceRef<'a> {
    vec_ref: Ref<'a, GosVec>,
    begin: usize,
    end: usize,
}

pub type SliceIter<'a> = std::slice::Iter<'a, RefCell<GosValue>>;

pub type SliceEnumIter<'a> = std::iter::Enumerate<std::slice::Iter<'a, RefCell<GosValue>>>;

impl<'a> SliceRef<'a> {
    pub fn new(s: &SliceVal) -> SliceRef {
        SliceRef {
            vec_ref: s.vec.borrow(),
            begin: s.begin,
            end: s.end,
        }
    }

    pub fn iter(&self) -> SliceIter {
        self.vec_ref[self.begin..self.end].iter()
    }

    #[inline]
    pub fn get(&self, i: usize) -> Option<&RefCell<GosValue>> {
        self.vec_ref.get(self.begin + i)
    }
}

impl PartialEq for SliceVal {
    fn eq(&self, _other: &SliceVal) -> bool {
        unreachable!() //false
    }
}

impl Eq for SliceVal {}

// ----------------------------------------------------------------------------
// StructVal

#[derive(Clone, Debug)]
pub struct StructVal {
    pub dark: bool,
    pub meta: GosValue,
    pub fields: Vec<GosValue>,
}

impl StructVal {
    pub fn field_method_index(&self, name: &str, metas: &MetadataObjs) -> OpIndex {
        if let MetadataType::Struct(_, map) = metas[*self.meta.as_meta()].typ() {
            map[name] as OpIndex
        } else {
            unreachable!()
        }
    }
}

// ----------------------------------------------------------------------------
// InterfaceVal

#[derive(Clone, Debug)]
pub struct InterfaceVal {
    pub dark: bool,
    pub meta: GosValue,
    pub obj: GosValue,              // the Named object behind the interface
    pub obj_meta: Option<GosValue>, // the type of the named object
    pub mapping: Vec<OpIndex>,      // mapping from interface's methods to object's methods
}

// ----------------------------------------------------------------------------
// ChannelVal

#[derive(Clone, Debug)]
pub struct ChannelVal {
    pub dark: bool,
}

// ----------------------------------------------------------------------------
// ClosureVal

#[derive(Clone, Debug, PartialEq)]
pub enum UpValue {
    /// Parent CallFrame is still alive, pointing to a local variable
    Open(FunctionKey, OpIndex), // (what func is the var defined, the index of the var)
    // Parent CallFrame is released, pointing to a Boxed value in the global pool
    Closed(GosValue),
}

/// ClosureVal is a variable containing a pinter to a function and
/// a. a receiver, in which case, it is a bound-method
/// b. upvalues, in which case, it is a "real" closure
///
#[derive(Clone, Debug)]
pub struct ClosureVal {
    pub func: FunctionKey,
    pub receiver: Option<GosValue>,
    pub upvalues: Vec<UpValue>,
}

impl ClosureVal {
    pub fn new(key: FunctionKey, upvalues: Vec<UpValue>) -> ClosureVal {
        ClosureVal {
            func: key,
            receiver: None,
            upvalues: upvalues,
        }
    }

    pub fn close_upvalue(&mut self, func: FunctionKey, index: OpIndex, val: &GosValue) {
        for i in 0..self.upvalues.len() {
            if self.upvalues[i] == UpValue::Open(func, index) {
                self.upvalues[i] = UpValue::Closed(val.clone());
            }
        }
    }
}

// ----------------------------------------------------------------------------
// PackageVal

/// PackageVal is part of the generated Bytecode, it stores imports, consts,
/// vars, funcs declared in a package
#[derive(Clone, Debug)]
pub struct PackageVal {
    name: String,
    members: Vec<GosValue>, // imports, const, var, func are all stored here
    member_indices: HashMap<EntityKey, OpIndex>,
    main_func_index: Option<usize>,
    // maps func_member_index of the constructor to pkg_member_index
    var_mapping: Option<HashMap<OpIndex, OpIndex>>,
}

impl PackageVal {
    pub fn new(name: String) -> PackageVal {
        PackageVal {
            name: name,
            members: Vec::new(),
            member_indices: HashMap::new(),
            main_func_index: None,
            var_mapping: Some(HashMap::new()),
        }
    }

    pub fn add_member(&mut self, entity: EntityKey, val: GosValue) -> OpIndex {
        self.members.push(val);
        let index = (self.members.len() - 1) as OpIndex;
        self.member_indices.insert(entity, index);
        index as OpIndex
    }

    /// add placeholder for vars, will be initialized when imported
    pub fn add_var(&mut self, entity: EntityKey, fn_index: OpIndex) -> OpIndex {
        let index = self.add_member(entity, GosValue::Nil);
        self.var_mapping
            .as_mut()
            .unwrap()
            .insert(fn_index.into(), index);
        index
    }

    pub fn init_var(&mut self, fn_member_index: &OpIndex, val: GosValue) {
        let index = self.var_mapping.as_ref().unwrap()[fn_member_index];
        self.members[index as usize] = val;
    }

    pub fn var_count(&self) -> usize {
        self.var_mapping.as_ref().unwrap().len()
    }

    pub fn set_main_func(&mut self, index: OpIndex) {
        self.main_func_index = Some(index as usize);
    }

    pub fn get_member_index(&self, entity: &EntityKey) -> Option<&OpIndex> {
        self.member_indices.get(entity)
    }

    pub fn inited(&self) -> bool {
        self.var_mapping.is_none()
    }

    pub fn set_inited(&mut self) {
        self.var_mapping = None
    }

    // negative index for main func
    pub fn member(&self, i: OpIndex) -> &GosValue {
        if i >= 0 {
            &self.members[i as usize]
        } else {
            &self.members[self.main_func_index.unwrap()]
        }
    }

    pub fn member_mut(&mut self, i: OpIndex) -> &mut GosValue {
        if i >= 0 {
            &mut self.members[i as usize]
        } else {
            &mut self.members[self.main_func_index.unwrap()]
        }
    }
}

// ----------------------------------------------------------------------------
// FunctionVal

/// EntIndex is for addressing a variable in the scope of a function
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum EntIndex {
    Const(OpIndex),
    LocalVar(OpIndex),
    UpValue(OpIndex),
    PackageMember(OpIndex),
    BuiltIn(Opcode), // built-in identifiers
    Blank,
}

impl From<EntIndex> for OpIndex {
    fn from(t: EntIndex) -> OpIndex {
        match t {
            EntIndex::Const(i) => i,
            EntIndex::LocalVar(i) => i,
            EntIndex::UpValue(i) => i,
            EntIndex::PackageMember(i) => i,
            EntIndex::BuiltIn(_) => unreachable!(),
            EntIndex::Blank => unreachable!(),
        }
    }
}

/// FunctionVal is the direct container of the Opcode.
#[derive(Clone, Debug)]
pub struct FunctionVal {
    pub package: PackageKey,
    pub meta: GosValue,
    pub code: Vec<Instruction>,
    pub consts: Vec<GosValue>,
    pub up_ptrs: Vec<UpValue>,
    // param_count, ret_count can be read from typ,
    // these fields are for faster access
    pub param_count: usize,
    pub ret_count: usize,
    entities: HashMap<EntityKey, EntIndex>,
    local_alloc: u16,
    variadic: bool,
    is_ctor: bool,
}

impl FunctionVal {
    pub fn new(package: PackageKey, meta: GosValue, variadic: bool, ctor: bool) -> FunctionVal {
        FunctionVal {
            package: package,
            meta: meta,
            code: Vec::new(),
            consts: Vec::new(),
            up_ptrs: Vec::new(),
            param_count: 0,
            ret_count: 0,
            entities: HashMap::new(),
            local_alloc: 0,
            variadic: variadic,
            is_ctor: ctor,
        }
    }

    pub fn is_ctor(&self) -> bool {
        self.is_ctor
    }

    pub fn variadic(&self) -> bool {
        self.variadic
    }

    pub fn local_count(&self) -> usize {
        self.local_alloc as usize - self.param_count - self.ret_count
    }

    pub fn entity_index(&self, entity: &EntityKey) -> Option<&EntIndex> {
        self.entities.get(entity)
    }

    pub fn const_val(&self, index: OpIndex) -> &GosValue {
        &self.consts[index as usize]
    }

    pub fn emit_inst(
        &mut self,
        op: Opcode,
        type0: Option<Value32Type>,
        type1: Option<Value32Type>,
        type2: Option<Value32Type>,
        imm: Option<i32>,
    ) {
        let i = Instruction::new(op, type0, type1, type2, imm);
        self.code.push(i);
    }

    pub fn emit_code(&mut self, code: Opcode) {
        self.emit_inst(code, None, None, None, None);
    }

    /// returns the index of the const if it's found
    pub fn get_const_index(&self, val: &GosValue) -> Option<EntIndex> {
        self.consts.iter().enumerate().find_map(|(i, x)| {
            if val == x {
                Some(EntIndex::Const(i as OpIndex))
            } else {
                None
            }
        })
    }

    // for unnamed return values, entity == None
    pub fn add_local(&mut self, entity: Option<EntityKey>) -> EntIndex {
        let result = self.local_alloc as OpIndex;
        if let Some(key) = entity {
            let old = self.entities.insert(key, EntIndex::LocalVar(result));
            assert_eq!(old, None);
        };
        self.local_alloc += 1;
        EntIndex::LocalVar(result)
    }

    /// add a const or get the index of a const.
    /// when 'entity' is no none, it's a const define, so it should not be called with the
    /// same 'entity' more than once
    pub fn add_const(&mut self, entity: Option<EntityKey>, cst: GosValue) -> EntIndex {
        if let Some(index) = self.get_const_index(&cst) {
            index
        } else {
            self.consts.push(cst);
            let result = (self.consts.len() - 1).try_into().unwrap();
            if let Some(key) = entity {
                let old = self.entities.insert(key, EntIndex::Const(result));
                assert_eq!(old, None);
            }
            EntIndex::Const(result)
        }
    }

    pub fn try_add_upvalue(&mut self, entity: &EntityKey, uv: UpValue) -> EntIndex {
        self.entities
            .get(entity)
            .map(|x| *x)
            .or_else(|| {
                self.up_ptrs.push(uv);
                let i = (self.up_ptrs.len() - 1).try_into().ok();
                let et = EntIndex::UpValue(i.unwrap());
                self.entities.insert(*entity, et);
                i.map(|x| EntIndex::UpValue(x))
            })
            .unwrap()
    }
}

// ----------------------------------------------------------------------------
// MetadataVal
#[derive(Debug)]
pub struct SigMetadata {
    pub recv: Option<GosValue>,
    pub params: Vec<GosValue>,
    pub results: Vec<GosValue>,
    pub variadic: bool,
}

#[derive(Debug)]
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

#[derive(Debug)]
pub struct MetadataVal {
    zero_val: GosValue,
    typ: MetadataType,
}

impl MetadataVal {
    fn new_bool(objs: &mut VMObjects) -> GosValue {
        let m = MetadataVal {
            zero_val: GosValue::Bool(false),
            typ: MetadataType::None,
        };
        GosValue::new_meta(m, &mut objs.metas)
    }

    fn new_int(objs: &mut VMObjects) -> GosValue {
        let m = MetadataVal {
            zero_val: GosValue::Int(0),
            typ: MetadataType::None,
        };
        GosValue::new_meta(m, &mut objs.metas)
    }

    fn new_float64(objs: &mut VMObjects) -> GosValue {
        let m = MetadataVal {
            zero_val: GosValue::Float64(0.0),
            typ: MetadataType::None,
        };
        GosValue::new_meta(m, &mut objs.metas)
    }

    fn new_str(objs: &mut VMObjects) -> GosValue {
        let m = MetadataVal {
            zero_val: GosValue::Str(objs.str_zero_val.clone()),
            typ: MetadataType::None,
        };
        GosValue::new_meta(m, &mut objs.metas)
    }

    pub fn new_sig(
        recv: Option<GosValue>,
        params: Vec<GosValue>,
        results: Vec<GosValue>,
        variadic: bool,
        objs: &mut VMObjects,
    ) -> GosValue {
        let m = MetadataVal {
            zero_val: GosValue::Nil,
            typ: MetadataType::Signature(SigMetadata {
                recv: recv,
                params: params,
                results: results,
                variadic: variadic,
            }),
        };
        GosValue::new_meta(m, &mut objs.metas)
    }

    pub fn new_slice(vtype: GosValue, objs: &mut VMObjects) -> GosValue {
        let m = MetadataVal {
            zero_val: GosValue::Slice(objs.slice_zero_val.clone()),
            typ: MetadataType::Slice(vtype),
        };
        GosValue::new_meta(m, &mut objs.metas)
    }

    pub fn new_map(ktype: GosValue, vtype: GosValue, objs: &mut VMObjects) -> GosValue {
        let m = MetadataVal {
            zero_val: GosValue::Map(objs.map_zero_val.clone()),
            typ: MetadataType::Map(ktype, vtype),
        };
        GosValue::new_meta(m, &mut objs.metas)
    }

    pub fn new_interface(fields: Vec<GosValue>, objs: &mut VMObjects) -> GosValue {
        let m = MetadataVal {
            zero_val: GosValue::Interface(Rc::new(RefCell::new(InterfaceVal {
                dark: false,
                meta: GosValue::Nil,
                obj: GosValue::Nil,
                obj_meta: None,
                mapping: vec![],
            }))),
            typ: MetadataType::Interface(fields),
        };
        GosValue::new_meta(m, &mut objs.metas)
    }

    pub fn new_struct(
        fields: Vec<GosValue>,
        fields_index: HashMap<String, OpIndex>,
        objs: &mut VMObjects,
    ) -> GosValue {
        let field_zeros: Vec<GosValue> = fields
            .iter()
            .map(|x| objs.metas[*x.as_meta()].zero_val().clone())
            .collect();
        let struct_val = StructVal {
            dark: false,
            meta: GosValue::Nil,
            fields: field_zeros,
        };
        let meta = MetadataVal {
            zero_val: GosValue::Struct(Rc::new(RefCell::new(struct_val))),
            typ: MetadataType::Struct(fields, fields_index),
        };
        let key = objs.metas.insert(meta);
        objs.metas[key].zero_val().as_struct().borrow_mut().meta = GosValue::Metadata(key);
        GosValue::Metadata(key)
    }

    pub fn new_channel(vtype: GosValue, objs: &mut VMObjects) -> GosValue {
        let m = MetadataVal {
            zero_val: GosValue::Channel(Rc::new(RefCell::new(ChannelVal { dark: false }))),
            typ: MetadataType::Channel(vtype),
        };
        GosValue::new_meta(m, &mut objs.metas)
    }

    pub fn new_boxed(inner: GosValue, objs: &mut VMObjects) -> GosValue {
        let m = MetadataVal {
            zero_val: GosValue::Boxed(Rc::new(RefCell::new(GosValue::Nil))),
            typ: MetadataType::Boxed(inner),
        };
        GosValue::new_meta(m, &mut objs.metas)
    }

    pub fn zero_val(&self) -> &GosValue {
        &self.zero_val
    }

    pub fn typ(&self) -> &MetadataType {
        &self.typ
    }

    pub fn typ_mut(&mut self) -> &mut MetadataType {
        &mut self.typ
    }

    pub fn get_value32_type(&self, metas: &MetadataObjs) -> Value32Type {
        match &self.typ {
            MetadataType::None => match &self.zero_val {
                GosValue::Nil => Value32Type::Nil,
                GosValue::Bool(_) => Value32Type::Bool,
                GosValue::Int(_) => Value32Type::Int,
                GosValue::Float64(_) => Value32Type::Float64,
                GosValue::Complex64(_, _) => Value32Type::Complex64,
                GosValue::Str(_) => Value32Type::Str,
                _ => unreachable!(),
            },
            MetadataType::Signature(_) => Value32Type::Function,
            MetadataType::Slice(_) => Value32Type::Slice,
            MetadataType::Map(_, _) => Value32Type::Map,
            MetadataType::Interface(_) => Value32Type::Interface,
            MetadataType::Struct(_, _) => Value32Type::Struct,
            MetadataType::Channel(_) => Value32Type::Channel,
            MetadataType::Boxed(_) => Value32Type::Boxed,
            MetadataType::Named(btype, _) => metas[*btype.as_meta()].get_value32_type(metas),
        }
    }
}
