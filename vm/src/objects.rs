#![macro_use]
use super::instruction::{Instruction, OpIndex, Opcode, ValueType};
use super::value::GosValue;
use goscript_parser::objects::EntityKey;
use slotmap::{new_key_type, DenseSlotMap};
use std::cell::{Ref, RefCell, RefMut};
use std::cmp::Ordering;
use std::collections::HashMap;
use std::convert::TryFrom;
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

new_key_type! { pub struct MetadataKey; }
new_key_type! { pub struct FunctionKey; }
new_key_type! { pub struct PackageKey; }

pub type InterfaceObjs = Vec<Weak<RefCell<InterfaceObj>>>;
pub type ClosureObjs = Vec<Weak<RefCell<ClosureObj>>>;
pub type SliceObjs = Vec<Weak<SliceObj>>;
pub type MapObjs = Vec<Weak<MapObj>>;
pub type StructObjs = Vec<Weak<RefCell<StructObj>>>;
pub type ChannelObjs = Vec<Weak<RefCell<ChannelObj>>>;
pub type BoxedObjs = Vec<Weak<GosValue>>;
pub type MetadataObjs = DenseSlotMap<MetadataKey, MetadataVal>;
pub type FunctionObjs = DenseSlotMap<FunctionKey, FunctionVal>;
pub type PackageObjs = DenseSlotMap<PackageKey, PackageVal>;

pub fn key_to_u64<K>(key: K) -> u64
where
    K: slotmap::Key,
{
    let data: slotmap::KeyData = key.into();
    data.as_ffi()
}

pub fn u64_to_key<K>(u: u64) -> K
where
    K: slotmap::Key,
{
    let data = slotmap::KeyData::from_ffi(u);
    data.into()
}

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
    pub bool_meta: GosValue,
    pub int_meta: GosValue,
    pub float64_meta: GosValue,
    pub complex64_meta: GosValue,
    pub string_meta: GosValue,
    pub default_sig_meta: Option<GosValue>,
    pub zero_val: ZeroVal,
}

impl VMObjects {
    pub fn new() -> VMObjects {
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
            bool_meta: GosValue::Nil,
            int_meta: GosValue::Nil,
            float64_meta: GosValue::Nil,
            complex64_meta: GosValue::Nil,
            string_meta: GosValue::Nil,
            default_sig_meta: None,
            zero_val: ZeroVal::new(),
        };
        objs.bool_meta = MetadataVal::new_bool(&mut objs);
        objs.int_meta = MetadataVal::new_int(&mut objs);
        objs.float64_meta = MetadataVal::new_float64(&mut objs);
        objs.complex64_meta = MetadataVal::new_complex64(&mut objs);
        objs.string_meta = MetadataVal::new_str(&mut objs);
        // default_sig_meta is used by manually assembiled functions
        objs.default_sig_meta = Some(MetadataVal::new_sig(None, vec![], vec![], None, &mut objs));
        objs
    }
}

#[derive(Debug)]
pub struct ZeroVal {
    pub str_zero_val: GosValue,
    pub boxed_zero_val: GosValue,
    pub closure_zero_val: GosValue,
    pub slice_zero_val: GosValue,
    pub map_zero_val: GosValue,
    pub iface_zero_val: GosValue,
    pub chan_zero_val: GosValue,
    pub pkg_zero_val: GosValue,
}

impl ZeroVal {
    fn new() -> ZeroVal {
        ZeroVal {
            str_zero_val: GosValue::Str(Rc::new(StringObj::with_str("".to_string()))),
            boxed_zero_val: GosValue::Boxed(Box::new(BoxedObj::Nil)),
            closure_zero_val: GosValue::Closure(Rc::new(ClosureObj::new(null_key!(), None, None))),
            slice_zero_val: GosValue::Slice(Rc::new(SliceObj::new(0, 0, &GosValue::Nil))),
            map_zero_val: GosValue::Map(Rc::new(MapObj::new(GosValue::Nil))),
            iface_zero_val: GosValue::Interface(Rc::new(RefCell::new(InterfaceObj::new(
                GosValue::Nil,
                None,
            )))),
            chan_zero_val: GosValue::Channel(Rc::new(RefCell::new(ChannelObj {}))),
            pkg_zero_val: GosValue::Package(null_key!()),
        }
    }

    #[inline]
    pub fn nil_zero_val(&self, t: ValueType) -> &GosValue {
        match t {
            ValueType::Boxed => &self.boxed_zero_val,
            ValueType::Interface => &self.iface_zero_val,
            ValueType::Map => &self.map_zero_val,
            ValueType::Slice => &self.slice_zero_val,
            ValueType::Channel => &self.chan_zero_val,
            ValueType::Function => &self.closure_zero_val,
            _ => unreachable!(),
        }
    }
}

// ----------------------------------------------------------------------------
// StringObj

pub type StringIter<'a> = std::str::Chars<'a>;

pub type StringEnumIter<'a> = std::iter::Enumerate<std::str::Chars<'a>>;

#[derive(Debug)]
pub struct StringObj {
    data: Rc<String>,
    begin: usize,
    end: usize,
}

impl StringObj {
    #[inline]
    pub fn with_str(s: String) -> StringObj {
        let len = s.len();
        StringObj {
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

    pub fn slice(&self, begin: isize, end: isize) -> StringObj {
        let self_len = self.len() as isize + 1;
        let bi = begin as usize;
        let ei = ((self_len + end) % self_len) as usize;
        StringObj {
            data: Rc::clone(&self.data),
            begin: bi,
            end: ei,
        }
    }

    pub fn iter(&self) -> StringIter {
        self.as_str().chars()
    }
}

impl Clone for StringObj {
    #[inline]
    fn clone(&self) -> Self {
        StringObj {
            data: Rc::clone(&self.data),
            begin: self.begin,
            end: self.end,
        }
    }
}

impl PartialEq for StringObj {
    #[inline]
    fn eq(&self, other: &StringObj) -> bool {
        self.as_str().eq(other.as_str())
    }
}

impl Eq for StringObj {}

impl PartialOrd for StringObj {
    #[inline]
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for StringObj {
    #[inline]
    fn cmp(&self, other: &Self) -> Ordering {
        dbg!(self.as_str());
        dbg!(other.as_str());
        self.as_str().cmp(other.as_str())
    }
}

// ----------------------------------------------------------------------------
// MapObj

pub type GosHashMap = HashMap<GosValue, RefCell<GosValue>>;

#[derive(Debug)]
pub struct MapObj {
    pub dark: bool,
    default_val: RefCell<GosValue>,
    map: Rc<RefCell<GosHashMap>>,
}

impl MapObj {
    pub fn new(default_val: GosValue) -> MapObj {
        MapObj {
            dark: false,
            default_val: RefCell::new(default_val),
            map: Rc::new(RefCell::new(HashMap::new())),
        }
    }

    /// deep_clone creates a new MapObj with duplicated content of 'self.map'
    pub fn deep_clone(&self) -> MapObj {
        MapObj {
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

impl Clone for MapObj {
    fn clone(&self) -> Self {
        MapObj {
            dark: false,
            default_val: self.default_val.clone(),
            map: Rc::clone(&self.map),
        }
    }
}

impl PartialEq for MapObj {
    fn eq(&self, _other: &MapObj) -> bool {
        unreachable!() //false
    }
}

impl Eq for MapObj {}

// ----------------------------------------------------------------------------
// SliceObj

pub type GosVec = Vec<RefCell<GosValue>>;

#[derive(Debug)]
pub struct SliceObj {
    pub dark: bool,
    begin: usize,
    end: usize,
    soft_cap: usize, // <= self.vec.capacity()
    vec: Rc<RefCell<GosVec>>,
}

impl<'a> SliceObj {
    pub fn new(len: usize, cap: usize, default_val: &GosValue) -> SliceObj {
        assert!(cap >= len);
        let mut val = SliceObj {
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

    pub fn with_data(val: Vec<GosValue>) -> SliceObj {
        SliceObj {
            dark: false,
            begin: 0,
            end: val.len(),
            soft_cap: val.len(),
            vec: Rc::new(RefCell::new(
                val.into_iter().map(|x| RefCell::new(x)).collect(),
            )),
        }
    }

    /// deep_clone creates a new SliceObj with duplicated content of 'self.vec'
    pub fn deep_clone(&self) -> SliceObj {
        let vec = Vec::from_iter(self.vec.borrow()[self.begin..self.end].iter().cloned());
        SliceObj {
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
    pub fn slice(&self, begin: isize, end: isize, max: isize) -> SliceObj {
        let self_len = self.len() as isize + 1;
        let self_cap = self.cap() as isize + 1;
        let bi = begin as usize;
        let ei = ((self_len + end) % self_len) as usize;
        let mi = ((self_cap + max) % self_cap) as usize;
        SliceObj {
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

impl Clone for SliceObj {
    fn clone(&self) -> Self {
        SliceObj {
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
    pub fn new(s: &SliceObj) -> SliceRef {
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

impl PartialEq for SliceObj {
    fn eq(&self, _other: &SliceObj) -> bool {
        unreachable!() //false
    }
}

impl Eq for SliceObj {}

// ----------------------------------------------------------------------------
// StructObj

#[derive(Clone, Debug)]
pub struct StructObj {
    pub dark: bool,
    pub meta: GosValue,
    pub fields: Vec<GosValue>,
}

impl StructObj {}

// ----------------------------------------------------------------------------
// InterfaceObj

#[derive(Clone, Debug)]
pub struct InterfaceObj {
    pub meta: GosValue,
    // the Named object behind the interface
    // mapping from interface's methods to object's methods
    underlying: Option<(GosValue, Rc<Vec<FunctionKey>>)>,
}

impl InterfaceObj {
    pub fn new(
        meta: GosValue,
        underlying: Option<(GosValue, Rc<Vec<FunctionKey>>)>,
    ) -> InterfaceObj {
        InterfaceObj {
            meta: meta,
            underlying: underlying,
        }
    }

    #[inline]
    pub fn underlying(&self) -> &Option<(GosValue, Rc<Vec<FunctionKey>>)> {
        &self.underlying
    }

    #[inline]
    pub fn set_underlying(&mut self, named: GosValue, mapping: Rc<Vec<FunctionKey>>) {
        self.underlying = Some((named, mapping));
    }
}

// ----------------------------------------------------------------------------
// ChannelObj

#[derive(Clone, Debug)]
pub struct ChannelObj {}

// ----------------------------------------------------------------------------
// BoxedObj
/// There are two kinds of boxed vars, which is determined by the behavior of
/// copy_semantic. Struct, Slice and Map have true pointers
/// Others don't have true pointers, so a upvalue-like open/close mechanism is needed
#[derive(Debug, Clone)]
pub enum BoxedObj {
    Nil,
    UpVal(UpValue),
    Struct(Rc<RefCell<StructObj>>),
    SliceMember(Rc<SliceObj>, OpIndex),
    StructField(Rc<RefCell<StructObj>>, OpIndex),
    PkgMember(PackageKey, OpIndex),
}

impl BoxedObj {
    #[inline]
    pub fn new_var_up_val(d: ValueDesc) -> BoxedObj {
        BoxedObj::UpVal(UpValue::new(d))
    }

    // supports only Struct for now
    #[inline]
    pub fn new_var_pointer(val: GosValue) -> BoxedObj {
        match val {
            GosValue::Struct(s) => BoxedObj::Struct(s),
            _ => unreachable!(),
        }
    }

    #[inline]
    pub fn new_slice_member(slice: &GosValue, index: OpIndex) -> BoxedObj {
        let s = slice.as_slice();
        BoxedObj::SliceMember(s.clone(), index)
    }

    #[inline]
    pub fn new_struct_field(stru: &GosValue, index: OpIndex) -> BoxedObj {
        let s = stru.as_struct();
        BoxedObj::StructField(s.clone(), index)
    }
}

// ----------------------------------------------------------------------------
// ClosureObj

#[derive(Clone, Debug, PartialEq)]
pub struct ValueDesc {
    pub func: FunctionKey,
    pub index: OpIndex,
    pub typ: ValueType,
}

#[derive(Clone, Debug, PartialEq)]
pub enum UpValueState {
    /// Parent CallFrame is still alive, pointing to a local variable
    Open(ValueDesc), // (what func is the var defined, the index of the var)
    // Parent CallFrame is released, pointing to a Boxed value in the global pool
    Closed(GosValue),
}

#[derive(Clone, Debug, PartialEq)]
pub struct UpValue {
    pub inner: Rc<RefCell<UpValueState>>,
}

impl UpValue {
    pub fn new(d: ValueDesc) -> UpValue {
        UpValue {
            inner: Rc::new(RefCell::new(UpValueState::Open(d))),
        }
    }

    pub fn downgrade(&self) -> WeakUpValue {
        WeakUpValue {
            inner: Rc::downgrade(&self.inner),
        }
    }

    pub fn desc(&self) -> ValueDesc {
        let r: &UpValueState = &self.inner.borrow();
        match r {
            UpValueState::Open(d) => d.clone(),
            _ => unreachable!(),
        }
    }

    pub fn close(&self, val: GosValue) {
        *self.inner.borrow_mut() = UpValueState::Closed(val);
    }
}

#[derive(Clone, Debug)]
pub struct WeakUpValue {
    pub inner: Weak<RefCell<UpValueState>>,
}

impl WeakUpValue {
    pub fn upgrade(&self) -> Option<UpValue> {
        Weak::upgrade(&self.inner).map(|x| UpValue { inner: x })
    }
}

/// ClosureObj is a variable containing a pinter to a function and
/// a. a receiver, in which case, it is a bound-method
/// b. upvalues, in which case, it is a "real" closure
///
#[derive(Clone, Debug)]
pub struct ClosureObj {
    pub func: FunctionKey,
    pub receiver: Option<GosValue>,
    upvalues: Option<Vec<UpValue>>,
}

impl ClosureObj {
    pub fn new(
        key: FunctionKey,
        receiver: Option<GosValue>,
        upvalues: Option<Vec<ValueDesc>>,
    ) -> ClosureObj {
        ClosureObj {
            func: key,
            receiver: receiver,
            upvalues: upvalues.map(|uvs| uvs.into_iter().map(|x| UpValue::new(x)).collect()),
        }
    }

    #[inline]
    pub fn has_upvalues(&self) -> bool {
        self.upvalues.is_some()
    }

    #[inline]
    pub fn upvalues(&self) -> &Vec<UpValue> {
        self.upvalues.as_ref().unwrap()
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
    member_indices: HashMap<String, OpIndex>,
    // maps func_member_index of the constructor to pkg_member_index
    var_mapping: Option<HashMap<OpIndex, OpIndex>>,
}

impl PackageVal {
    pub fn new(name: String) -> PackageVal {
        PackageVal {
            name: name,
            members: Vec::new(),
            member_indices: HashMap::new(),
            var_mapping: Some(HashMap::new()),
        }
    }

    pub fn add_member(&mut self, name: String, val: GosValue) -> OpIndex {
        self.members.push(val);
        let index = (self.members.len() - 1) as OpIndex;
        self.member_indices.insert(name, index);
        index as OpIndex
    }

    pub fn add_var_mapping(&mut self, name: String, fn_index: OpIndex) -> OpIndex {
        let index = *self.get_member_index(&name).unwrap();
        self.var_mapping
            .as_mut()
            .unwrap()
            .insert(fn_index.into(), index);
        index
    }

    pub fn var_mut(&mut self, fn_member_index: OpIndex) -> &mut GosValue {
        let index = self.var_mapping.as_ref().unwrap()[&fn_member_index];
        &mut self.members[index as usize]
    }

    pub fn var_count(&self) -> usize {
        self.var_mapping.as_ref().unwrap().len()
    }

    pub fn get_member_index(&self, name: &str) -> Option<&OpIndex> {
        self.member_indices.get(name)
    }

    pub fn inited(&self) -> bool {
        self.var_mapping.is_none()
    }

    pub fn set_inited(&mut self) {
        self.var_mapping = None
    }

    #[inline]
    pub fn member(&self, i: OpIndex) -> &GosValue {
        &self.members[i as usize]
    }

    #[inline]
    pub fn member_mut(&mut self, i: OpIndex) -> &mut GosValue {
        &mut self.members[i as usize]
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
    pub up_ptrs: Vec<ValueDesc>,
    // param_count, ret_count can be read from typ,
    // these fields are for faster access
    pub param_count: usize,
    pub ret_count: usize,
    pub local_metas: Vec<GosValue>,
    // from local_metas, for faster runtime access
    pub local_zeros: Vec<GosValue>,
    entities: HashMap<EntityKey, EntIndex>,
    local_alloc: u16,
    variadic_type: Option<ValueType>,
    is_ctor: bool,
}

impl FunctionVal {
    pub fn new(
        package: PackageKey,
        meta: GosValue,
        variadic: Option<ValueType>,
        ctor: bool,
    ) -> FunctionVal {
        FunctionVal {
            package: package,
            meta: meta,
            code: Vec::new(),
            consts: Vec::new(),
            up_ptrs: Vec::new(),
            param_count: 0,
            ret_count: 0,
            local_metas: Vec::new(),
            local_zeros: Vec::new(),
            entities: HashMap::new(),
            local_alloc: 0,
            variadic_type: variadic,
            is_ctor: ctor,
        }
    }

    pub fn is_ctor(&self) -> bool {
        self.is_ctor
    }

    pub fn variadic(&self) -> Option<ValueType> {
        self.variadic_type
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

    pub fn offset(&self, loc: usize) -> OpIndex {
        // todo: don't crash if OpIndex overflows
        OpIndex::try_from((self.code.len() - loc) as isize).unwrap()
    }

    pub fn next_code_index(&self) -> usize {
        self.code.len()
    }

    pub fn emit_inst(
        &mut self,
        op: Opcode,
        type0: Option<ValueType>,
        type1: Option<ValueType>,
        type2: Option<ValueType>,
        imm: Option<i32>,
    ) {
        let i = Instruction::new(op, type0, type1, type2, imm);
        self.code.push(i);
    }

    pub fn emit_raw_inst(&mut self, u: u64) {
        let i = Instruction::from_u64(u);
        self.code.push(i);
    }

    pub fn emit_code_with_type(&mut self, code: Opcode, t: ValueType) {
        self.emit_inst(code, Some(t), None, None, None);
    }

    pub fn emit_code_with_imm(&mut self, code: Opcode, imm: OpIndex) {
        self.emit_inst(code, None, None, None, Some(imm));
    }

    pub fn emit_code_with_type_imm(&mut self, code: Opcode, t: ValueType, imm: OpIndex) {
        self.emit_inst(code, Some(t), None, None, Some(imm));
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
    // for parameters, zero == None
    pub fn add_local(&mut self, entity: Option<EntityKey>, zero: Option<GosValue>) -> EntIndex {
        let result = self.local_alloc as OpIndex;
        if let Some(key) = entity {
            let old = self.entities.insert(key, EntIndex::LocalVar(result));
            assert_eq!(old, None);
        };
        if let Some(z) = zero {
            self.local_zeros.push(z);
        }
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

    pub fn try_add_upvalue(&mut self, entity: &EntityKey, uv: ValueDesc) -> EntIndex {
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
    pub variadic: Option<GosValue>,
}

impl SigMetadata {
    pub fn boxed_recv(&self, metas: &MetadataObjs) -> bool {
        self.recv.as_ref().map_or(false, |x| {
            MetadataVal::get_value_type(&x, metas) == ValueType::Boxed
        })
    }
}

#[derive(Debug)]
pub struct OrderedMembers {
    pub members: Vec<GosValue>,
    pub mapping: HashMap<String, OpIndex>,
}

impl OrderedMembers {
    pub fn new(members: Vec<GosValue>, mapping: HashMap<String, OpIndex>) -> OrderedMembers {
        OrderedMembers {
            members: members,
            mapping: mapping,
        }
    }

    pub fn iface_mapping(&self, named_obj: &OrderedMembers) -> Vec<FunctionKey> {
        let mut result = vec![null_key!(); self.members.len()];
        for (n, i) in self.mapping.iter() {
            result[*i as usize] = named_obj.members[named_obj.mapping[n] as usize]
                .as_closure()
                .func;
        }
        result
    }
}

#[derive(Debug)]
pub enum MetadataType {
    None,
    Signature(SigMetadata),
    Slice(GosValue),
    Map(GosValue, GosValue),
    Interface(OrderedMembers),
    Struct(OrderedMembers),
    Channel(GosValue),
    Boxed(GosValue),
    Named(OrderedMembers, GosValue),
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

    fn new_complex64(objs: &mut VMObjects) -> GosValue {
        let m = MetadataVal {
            zero_val: GosValue::Complex64(0.0.into(), 0.0.into()),
            typ: MetadataType::None,
        };
        GosValue::new_meta(m, &mut objs.metas)
    }

    fn new_float64(objs: &mut VMObjects) -> GosValue {
        let m = MetadataVal {
            zero_val: GosValue::Float64(0.0.into()),
            typ: MetadataType::None,
        };
        GosValue::new_meta(m, &mut objs.metas)
    }

    fn new_str(objs: &mut VMObjects) -> GosValue {
        let m = MetadataVal {
            zero_val: objs.zero_val.str_zero_val.clone(),
            typ: MetadataType::None,
        };
        GosValue::new_meta(m, &mut objs.metas)
    }

    pub fn new_sig(
        recv: Option<GosValue>,
        params: Vec<GosValue>,
        results: Vec<GosValue>,
        variadic: Option<GosValue>,
        objs: &mut VMObjects,
    ) -> GosValue {
        let m = MetadataVal {
            zero_val: objs.zero_val.closure_zero_val.clone(),
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
            zero_val: objs.zero_val.slice_zero_val.clone(),
            typ: MetadataType::Slice(vtype),
        };
        GosValue::new_meta(m, &mut objs.metas)
    }

    pub fn new_map(ktype: GosValue, vtype: GosValue, objs: &mut VMObjects) -> GosValue {
        let m = MetadataVal {
            zero_val: objs.zero_val.map_zero_val.clone(),
            typ: MetadataType::Map(ktype, vtype),
        };
        GosValue::new_meta(m, &mut objs.metas)
    }

    pub fn new_interface(fields: OrderedMembers, objs: &mut VMObjects) -> GosValue {
        let m = MetadataVal {
            zero_val: objs.zero_val.iface_zero_val.clone(),
            typ: MetadataType::Interface(fields),
        };
        GosValue::new_meta(m, &mut objs.metas)
    }

    pub fn new_struct(fields: OrderedMembers, objs: &mut VMObjects) -> GosValue {
        let field_zeros: Vec<GosValue> = fields
            .members
            .iter()
            .map(|x| objs.metas[*x.as_meta()].zero_val().clone())
            .collect();
        let struct_val = StructObj {
            dark: false,
            meta: GosValue::Nil, // placeholder
            fields: field_zeros,
        };
        let meta = MetadataVal {
            zero_val: GosValue::Struct(Rc::new(RefCell::new(struct_val))),
            typ: MetadataType::Struct(fields),
        };
        let key = objs.metas.insert(meta);
        objs.metas[key].zero_val().as_struct().borrow_mut().meta = GosValue::Metadata(key);
        GosValue::Metadata(key)
    }

    pub fn new_channel(vtype: GosValue, objs: &mut VMObjects) -> GosValue {
        let m = MetadataVal {
            zero_val: objs.zero_val.chan_zero_val.clone(),
            typ: MetadataType::Channel(vtype),
        };
        GosValue::new_meta(m, &mut objs.metas)
    }

    pub fn new_boxed(inner: GosValue, objs: &mut VMObjects) -> GosValue {
        let m = MetadataVal {
            zero_val: objs.zero_val.boxed_zero_val.clone(),
            typ: MetadataType::Boxed(inner),
        };
        GosValue::new_meta(m, &mut objs.metas)
    }

    pub fn new_named(underlying: GosValue, objs: &mut VMObjects) -> GosValue {
        let m = MetadataVal {
            zero_val: GosValue::Nil, // placeholder
            typ: MetadataType::Named(OrderedMembers::new(vec![], HashMap::new()), underlying),
        };
        GosValue::new_meta(m, &mut objs.metas)
    }

    #[inline]
    pub fn zero_val(&self) -> &GosValue {
        &self.zero_val
    }

    #[inline]
    pub fn typ(&self) -> &MetadataType {
        &self.typ
    }

    #[inline]
    pub fn typ_mut(&mut self) -> &mut MetadataType {
        &mut self.typ
    }

    #[inline]
    pub fn sig_metadata(&self) -> &SigMetadata {
        match &self.typ {
            MetadataType::Signature(stdata) => stdata,
            _ => unreachable!(),
        }
    }

    pub fn add_method(&mut self, name: String, val: GosValue) {
        match &mut self.typ {
            MetadataType::Named(m, _) => {
                m.members.push(val);
                m.mapping.insert(name, m.members.len() as OpIndex - 1);
            }
            _ => unreachable!(),
        }
    }

    pub fn get_underlying<'a>(v: &GosValue, metas: &'a MetadataObjs) -> &'a MetadataVal {
        let meta = &metas[*v.as_meta()];
        match &meta.typ {
            MetadataType::Named(_, underlying) => &metas[*underlying.as_meta()],
            _ => meta,
        }
    }

    #[inline]
    pub fn get_value_type(v: &GosValue, metas: &MetadataObjs) -> ValueType {
        let meta = &metas[*v.as_meta()];
        match &meta.typ {
            MetadataType::None => match &meta.zero_val {
                GosValue::Bool(_) => ValueType::Bool,
                GosValue::Int(_) => ValueType::Int,
                GosValue::Float64(_) => ValueType::Float64,
                GosValue::Complex64(_, _) => ValueType::Complex64,
                GosValue::Str(_) => ValueType::Str,
                _ => unreachable!(),
            },
            MetadataType::Signature(_) => ValueType::Function,
            MetadataType::Slice(_) => ValueType::Slice,
            MetadataType::Map(_, _) => ValueType::Map,
            MetadataType::Interface(_) => ValueType::Interface,
            MetadataType::Struct(_) => ValueType::Struct,
            MetadataType::Channel(_) => ValueType::Channel,
            MetadataType::Boxed(_) => ValueType::Boxed,
            MetadataType::Named(_, u) => MetadataVal::get_value_type(u, metas),
        }
    }

    #[inline]
    pub fn get_method(v: &GosValue, index: OpIndex, metas: &MetadataObjs) -> GosValue {
        match &metas[*v.as_meta()].typ {
            MetadataType::Named(m, _) => m.members[index as usize].clone(),
            _ => unreachable!(),
        }
    }

    #[inline]
    pub fn get_all_methods(v: &GosValue, metas: &MetadataObjs) -> Vec<FunctionKey> {
        match &metas[*v.as_meta()].typ {
            MetadataType::Named(m, _) => m.members.iter().map(|x| x.as_closure().func).collect(),
            _ => unreachable!(),
        }
    }

    #[inline]
    pub fn field_index(v: &GosValue, name: &str, metas: &MetadataObjs) -> OpIndex {
        let v = if let MetadataType::Boxed(b) = metas[*v.as_meta()].typ() {
            b
        } else {
            v
        };
        let v = if let MetadataType::Named(_, u) = metas[*v.as_meta()].typ() {
            u
        } else {
            v
        };
        if let MetadataType::Struct(m) = metas[*v.as_meta()].typ() {
            m.mapping[name] as OpIndex
        } else {
            unreachable!()
        }
    }

    /// method_index returns the index of the method of a non-interface
    #[inline]
    pub fn method_index(v: &GosValue, name: &str, metas: &MetadataObjs) -> OpIndex {
        if let MetadataType::Named(m, _) = metas[*v.as_meta()].typ() {
            m.mapping[name] as OpIndex
        } else {
            unreachable!()
        }
    }

    /// iface_method_index returns the index of the method of an interface
    #[inline]
    pub fn iface_method_index(v: &GosValue, name: &str, metas: &MetadataObjs) -> OpIndex {
        if let MetadataType::Named(_, under) = metas[*v.as_meta()].typ() {
            if let MetadataType::Interface(m) = metas[*under.as_meta()].typ() {
                m.mapping[name] as OpIndex
            } else {
                unreachable!()
            }
        } else {
            unreachable!()
        }
    }
}
