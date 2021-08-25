#![macro_use]
use super::ffi::Ffi;
use super::gc::GcoVec;
use super::instruction::{Instruction, OpIndex, Opcode, ValueType};
use super::metadata::*;
use super::stack::Stack;
use super::value::{rcount_mark_and_queue, GosValue, RCQueue, RCount, RuntimeResult};
use goscript_parser::objects::{EntityKey, IdentKey};
use slotmap::{new_key_type, DenseSlotMap};
use smol::channel;
use smol::future;
use std::cell::{Cell, Ref, RefCell, RefMut};
use std::cmp::Ordering;
use std::collections::HashMap;
use std::convert::TryFrom;
use std::convert::TryInto;
use std::fmt::Write;
use std::fmt::{self, Display};
use std::hash::{Hash, Hasher};
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

pub type MetadataObjs = DenseSlotMap<MetadataKey, MetadataType>;
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
    pub metas: MetadataObjs,
    pub functions: FunctionObjs,
    pub packages: PackageObjs,
    pub metadata: Metadata,
}

impl VMObjects {
    pub fn new() -> VMObjects {
        let mut metas = DenseSlotMap::with_capacity_and_key(DEFAULT_CAPACITY);
        let md = Metadata::new(&mut metas);
        VMObjects {
            metas: metas,
            functions: DenseSlotMap::with_capacity_and_key(DEFAULT_CAPACITY),
            packages: DenseSlotMap::with_capacity_and_key(DEFAULT_CAPACITY),
            metadata: md,
        }
    }
}

// ----------------------------------------------------------------------------
// StringObj

pub type StringIter<'a> = std::str::Chars<'a>;

pub type StringEnumIter<'a> = std::iter::Enumerate<StringIter<'a>>;

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
    pub fn get_byte(&self, i: usize) -> Option<&u8> {
        self.as_str().as_bytes().get(i)
    }

    pub fn slice(&self, begin: isize, end: isize) -> StringObj {
        let self_end = self.len() as isize + 1;
        let bi = begin as usize;
        let ei = ((self_end + end) % self_end) as usize;
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
        self.as_str().cmp(other.as_str())
    }
}

// ----------------------------------------------------------------------------
// MapObj

pub type GosHashMap = HashMap<GosValue, RefCell<GosValue>>;

pub type GosHashMapIter<'a> = std::collections::hash_map::Iter<'a, GosValue, RefCell<GosValue>>;

#[derive(Debug)]
pub struct MapObj {
    pub meta: GosMetadata,
    default_val: RefCell<GosValue>,
    pub map: Option<Rc<RefCell<GosHashMap>>>,
}

impl MapObj {
    pub fn new(meta: GosMetadata, default_val: GosValue) -> MapObj {
        MapObj {
            meta: meta,
            default_val: RefCell::new(default_val),
            map: Some(Rc::new(RefCell::new(HashMap::new()))),
        }
    }

    pub fn new_nil(meta: GosMetadata, default_val: GosValue) -> MapObj {
        MapObj {
            meta: meta,
            default_val: RefCell::new(default_val),
            map: None,
        }
    }

    /// deep_clone creates a new MapObj with duplicated content of 'self.map'
    pub fn deep_clone(&self, gcos: &GcoVec) -> MapObj {
        let m = self.map.as_ref().map(|x| {
            Rc::new(RefCell::new(
                x.borrow()
                    .iter()
                    .map(|(k, v)| {
                        (
                            k.deep_clone(gcos),
                            RefCell::new(v.borrow().deep_clone(gcos)),
                        )
                    })
                    .collect(),
            ))
        });
        MapObj {
            meta: self.meta,
            default_val: self.default_val.clone(),
            map: m,
        }
    }

    #[inline]
    pub fn insert(&self, key: GosValue, val: GosValue) -> Option<GosValue> {
        self.borrow_data_mut()
            .insert(key, RefCell::new(val))
            .map(|x| x.into_inner())
    }

    #[inline]
    pub fn is_nil(&self) -> bool {
        self.map.is_none()
    }

    #[inline]
    pub fn get(&self, key: &GosValue) -> GosValue {
        let mref = self.borrow_data();
        let cell = match mref.get(key) {
            Some(v) => v,
            None => &self.default_val,
        };
        cell.clone().into_inner()
    }

    #[inline]
    pub fn try_get(&self, key: &GosValue) -> Option<GosValue> {
        let mref = self.borrow_data();
        mref.get(key).map(|x| x.clone().into_inner())
    }

    /// touch_key makes sure there is a value for the 'key', a default value is set if
    /// the value is empty
    #[inline]
    pub fn touch_key(&self, key: &GosValue) {
        if self.borrow_data().get(&key).is_none() {
            self.borrow_data_mut()
                .insert(key.clone(), self.default_val.clone());
        }
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.borrow_data().len()
    }

    #[inline]
    pub fn borrow_data_mut(&self) -> RefMut<GosHashMap> {
        self.map.as_ref().unwrap().borrow_mut()
    }

    #[inline]
    pub fn borrow_data(&self) -> Ref<GosHashMap> {
        self.map.as_ref().unwrap().borrow()
    }

    #[inline]
    pub fn clone_inner(&self) -> Rc<RefCell<GosHashMap>> {
        self.map.as_ref().unwrap().clone()
    }
}

impl Clone for MapObj {
    fn clone(&self) -> Self {
        MapObj {
            meta: self.meta,
            default_val: self.default_val.clone(),
            map: self.map.clone(),
        }
    }
}

impl PartialEq for MapObj {
    fn eq(&self, _other: &MapObj) -> bool {
        unreachable!() //false
    }
}

impl Eq for MapObj {}

impl Display for MapObj {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("map[")?;
        if let Some(m) = &self.map {
            for (i, kv) in m.borrow().iter().enumerate() {
                if i > 0 {
                    f.write_char(' ')?;
                }
                let v: &GosValue = &kv.1.borrow();
                write!(f, "{}:{}", kv.0, v)?
            }
        }
        f.write_char(']')
    }
}

// ----------------------------------------------------------------------------
// ArrayObj

pub type GosVec = Vec<RefCell<GosValue>>;

#[derive(Debug)]
pub struct ArrayObj {
    pub meta: GosMetadata,
    pub vec: Rc<RefCell<GosVec>>,
}

impl ArrayObj {
    pub fn with_size(size: usize, val: &GosValue, meta: GosMetadata, gcos: &GcoVec) -> ArrayObj {
        let mut v = GosVec::with_capacity(size);
        for _ in 0..size {
            v.push(RefCell::new(val.copy_semantic(gcos)))
        }
        ArrayObj {
            meta: meta,
            vec: Rc::new(RefCell::new(v)),
        }
    }

    pub fn with_data(val: Vec<GosValue>, meta: GosMetadata) -> ArrayObj {
        ArrayObj {
            meta: meta,
            vec: Rc::new(RefCell::new(
                val.into_iter().map(|x| RefCell::new(x)).collect(),
            )),
        }
    }

    pub fn deep_clone(&self, gcos: &GcoVec) -> ArrayObj {
        ArrayObj {
            meta: self.meta,
            vec: Rc::new(RefCell::new(
                self.borrow_data()
                    .iter()
                    .map(|x| RefCell::new(x.borrow().deep_clone(gcos)))
                    .collect(),
            )),
        }
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.borrow_data().len()
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
    pub fn get(&self, i: usize) -> Option<GosValue> {
        self.borrow_data().get(i).map(|x| x.clone().into_inner())
    }

    #[inline]
    pub fn set_from(&self, other: &ArrayObj) {
        *self.borrow_data_mut() = other.borrow_data().clone()
    }
}

impl Display for ArrayObj {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_char('[')?;
        for (i, e) in self.vec.borrow().iter().enumerate() {
            if i > 0 {
                f.write_char(' ')?;
            }
            write!(f, "{}", e.borrow())?
        }
        f.write_char(']')
    }
}

impl Hash for ArrayObj {
    fn hash<H: Hasher>(&self, state: &mut H) {
        for e in self.borrow_data().iter() {
            e.borrow().hash(state);
        }
    }
}

impl Eq for ArrayObj {}

impl PartialEq for ArrayObj {
    fn eq(&self, b: &ArrayObj) -> bool {
        if self.borrow_data().len() != b.borrow_data().len() {
            return false;
        }
        for (i, e) in self.borrow_data().iter().enumerate() {
            if e != b.borrow_data().get(i).unwrap() {
                return false;
            }
        }
        true
    }
}

impl Clone for ArrayObj {
    fn clone(&self) -> Self {
        ArrayObj {
            meta: self.meta,
            vec: self.vec.clone(),
        }
    }
}

// ----------------------------------------------------------------------------
// SliceObj

#[derive(Debug)]
pub struct SliceObj {
    pub meta: GosMetadata,
    is_nil: bool,
    begin: Cell<usize>,
    end: Cell<usize>,
    soft_cap: Cell<usize>, // <= self.vec.capacity()
    pub vec: Rc<RefCell<GosVec>>,
}

impl<'a> SliceObj {
    pub fn new(
        len: usize,
        cap: usize,
        meta: GosMetadata,
        default_val: Option<&GosValue>,
    ) -> SliceObj {
        assert!(cap >= len);
        let mut val = SliceObj {
            meta: meta,
            is_nil: false,
            begin: Cell::from(0),
            end: Cell::from(0),
            soft_cap: Cell::from(cap),
            vec: Rc::new(RefCell::new(Vec::with_capacity(cap))),
        };
        for _ in 0..len {
            val.push(default_val.unwrap().clone());
        }
        val
    }

    pub fn with_data(val: Vec<GosValue>, meta: GosMetadata) -> SliceObj {
        SliceObj {
            meta: meta,
            is_nil: false,
            begin: Cell::from(0),
            end: Cell::from(val.len()),
            soft_cap: Cell::from(val.len()),
            vec: Rc::new(RefCell::new(
                val.into_iter().map(|x| RefCell::new(x)).collect(),
            )),
        }
    }

    pub fn with_array(arr: &ArrayObj, begin: isize, end: isize) -> SliceObj {
        let elem_meta = GosMetadata::new_slice_from_array(arr.meta);
        let len = arr.len();
        let self_end = len as isize + 1;
        let bi = begin as usize;
        let ei = ((self_end + end) % self_end) as usize;
        SliceObj {
            meta: elem_meta,
            is_nil: false,
            begin: Cell::from(bi),
            end: Cell::from(ei),
            soft_cap: Cell::from(len),
            vec: arr.vec.clone(),
        }
    }

    pub fn new_nil(meta: GosMetadata) -> SliceObj {
        SliceObj {
            meta: meta,
            is_nil: true,
            begin: Cell::from(0),
            end: Cell::from(0),
            soft_cap: Cell::from(0),
            vec: Rc::new(RefCell::new(vec![])),
        }
    }

    pub fn set_from(&self, other: &SliceObj) {
        self.begin.set(other.begin());
        self.end.set(other.end());
        self.soft_cap.set(other.soft_cap());
        *self.borrow_data_mut() = other.borrow_data().clone()
    }

    /// deep_clone creates a new SliceObj with duplicated content of 'self.vec'
    pub fn deep_clone(&self, gcos: &GcoVec) -> SliceObj {
        SliceObj {
            meta: self.meta,
            is_nil: self.is_nil,
            begin: Cell::from(0),
            end: Cell::from(self.cap()),
            soft_cap: Cell::from(self.cap()),
            vec: Rc::new(RefCell::new(Vec::from_iter(
                self.vec.borrow()[self.begin()..self.end()]
                    .iter()
                    .map(|x| RefCell::new(x.borrow().deep_clone(gcos))),
            ))),
        }
    }

    #[inline]
    pub fn is_nil(&self) -> bool {
        self.is_nil
    }

    #[inline]
    pub fn begin(&self) -> usize {
        self.begin.get()
    }

    #[inline]
    pub fn end(&self) -> usize {
        self.end.get()
    }

    #[inline]
    pub fn soft_cap(&self) -> usize {
        self.soft_cap.get()
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.end() - self.begin()
    }

    #[inline]
    pub fn cap(&self) -> usize {
        self.soft_cap() - self.begin()
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
        self.borrow_data_mut().push(RefCell::new(val));
        *self.end.get_mut() += 1;
    }

    #[inline]
    pub fn append(&mut self, vals: &mut GosVec) {
        let new_len = self.len() + vals.len();
        self.try_grow_vec(new_len);
        self.borrow_data_mut().append(vals);
        *self.end.get_mut() = self.begin() + new_len;
    }

    #[inline]
    pub fn get(&self, i: usize) -> Option<GosValue> {
        self.borrow_data()
            .get(self.begin() + i)
            .map(|x| x.clone().into_inner())
    }

    #[inline]
    pub fn set(&self, i: usize, val: GosValue) {
        self.borrow_data()[self.begin() + i].replace(val);
    }

    #[inline]
    pub fn slice(&self, begin: isize, end: isize, max: isize) -> SliceObj {
        let self_len = self.len() as isize + 1;
        let self_cap = self.cap() as isize + 1;
        let bi = begin as usize;
        let ei = ((self_len + end) % self_len) as usize;
        let mi = ((self_cap + max) % self_cap) as usize;
        SliceObj {
            meta: self.meta,
            is_nil: self.is_nil,
            begin: Cell::from(self.begin() + bi),
            end: Cell::from(self.begin() + ei),
            soft_cap: Cell::from(self.begin() + mi),
            vec: self.vec.clone(),
        }
    }

    #[inline]
    pub fn get_vec(&self) -> Vec<GosValue> {
        self.borrow_data()
            .iter()
            .map(|x| x.borrow().clone())
            .collect()
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
        let mut vec = Vec::from_iter(self.borrow_data()[self.begin()..self.end()].iter().cloned());
        vec.reserve_exact(cap - vec.len());
        self.vec = Rc::new(RefCell::new(vec));
        self.begin.set(0);
        self.end.set(data_len);
        self.soft_cap.set(cap);
    }
}

impl Clone for SliceObj {
    fn clone(&self) -> Self {
        SliceObj {
            meta: self.meta,
            is_nil: self.is_nil,
            begin: self.begin.clone(),
            end: self.end.clone(),
            soft_cap: self.soft_cap.clone(),
            vec: self.vec.clone(),
        }
    }
}

impl Display for SliceObj {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_char('[')?;
        for (i, e) in self.borrow().iter().enumerate() {
            if i > 0 {
                f.write_char(' ')?;
            }
            write!(f, "{}", e.borrow())?
        }
        f.write_char(']')
    }
}

pub struct SliceRef<'a> {
    vec_ref: Ref<'a, GosVec>,
    begin: usize,
    end: usize,
}

pub type SliceIter<'a> = std::slice::Iter<'a, RefCell<GosValue>>;

pub type SliceEnumIter<'a> = std::iter::Enumerate<SliceIter<'a>>;

impl<'a> SliceRef<'a> {
    pub fn new(s: &SliceObj) -> SliceRef {
        SliceRef {
            vec_ref: s.borrow_data(),
            begin: s.begin(),
            end: s.end(),
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
    pub meta: GosMetadata,
    pub fields: Vec<GosValue>,
}

impl StructObj {
    pub fn deep_clone(&self, gcos: &GcoVec) -> StructObj {
        StructObj {
            meta: self.meta,
            fields: Vec::from_iter(self.fields.iter().map(|x| x.deep_clone(gcos))),
        }
    }
}

impl Eq for StructObj {}

impl PartialEq for StructObj {
    #[inline]
    fn eq(&self, other: &StructObj) -> bool {
        for (i, f) in self.fields.iter().enumerate() {
            if f != &other.fields[i] {
                return false;
            }
        }
        return true;
    }
}

impl Hash for StructObj {
    #[inline]
    fn hash<H: Hasher>(&self, state: &mut H) {
        for f in self.fields.iter() {
            f.hash(state)
        }
    }
}

impl Display for StructObj {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_char('{')?;
        for (i, fld) in self.fields.iter().enumerate() {
            if i > 0 {
                f.write_char(' ')?;
            }
            write!(f, "{}", fld)?
        }
        f.write_char('}')
    }
}

// ----------------------------------------------------------------------------
// InterfaceObj

#[derive(Clone, Debug)]
pub struct UnderlyingFfi {
    pub ffi_obj: Rc<RefCell<dyn Ffi>>,
    pub methods: Vec<(String, GosMetadata)>,
}

impl UnderlyingFfi {
    pub fn new(obj: Rc<RefCell<dyn Ffi>>, methods: Vec<(String, GosMetadata)>) -> UnderlyingFfi {
        UnderlyingFfi {
            ffi_obj: obj,
            methods: methods,
        }
    }
}

#[derive(Clone, Debug)]
pub enum IfaceUnderlying {
    None,
    Gos(GosValue, Rc<Vec<FunctionKey>>),
    Ffi(UnderlyingFfi),
}

impl Eq for IfaceUnderlying {}

impl PartialEq for IfaceUnderlying {
    #[inline]
    fn eq(&self, other: &IfaceUnderlying) -> bool {
        match (self, other) {
            (Self::None, Self::None) => true,
            (Self::Gos(x, _), Self::Gos(y, _)) => x == y,
            (Self::Ffi(x), Self::Ffi(y)) => Rc::ptr_eq(&x.ffi_obj, &y.ffi_obj),
            _ => false,
        }
    }
}

#[derive(Clone, Debug)]
pub struct InterfaceObj {
    pub meta: GosMetadata,
    // the Named object behind the interface
    // mapping from interface's methods to object's methods
    underlying: IfaceUnderlying,
}

impl InterfaceObj {
    pub fn new(meta: GosMetadata, underlying: IfaceUnderlying) -> InterfaceObj {
        InterfaceObj {
            meta: meta,
            underlying: underlying,
        }
    }

    #[inline]
    pub fn underlying(&self) -> &IfaceUnderlying {
        &self.underlying
    }

    #[inline]
    pub fn set_underlying(&mut self, v: IfaceUnderlying) {
        self.underlying = v;
    }

    #[inline]
    pub fn underlying_value(&self) -> Option<&GosValue> {
        match self.underlying() {
            IfaceUnderlying::Gos(v, _) => Some(v),
            _ => None,
        }
    }

    #[inline]
    pub fn is_nil(&self) -> bool {
        self.underlying() == &IfaceUnderlying::None
    }

    /// for gc
    pub fn ref_sub_one(&self) {
        match self.underlying() {
            IfaceUnderlying::Gos(v, _) => v.ref_sub_one(),
            _ => {}
        };
    }

    /// for gc
    pub fn mark_dirty(&self, queue: &mut RCQueue) {
        match self.underlying() {
            IfaceUnderlying::Gos(v, _) => v.mark_dirty(queue),
            _ => {}
        };
    }
}

impl Eq for InterfaceObj {}

impl PartialEq for InterfaceObj {
    #[inline]
    fn eq(&self, other: &InterfaceObj) -> bool {
        self.underlying() == other.underlying()
    }
}

impl Hash for InterfaceObj {
    #[inline]
    fn hash<H: Hasher>(&self, state: &mut H) {
        match self.underlying() {
            IfaceUnderlying::Gos(v, _) => v.hash(state),
            IfaceUnderlying::Ffi(ffi) => Rc::as_ptr(&ffi.ffi_obj).hash(state),
            IfaceUnderlying::None => 0.hash(state),
        }
    }
}

impl Display for InterfaceObj {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.underlying() {
            IfaceUnderlying::Gos(v, _) => write!(f, "{}", v),
            IfaceUnderlying::Ffi(ffi) => write!(f, "<ffi>{:?}", ffi.ffi_obj.borrow()),
            IfaceUnderlying::None => f.write_str("<nil>"),
        }
    }
}

/* next todo:
 support zero cap buffer
 remove interface from gc
 recv comma_ok
 real zero value RECV
 fix gc
*/

// ----------------------------------------------------------------------------
// ChannelObj

#[derive(Clone, Debug)]
pub struct ChannelObj {
    pub meta: GosMetadata,
    // we can use Option here for send_only/recv_only channels, but would it help?
    pub sender: channel::Sender<GosValue>,
    pub receiver: channel::Receiver<GosValue>,
}

impl ChannelObj {
    pub fn new(meta: GosMetadata, cap: usize) -> ChannelObj {
        let (s, r) = channel::bounded(cap);
        ChannelObj {
            meta: meta,
            sender: s,
            receiver: r,
        }
    }

    pub async fn send(&self, v: GosValue) -> RuntimeResult {
        loop {
            let re = self.sender.try_send(v.clone());
            if re.is_ok() {
                return Ok(());
            } else {
                match re.unwrap_err() {
                    channel::TrySendError::Full(_) => {
                        future::yield_now().await;
                    }
                    channel::TrySendError::Closed(_) => {
                        return Err("channel closed!".to_string());
                    }
                }
            }
        }
    }

    pub async fn recv(&self) -> GosValue {
        loop {
            let re = self.receiver.try_recv();
            if re.is_ok() {
                return re.unwrap();
            } else {
                match re.unwrap_err() {
                    channel::TryRecvError::Empty => {
                        future::yield_now().await;
                    }
                    // todo: real zero value
                    channel::TryRecvError::Closed => return GosValue::new_nil(),
                }
            }
        }
    }
}

// ----------------------------------------------------------------------------
// PointerObj
/// Logically there are 4 types of pointers, they point to:
/// - local
/// - slice member
/// - struct field
/// - package member
/// and for pointers to locals, the default way of handling it is to use "UpValue"
/// (PointerObj::UpVal). Struct/Map/Slice are optimizations for this type, when
/// the pointee has a "real" pointer
///

#[derive(Debug, Clone)]
pub enum PointerObj {
    Released,
    UpVal(UpValue),
    Struct(Rc<(RefCell<StructObj>, RCount)>, GosMetadata),
    Array(Rc<(ArrayObj, RCount)>, GosMetadata),
    Slice(Rc<(SliceObj, RCount)>, GosMetadata),
    Map(Rc<(MapObj, RCount)>, GosMetadata),
    SliceMember(Rc<(SliceObj, RCount)>, OpIndex),
    StructField(Rc<(RefCell<StructObj>, RCount)>, OpIndex),
    PkgMember(PackageKey, OpIndex),
}

impl PointerObj {
    #[inline]
    pub fn new_local(val: GosValue) -> PointerObj {
        match val {
            GosValue::Named(s) => match &s.0 {
                GosValue::Struct(stru) => PointerObj::Struct(stru.clone(), s.1),
                GosValue::Array(arr) => PointerObj::Array(arr.clone(), s.1),
                GosValue::Slice(slice) => PointerObj::Slice(slice.clone(), s.1),
                GosValue::Map(map) => PointerObj::Map(map.clone(), s.1),
                _ => {
                    dbg!(s);
                    unreachable!()
                }
            },
            GosValue::Struct(s) => PointerObj::Struct(s.clone(), GosMetadata::Untyped),
            GosValue::Array(a) => PointerObj::Array(a.clone(), GosMetadata::Untyped),
            GosValue::Slice(s) => PointerObj::Slice(s.clone(), GosMetadata::Untyped),
            GosValue::Map(m) => PointerObj::Map(m.clone(), GosMetadata::Untyped),
            _ => {
                dbg!(val);
                unreachable!()
            }
        }
    }

    #[inline]
    pub fn set_local_ref_type(&self, val: GosValue) {
        match self {
            Self::Struct(v, _) => {
                let mref: &mut StructObj = &mut v.0.borrow_mut();
                *mref = val.try_get_struct().unwrap().0.borrow().clone();
            }
            _ => unreachable!(),
        }
    }

    pub fn deep_clone(&self, gcos: &GcoVec) -> PointerObj {
        match &self {
            PointerObj::Released => PointerObj::Released,
            PointerObj::Struct(s, m) => PointerObj::Struct(
                Rc::new((RefCell::new(s.0.borrow().deep_clone(gcos)), Cell::new(0))),
                *m,
            ),
            PointerObj::Slice(s, m) => {
                PointerObj::Slice(Rc::new((s.0.deep_clone(gcos), Cell::new(0))), *m)
            }
            PointerObj::Map(map, m) => {
                PointerObj::Map(Rc::new((map.0.deep_clone(gcos), Cell::new(0))), *m)
            }
            _ => unreachable!(),
        }
    }

    /// for gc
    pub fn ref_sub_one(&self) {
        match &self {
            PointerObj::UpVal(uv) => uv.ref_sub_one(),
            PointerObj::Struct(s, _) => s.1.set(s.1.get() - 1),
            PointerObj::Slice(s, _) => s.1.set(s.1.get() - 1),
            PointerObj::Map(s, _) => s.1.set(s.1.get() - 1),
            PointerObj::SliceMember(s, _) => s.1.set(s.1.get() - 1),
            PointerObj::StructField(s, _) => s.1.set(s.1.get() - 1),
            _ => {}
        };
    }

    /// for gc
    pub fn mark_dirty(&self, queue: &mut RCQueue) {
        match &self {
            PointerObj::UpVal(uv) => uv.mark_dirty(queue),
            PointerObj::Struct(s, _) => rcount_mark_and_queue(&s.1, queue),
            PointerObj::Slice(s, _) => rcount_mark_and_queue(&s.1, queue),
            PointerObj::Map(s, _) => rcount_mark_and_queue(&s.1, queue),
            PointerObj::SliceMember(s, _) => rcount_mark_and_queue(&s.1, queue),
            PointerObj::StructField(s, _) => rcount_mark_and_queue(&s.1, queue),
            _ => {}
        };
    }
}

impl Eq for PointerObj {}

impl PartialEq for PointerObj {
    #[inline]
    fn eq(&self, other: &PointerObj) -> bool {
        match (self, other) {
            (Self::UpVal(x), Self::UpVal(y)) => x == y,
            (Self::Struct(x, _), Self::Struct(y, _)) => x == y,
            (Self::Array(x, _), Self::Array(y, _)) => x == y,
            (Self::Slice(x, _), Self::Slice(y, _)) => x == y,
            (Self::Map(x, _), Self::Map(y, _)) => x == y,
            (Self::SliceMember(x, ix), Self::SliceMember(y, iy)) => Rc::ptr_eq(x, y) && ix == iy,
            (Self::StructField(x, ix), Self::StructField(y, iy)) => Rc::ptr_eq(x, y) && ix == iy,
            (Self::PkgMember(ka, ix), Self::PkgMember(kb, iy)) => ka == kb && ix == iy,
            _ => false,
        }
    }
}

impl Hash for PointerObj {
    fn hash<H: Hasher>(&self, state: &mut H) {
        match self {
            Self::UpVal(x) => x.hash(state),
            Self::Struct(s, _) => Rc::as_ptr(s).hash(state),
            Self::Array(s, _) => Rc::as_ptr(s).hash(state),
            Self::Slice(s, _) => Rc::as_ptr(s).hash(state),
            Self::Map(s, _) => Rc::as_ptr(s).hash(state),
            Self::SliceMember(s, index) => {
                Rc::as_ptr(s).hash(state);
                index.hash(state);
            }
            Self::StructField(s, index) => {
                Rc::as_ptr(s).hash(state);
                index.hash(state);
            }
            Self::PkgMember(p, index) => {
                p.hash(state);
                index.hash(state);
            }
            Self::Released => unreachable!(),
        }
    }
}

impl Display for PointerObj {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UpVal(uv) => f.write_fmt(format_args!("{:p}", Rc::as_ptr(&uv.inner))),
            Self::Struct(s, _) => f.write_fmt(format_args!("{:p}", Rc::as_ptr(&s))),
            Self::Array(s, _) => f.write_fmt(format_args!("{:p}", Rc::as_ptr(&s))),
            Self::Slice(s, _) => f.write_fmt(format_args!("{:p}", Rc::as_ptr(&s))),
            Self::Map(m, _) => f.write_fmt(format_args!("{:p}", Rc::as_ptr(&m))),
            Self::SliceMember(s, i) => f.write_fmt(format_args!("{:p}i{}", Rc::as_ptr(&s), i)),
            Self::StructField(s, i) => f.write_fmt(format_args!("{:p}i{}", Rc::as_ptr(&s), i)),
            Self::PkgMember(p, i) => f.write_fmt(format_args!("{:x}i{}", key_to_u64(*p), i)),
            Self::Released => f.write_str("released!!!"),
        }
    }
}

// ----------------------------------------------------------------------------
// ClosureObj

#[derive(Clone, Debug)]
pub struct ValueDesc {
    pub func: FunctionKey,
    pub index: OpIndex,
    pub typ: ValueType,
    pub is_up_value: bool,
    pub stack: Weak<RefCell<Stack>>,
    pub stack_base: OpIndex,
}

impl Eq for ValueDesc {}

impl PartialEq for ValueDesc {
    #[inline]
    fn eq(&self, other: &ValueDesc) -> bool {
        self.index == other.index
    }
}

impl ValueDesc {
    pub fn new(func: FunctionKey, index: OpIndex, typ: ValueType, is_up_value: bool) -> ValueDesc {
        ValueDesc {
            func: func,
            index: index,
            typ: typ,
            is_up_value: is_up_value,
            stack: Weak::new(),
            stack_base: 0,
        }
    }

    pub fn clone_with_stack(&self, stack: Weak<RefCell<Stack>>, stack_base: OpIndex) -> ValueDesc {
        ValueDesc {
            func: self.func,
            index: self.index,
            typ: self.typ,
            is_up_value: self.is_up_value,
            stack: stack,
            stack_base: stack_base,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UpValueState {
    /// Parent CallFrame is still alive, pointing to a local variable
    Open(ValueDesc), // (what func is the var defined, the index of the var)
    // Parent CallFrame is released, pointing to a pointer value in the global pool
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

    pub fn new_closed(v: GosValue) -> UpValue {
        UpValue {
            inner: Rc::new(RefCell::new(UpValueState::Closed(v))),
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

    /// for gc
    pub fn ref_sub_one(&self) {
        let state: &UpValueState = &self.inner.borrow();
        if let UpValueState::Closed(uvs) = state {
            uvs.ref_sub_one()
        }
    }

    /// for gc
    pub fn mark_dirty(&self, queue: &mut RCQueue) {
        let state: &UpValueState = &self.inner.borrow();
        if let UpValueState::Closed(uvs) = state {
            uvs.mark_dirty(queue)
        }
    }
}

impl Hash for UpValue {
    #[inline]
    fn hash<H: Hasher>(&self, state: &mut H) {
        let b: &UpValueState = &self.inner.borrow();
        match b {
            UpValueState::Open(desc) => desc.index.hash(state),
            UpValueState::Closed(_) => Rc::as_ptr(&self.inner).hash(state),
        }
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

#[derive(Clone, Debug)]
pub struct FfiClosureObj {
    pub ffi: Rc<RefCell<dyn Ffi>>,
    pub func_name: String,
    pub meta: GosMetadata,
}

#[derive(Clone, Debug)]
pub struct ClosureObj {
    pub func: Option<FunctionKey>,
    pub uvs: Option<HashMap<usize, UpValue>>,
    pub recv: Option<GosValue>,

    pub ffi: Option<Box<FfiClosureObj>>,

    pub meta: GosMetadata,
}

impl ClosureObj {
    pub fn new_gos(key: FunctionKey, fobjs: &FunctionObjs, recv: Option<GosValue>) -> ClosureObj {
        let func = &fobjs[key];
        let uvs: Option<HashMap<usize, UpValue>> = if func.up_ptrs.len() > 0 {
            Some(
                func.up_ptrs
                    .iter()
                    .enumerate()
                    .filter(|(_, x)| x.is_up_value)
                    .map(|(i, x)| (i, UpValue::new(x.clone())))
                    .collect(),
            )
        } else {
            None
        };
        ClosureObj {
            func: Some(key),
            uvs: uvs,
            recv: recv,
            ffi: None,
            meta: func.meta,
        }
    }

    #[inline]
    pub fn new_ffi(ffi: FfiClosureObj) -> ClosureObj {
        let m = ffi.meta;
        ClosureObj {
            func: None,
            uvs: None,
            recv: None,
            ffi: Some(Box::new(ffi)),
            meta: m,
        }
    }

    /// for gc
    pub fn ref_sub_one(&self) {
        if self.func.is_some() {
            if let Some(uvs) = &self.uvs {
                for (_, v) in uvs.iter() {
                    v.ref_sub_one()
                }
            }
            if let Some(recv) = &self.recv {
                recv.ref_sub_one()
            }
        }
    }

    /// for gc
    pub fn mark_dirty(&self, queue: &mut RCQueue) {
        if self.func.is_some() {
            if let Some(uvs) = &self.uvs {
                for (_, v) in uvs.iter() {
                    v.mark_dirty(queue)
                }
            }
            if let Some(recv) = &self.recv {
                recv.mark_dirty(queue)
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
    members: Vec<Rc<RefCell<GosValue>>>, // imports, const, var, func are all stored here
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
        self.members.push(Rc::new(RefCell::new(val)));
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

    pub fn var_mut(&self, fn_member_index: OpIndex) -> RefMut<GosValue> {
        let index = self.var_mapping.as_ref().unwrap()[&fn_member_index];
        self.members[index as usize].borrow_mut()
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
    pub fn member(&self, i: OpIndex) -> Ref<GosValue> {
        self.members[i as usize].borrow()
    }

    #[inline]
    pub fn member_mut(&self, i: OpIndex) -> RefMut<GosValue> {
        self.members[i as usize].borrow_mut()
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
    PackageMember(PackageKey, IdentKey),
    BuiltInVal(Opcode), // built-in identifiers
    BuiltInType(GosMetadata),
    Blank,
}

impl From<EntIndex> for OpIndex {
    fn from(t: EntIndex) -> OpIndex {
        match t {
            EntIndex::Const(i) => i,
            EntIndex::LocalVar(i) => i,
            EntIndex::UpValue(i) => i,
            EntIndex::PackageMember(_, _) => unreachable!(),
            EntIndex::BuiltInVal(_) => unreachable!(),
            EntIndex::BuiltInType(_) => unreachable!(),
            EntIndex::Blank => unreachable!(),
        }
    }
}

/// FunctionVal is the direct container of the Opcode.
#[derive(Clone, Debug)]
pub struct FunctionVal {
    pub package: PackageKey,
    pub meta: GosMetadata,
    code: Vec<Instruction>,
    pos: Vec<Option<usize>>,
    pub consts: Vec<GosValue>,
    pub up_ptrs: Vec<ValueDesc>,

    pub ret_zeros: Vec<GosValue>,
    pub local_zeros: Vec<GosValue>,

    param_count: usize,
    entities: HashMap<EntityKey, EntIndex>,
    uv_entities: HashMap<EntityKey, EntIndex>,
    local_alloc: u16,
    is_ctor: bool,
}

impl FunctionVal {
    pub fn new(
        package: PackageKey,
        meta: GosMetadata,
        objs: &VMObjects,
        gcv: &GcoVec,
        ctor: bool,
    ) -> FunctionVal {
        match &objs.metas[meta.as_non_ptr()] {
            MetadataType::Signature(s) => {
                let mut returns = vec![];
                for m in s.results.iter() {
                    returns.push(zero_val!(m, objs, gcv));
                }
                let params = s.params.len() + s.recv.map_or(0, |_| 1);
                FunctionVal {
                    package: package,
                    meta: meta,
                    code: Vec::new(),
                    pos: Vec::new(),
                    consts: Vec::new(),
                    up_ptrs: Vec::new(),
                    ret_zeros: returns,
                    local_zeros: Vec::new(),
                    param_count: params,
                    entities: HashMap::new(),
                    uv_entities: HashMap::new(),
                    local_alloc: 0,
                    is_ctor: ctor,
                }
            }
            _ => unreachable!(),
        }
    }

    #[inline]
    pub fn code(&self) -> &Vec<Instruction> {
        &self.code
    }

    #[inline]
    pub fn instruction_mut(&mut self, i: usize) -> &mut Instruction {
        self.code.get_mut(i).unwrap()
    }

    #[inline]
    pub fn pos(&self) -> &Vec<Option<usize>> {
        &self.pos
    }

    #[inline]
    pub fn param_count(&self) -> usize {
        self.param_count
    }

    #[inline]
    pub fn ret_count(&self) -> usize {
        self.ret_zeros.len()
    }

    #[inline]
    pub fn is_ctor(&self) -> bool {
        self.is_ctor
    }

    #[inline]
    pub fn local_count(&self) -> usize {
        self.local_alloc as usize - self.param_count() - self.ret_count()
    }

    #[inline]
    pub fn entity_index(&self, entity: &EntityKey) -> Option<&EntIndex> {
        self.entities.get(entity)
    }

    #[inline]
    pub fn const_val(&self, index: OpIndex) -> &GosValue {
        &self.consts[index as usize]
    }

    #[inline]
    pub fn offset(&self, loc: usize) -> OpIndex {
        // todo: don't crash if OpIndex overflows
        OpIndex::try_from((self.code.len() - loc) as isize).unwrap()
    }

    #[inline]
    pub fn next_code_index(&self) -> usize {
        self.code.len()
    }

    #[inline]
    pub fn push_inst_pos(&mut self, i: Instruction, pos: Option<usize>) {
        self.code.push(i);
        self.pos.push(pos);
    }

    #[inline]
    pub fn emit_inst(
        &mut self,
        op: Opcode,
        types: [Option<ValueType>; 3],
        imm: Option<i32>,
        pos: Option<usize>,
    ) {
        let i = Instruction::new(op, types[0], types[1], types[2], imm);
        self.code.push(i);
        self.pos.push(pos);
    }

    pub fn emit_raw_inst(&mut self, u: u64, pos: Option<usize>) {
        let i = Instruction::from_u64(u);
        self.code.push(i);
        self.pos.push(pos);
    }

    pub fn emit_code_with_type(&mut self, code: Opcode, t: ValueType, pos: Option<usize>) {
        self.emit_inst(code, [Some(t), None, None], None, pos);
    }

    pub fn emit_code_with_type2(
        &mut self,
        code: Opcode,
        t0: ValueType,
        t1: Option<ValueType>,
        pos: Option<usize>,
    ) {
        self.emit_inst(code, [Some(t0), t1, None], None, pos);
    }

    pub fn emit_code_with_imm(&mut self, code: Opcode, imm: OpIndex, pos: Option<usize>) {
        self.emit_inst(code, [None, None, None], Some(imm), pos);
    }

    pub fn emit_code_with_type_imm(
        &mut self,
        code: Opcode,
        t: ValueType,
        imm: OpIndex,
        pos: Option<usize>,
    ) {
        self.emit_inst(code, [Some(t), None, None], Some(imm), pos);
    }

    pub fn emit_code_with_flag_imm(
        &mut self,
        code: Opcode,
        comma_ok: bool,
        imm: OpIndex,
        pos: Option<usize>,
    ) {
        let mut inst = Instruction::new(code, None, None, None, Some(imm));
        let flag = if comma_ok { 1 } else { 0 };
        inst.set_t2_with_index(flag);
        self.code.push(inst);
        self.pos.push(pos);
    }

    pub fn emit_code(&mut self, code: Opcode, pos: Option<usize>) {
        self.emit_inst(code, [None, None, None], None, pos);
    }

    /// returns the index of the const if it's found
    pub fn get_const_index(&self, val: &GosValue) -> Option<EntIndex> {
        self.consts.iter().enumerate().find_map(|(i, x)| {
            if val.identical(x) {
                Some(EntIndex::Const(i as OpIndex))
            } else {
                None
            }
        })
    }

    pub fn add_local(&mut self, entity: Option<EntityKey>) -> EntIndex {
        let result = self.local_alloc as OpIndex;
        if let Some(key) = entity {
            let old = self.entities.insert(key, EntIndex::LocalVar(result));
            assert_eq!(old, None);
        };
        self.local_alloc += 1;
        EntIndex::LocalVar(result)
    }

    pub fn add_local_zero(&mut self, zero: GosValue) {
        self.local_zeros.push(zero)
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
        match self.uv_entities.get(entity) {
            Some(i) => *i,
            None => self.add_upvalue(entity, uv),
        }
    }

    fn add_upvalue(&mut self, entity: &EntityKey, uv: ValueDesc) -> EntIndex {
        self.up_ptrs.push(uv);
        let i = (self.up_ptrs.len() - 1).try_into().unwrap();
        let et = EntIndex::UpValue(i);
        self.uv_entities.insert(*entity, et);
        et
    }
}
