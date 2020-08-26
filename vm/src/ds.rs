use super::opcode::{CodeData, OpIndex, Opcode};
use super::value::{FunctionKey, GosValue, MetaKey, MetadataType, PackageKey, VMObjects};
use std::cell::{Ref, RefCell, RefMut};
use std::cmp::Ordering;
use std::collections::HashMap;
use std::convert::TryInto;
use std::fmt;
use std::hash::{Hash, Hasher};
use std::iter::FromIterator;
use std::rc::Rc;

use goscript_parser::objects::EntityKey;

// ----------------------------------------------------------------------------
// StringVal

pub type StringIter<'a> = std::iter::Take<std::iter::Skip<std::str::Chars<'a>>>;

pub type StringEnumIter<'a> =
    std::iter::Enumerate<std::iter::Take<std::iter::Skip<std::str::Chars<'a>>>>;

#[derive(Clone, Debug)]
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
        self.data.as_ref().as_bytes()[self.begin + i]
    }

    pub fn slice(&self, begin: Option<usize>, end: Option<usize>) -> StringVal {
        let self_len = self.len();
        let bi = begin.unwrap_or(0);
        let ei = end.unwrap_or(self_len);
        assert!(bi < self_len);
        assert!(bi <= ei && ei <= self_len);
        StringVal {
            data: self.data.clone(),
            begin: bi,
            end: ei,
        }
    }

    pub fn iter(&self) -> StringIter {
        self.data
            .chars()
            .skip(self.begin)
            .take(self.end - self.begin)
    }
}

impl PartialEq for StringVal {
    fn eq(&self, other: &StringVal) -> bool {
        self.as_str().eq(other.as_str())
    }
}

impl Eq for StringVal {}

impl PartialOrd for StringVal {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for StringVal {
    fn cmp(&self, other: &Self) -> Ordering {
        dbg!(self.as_str());
        dbg!(other.as_str());
        self.as_str().cmp(other.as_str())
    }
}

// ----------------------------------------------------------------------------
// MapVal

#[derive(Clone)]
pub struct HashKey {
    pub val: GosValue,
    // needed for accessing managed data like GosValue::Str
    pub objs: *const VMObjects,
}

impl HashKey {
    fn objs(&self) -> &VMObjects {
        // There is no way around it, if you what the Hash trait implemented
        // because managed data like GosValue::Str needs a ref to VMObjects
        unsafe { self.objs.as_ref().unwrap() }
    }
}

impl Eq for HashKey {}

impl PartialEq for HashKey {
    fn eq(&self, other: &HashKey) -> bool {
        self.val.eq(&other.val, self.objs())
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
            GosValue::Str(s) => s.data.hash(state),
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

pub type GosHashMap = HashMap<HashKey, RefCell<GosValue>>;

#[derive(Clone)]
pub struct MapVal {
    pub dark: bool,
    objs: *const VMObjects,
    default_val: RefCell<GosValue>,
    // Why not Rc or no wrapper at all?
    // - Sometimes we need to copy it out of VMObjects, so we need a wapper
    // - If the underlying data is not read-only, you cannot simply use Rc
    map: Rc<RefCell<GosHashMap>>,
}

impl fmt::Debug for MapVal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MapVal")
            .field("dark", &self.dark)
            .field("default_val", &self.default_val)
            .field("map", &self.map)
            .finish()
    }
}

impl MapVal {
    pub fn new(objs: &VMObjects, default_val: GosValue) -> MapVal {
        MapVal {
            dark: false,
            objs: objs,
            default_val: RefCell::new(default_val),
            map: Rc::new(RefCell::new(HashMap::new())),
        }
    }

    /// deep_clone creates a new MapVal with duplicated content of 'self.map'
    pub fn deep_clone(&self) -> MapVal {
        MapVal {
            dark: false,
            objs: self.objs,
            default_val: self.default_val.clone(),
            map: Rc::new(RefCell::new(self.map.borrow().clone())),
        }
    }

    #[inline]
    pub fn insert(&mut self, key: GosValue, val: GosValue) -> Option<GosValue> {
        let hk = self.hash_key(key);
        self.map
            .borrow_mut()
            .insert(hk, RefCell::new(val))
            .map(|x| x.into_inner())
    }

    #[inline]
    pub fn get(&self, key: &GosValue) -> GosValue {
        let hk = HashKey {
            val: key.clone(),
            objs: self.objs,
        };
        let mref = self.map.borrow();
        let cell = match mref.get(&hk) {
            Some(v) => v,
            None => &self.default_val,
        };
        cell.clone().into_inner()
    }

    #[inline]
    pub fn hash_key(&self, key: GosValue) -> HashKey {
        HashKey {
            val: key,
            objs: self.objs,
        }
    }

    /// touch_key makes sure there is a value for the 'key', a default value is set if
    /// the value is empty
    #[inline]
    pub fn touch_key(&self, key: &GosValue) {
        let hk = HashKey {
            val: key.clone(),
            objs: self.objs,
        };
        if self.map.borrow().get(&hk).is_none() {
            self.map.borrow_mut().insert(hk, self.default_val.clone());
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
    pub fn clone_data(&self) -> Rc<RefCell<GosHashMap>> {
        self.map.clone()
    }
}

// ----------------------------------------------------------------------------
// SliceVal

pub type GosVec = Vec<RefCell<GosValue>>;

#[derive(Clone, Debug)]
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
            vec: self.vec.clone(),
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

pub struct SliceRef<'a> {
    vec_ref: Ref<'a, GosVec>,
    begin: usize,
    end: usize,
}

pub type SliceIter<'a> = std::iter::Take<std::iter::Skip<std::slice::Iter<'a, RefCell<GosValue>>>>;

pub type SliceEnumIter<'a> =
    std::iter::Enumerate<std::iter::Take<std::iter::Skip<std::slice::Iter<'a, RefCell<GosValue>>>>>;

impl<'a> SliceRef<'a> {
    pub fn new(s: &SliceVal) -> SliceRef {
        SliceRef {
            vec_ref: s.vec.borrow(),
            begin: s.begin,
            end: s.end,
        }
    }

    pub fn iter(&self) -> SliceIter {
        self.vec_ref
            .iter()
            .skip(self.begin)
            .take(self.end - self.begin)
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
    pub meta: MetaKey,
    pub fields: Vec<GosValue>,
}

impl StructVal {
    pub fn field_method_index(&self, name: &str, objs: &VMObjects) -> OpIndex {
        let t = &objs.metas[self.meta];
        if let MetadataType::Struct(_, map) = &t.typ {
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
    pub meta: MetaKey,
    pub obj: GosValue,             // the Named object behind the interface
    pub obj_type: Option<MetaKey>, // the type of the named object
    pub mapping: Vec<OpIndex>,     // mapping from interface's methods to object's methods
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
    objs: *const VMObjects,
    pub package: PackageKey,
    pub meta: MetaKey,
    pub code: Vec<CodeData>,
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
    pub fn new(
        objs: &VMObjects,
        package: PackageKey,
        meta: MetaKey,
        variadic: bool,
        ctor: bool,
    ) -> FunctionVal {
        FunctionVal {
            objs: objs,
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

    pub fn objs(&self) -> &VMObjects {
        unsafe { &*self.objs }
    }

    pub fn entity_index(&self, entity: &EntityKey) -> Option<&EntIndex> {
        self.entities.get(entity)
    }

    pub fn const_val(&self, index: OpIndex) -> &GosValue {
        &self.consts[index as usize]
    }

    pub fn emit_code(&mut self, code: Opcode) {
        self.code.push(CodeData::Code(code));
    }

    pub fn emit_data(&mut self, data: OpIndex) {
        self.code.push(CodeData::Data(data));
    }

    /// returns the index of the const if it's found
    pub fn get_const_index(&self, val: &GosValue) -> Option<EntIndex> {
        self.consts.iter().enumerate().find_map(|(i, x)| {
            if val.eq(x, self.objs()) {
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

#[cfg(test)]
mod test {
    use super::super::value::*;
    use super::*;
    use std::mem;

    #[test]
    fn test_size() {
        dbg!(mem::size_of::<HashMap<HashKey, GosValue>>());
        dbg!(mem::size_of::<String>());
        dbg!(mem::size_of::<Rc<String>>());
        dbg!(mem::size_of::<SliceVal>());
        dbg!(mem::size_of::<RefCell<GosValue>>());
        dbg!(mem::size_of::<GosValue>());
        dbg!(mem::size_of::<SliceKey>());

        let mut h: HashMap<isize, isize> = HashMap::new();
        h.insert(0, 1);
        let mut h2 = h.clone();
        h2.insert(0, 3);
        dbg!(h[&0]);
        dbg!(h2[&0]);
    }
}
