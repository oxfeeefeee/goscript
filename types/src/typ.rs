#![allow(dead_code)]
use super::obj::{LangObj, ObjSet};
use super::objects::{ObjKey, ScopeKey, TCObjects, TypeKey};
use std::cell::{Ref, RefCell, RefMut};
use std::collections::HashSet;
use std::fmt;
use std::fmt::Debug;
use std::fmt::Write;
use std::mem::size_of as std_size_of;
use std::rc::Rc;

macro_rules! compare_by_method_name {
    ($objs:expr) => {{
        |a, b| {
            let oa = &$objs.lobjs[*a];
            let ob = &$objs.lobjs[*b];
            oa.id($objs).as_ref().cmp(ob.id($objs).as_ref())
        }
    }};
}

macro_rules! compare_by_type_name {
    ($objs:expr) => {{
        |a, b| {
            let sort_name = |t: &Type| match t {
                Type::Named(n) => {
                    let typ = &$objs.lobjs[n.obj().unwrap()];
                    typ.id($objs)
                }
                _ => "".into(),
            };
            let ta = sort_name(&$objs.types[*a]);
            let tb = sort_name(&$objs.types[*b]);
            ta.as_ref().cmp(tb.as_ref())
        }
    }};
}

#[derive(Debug)]
pub enum Type {
    Basic(BasicDetail),
    Array(ArrayDetail),
    Slice(SliceDetail),
    Struct(StructDetail),
    Pointer(PointerDetail),
    Tuple(TupleDetail),
    Signature(SignatureDetail),
    Interface(InterfaceDetail),
    Map(MapDetail),
    Chan(ChanDetail),
    Named(NamedDetail),
}

impl Type {
    pub fn try_as_basic(&self) -> Option<&BasicDetail> {
        match &self {
            Type::Basic(b) => Some(b),
            _ => None,
        }
    }

    pub fn try_as_array(&self) -> Option<&ArrayDetail> {
        match &self {
            Type::Array(a) => Some(a),
            _ => None,
        }
    }

    pub fn try_as_array_mut(&mut self) -> Option<&mut ArrayDetail> {
        match self {
            Type::Array(a) => Some(a),
            _ => None,
        }
    }

    pub fn try_as_slice(&self) -> Option<&SliceDetail> {
        match &self {
            Type::Slice(s) => Some(s),
            _ => None,
        }
    }

    pub fn try_as_struct(&self) -> Option<&StructDetail> {
        match &self {
            Type::Struct(s) => Some(s),
            _ => None,
        }
    }

    pub fn try_as_pointer(&self) -> Option<&PointerDetail> {
        match &self {
            Type::Pointer(p) => Some(p),
            _ => None,
        }
    }

    pub fn try_as_tuple(&self) -> Option<&TupleDetail> {
        match self {
            Type::Tuple(t) => Some(t),
            _ => None,
        }
    }

    pub fn try_as_tuple_mut(&mut self) -> Option<&mut TupleDetail> {
        match self {
            Type::Tuple(t) => Some(t),
            _ => None,
        }
    }

    pub fn try_as_signature(&self) -> Option<&SignatureDetail> {
        match self {
            Type::Signature(s) => Some(s),
            _ => None,
        }
    }

    pub fn try_as_signature_mut(&mut self) -> Option<&mut SignatureDetail> {
        match self {
            Type::Signature(s) => Some(s),
            _ => None,
        }
    }

    pub fn try_as_interface(&self) -> Option<&InterfaceDetail> {
        match &self {
            Type::Interface(i) => Some(i),
            _ => None,
        }
    }

    pub fn try_as_interface_mut(&mut self) -> Option<&mut InterfaceDetail> {
        match self {
            Type::Interface(i) => Some(i),
            _ => None,
        }
    }

    pub fn try_as_map(&self) -> Option<&MapDetail> {
        match &self {
            Type::Map(m) => Some(m),
            _ => None,
        }
    }

    pub fn try_as_chan(&self) -> Option<&ChanDetail> {
        match self {
            Type::Chan(c) => Some(c),
            _ => None,
        }
    }

    pub fn try_as_chan_mut(&mut self) -> Option<&mut ChanDetail> {
        match self {
            Type::Chan(c) => Some(c),
            _ => None,
        }
    }

    pub fn try_as_named(&self) -> Option<&NamedDetail> {
        match self {
            Type::Named(n) => Some(n),
            _ => None,
        }
    }

    pub fn try_as_named_mut(&mut self) -> Option<&mut NamedDetail> {
        match self {
            Type::Named(n) => Some(n),
            _ => None,
        }
    }

    pub fn underlying(&self) -> Option<TypeKey> {
        match self {
            Type::Named(detail) => detail.underlying,
            _ => None,
        }
    }

    pub fn underlying_val<'a>(&'a self, objs: &'a TCObjects) -> &'a Type {
        if let Some(k) = self.underlying() {
            &objs.types[k]
        } else {
            &self
        }
    }

    pub fn is_named(&self) -> bool {
        match self {
            Type::Basic(_) | Type::Named(_) => true,
            _ => false,
        }
    }
    pub fn is_boolean(&self, objs: &TCObjects) -> bool {
        match self.underlying_val(objs) {
            Type::Basic(b) => b.info() == BasicInfo::IsBoolean,
            _ => false,
        }
    }
    pub fn is_integer(&self, objs: &TCObjects) -> bool {
        match self.underlying_val(objs) {
            Type::Basic(b) => b.info() == BasicInfo::IsInteger,
            _ => false,
        }
    }
    pub fn is_unsigned(&self, objs: &TCObjects) -> bool {
        match self.underlying_val(objs) {
            Type::Basic(b) => b.typ().is_unsigned(),
            _ => false,
        }
    }
    pub fn is_float(&self, objs: &TCObjects) -> bool {
        match self.underlying_val(objs) {
            Type::Basic(b) => b.info() == BasicInfo::IsFloat,
            _ => false,
        }
    }
    pub fn is_complex(&self, objs: &TCObjects) -> bool {
        match self.underlying_val(objs) {
            Type::Basic(b) => b.info() == BasicInfo::IsComplex,
            _ => false,
        }
    }
    pub fn is_numeric(&self, objs: &TCObjects) -> bool {
        match self.underlying_val(objs) {
            Type::Basic(b) => b.info().is_numeric(),
            _ => false,
        }
    }
    pub fn is_string(&self, objs: &TCObjects) -> bool {
        match self.underlying_val(objs) {
            Type::Basic(b) => b.info() == BasicInfo::IsString,
            _ => false,
        }
    }
    pub fn is_typed(&self, objs: &TCObjects) -> bool {
        match self.underlying_val(objs) {
            Type::Basic(b) => !b.typ().is_untyped(),
            _ => true,
        }
    }
    pub fn is_untyped(&self, objs: &TCObjects) -> bool {
        match self.underlying_val(objs) {
            Type::Basic(b) => b.typ().is_untyped(),
            _ => false,
        }
    }
    pub fn is_ordered(&self, objs: &TCObjects) -> bool {
        match self.underlying_val(objs) {
            Type::Basic(b) => b.info().is_ordered(),
            _ => false,
        }
    }
    pub fn is_const_type(&self, objs: &TCObjects) -> bool {
        match self.underlying_val(objs) {
            Type::Basic(b) => b.info().is_const_type(),
            _ => false,
        }
    }
    pub fn is_interface(&self, objs: &TCObjects) -> bool {
        match self.underlying_val(objs) {
            Type::Interface(_) => true,
            _ => false,
        }
    }
    /// has_nil reports whether a type includes the nil value.
    pub fn has_nil(&self, objs: &TCObjects) -> bool {
        match self.underlying_val(objs) {
            Type::Basic(b) => b.typ() == BasicType::UnsafePointer,
            Type::Slice(_)
            | Type::Pointer(_)
            | Type::Signature(_)
            | Type::Interface(_)
            | Type::Map(_)
            | Type::Chan(_) => true,
            _ => false,
        }
    }
    /// comparable reports whether values of type T are comparable.
    pub fn comparable(&self, objs: &TCObjects) -> bool {
        match self.underlying_val(objs) {
            Type::Basic(b) => b.typ() != BasicType::UntypedNil,
            Type::Pointer(_) | Type::Interface(_) | Type::Chan(_) => true,
            Type::Struct(s) => !s
                .fields()
                .iter()
                .any(|f| !comparable(objs.lobjs[*f].typ().unwrap(), objs)),
            Type::Array(a) => comparable(a.elem(), objs),
            _ => false,
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum BasicType {
    Invalid,
    // predeclared types
    Bool,
    Int,
    Int8,
    Int16,
    Int32,
    Int64,
    Uint,
    Uint8,
    Uint16,
    Uint32,
    Uint64,
    Uintptr,
    Float32,
    Float64,
    Complex64,
    Complex128,
    Str,
    UnsafePointer,
    // types for untyped values
    UntypedBool,
    UntypedInt,
    UntypedRune,
    UntypedFloat,
    UntypedComplex,
    UntypedString,
    UntypedNil,
    // aliases
    Byte, // = Uint8
    Rune, // = Int32
}

impl BasicType {
    pub fn is_unsigned(&self) -> bool {
        match self {
            BasicType::Uint
            | BasicType::Uint8
            | BasicType::Uint16
            | BasicType::Uint32
            | BasicType::Uint64
            | BasicType::Byte
            | BasicType::Rune
            | BasicType::Uintptr => true,
            _ => false,
        }
    }

    pub fn is_untyped(&self) -> bool {
        match self {
            BasicType::UntypedBool
            | BasicType::UntypedInt
            | BasicType::UntypedRune
            | BasicType::UntypedFloat
            | BasicType::UntypedComplex
            | BasicType::UntypedString
            | BasicType::UntypedNil => true,
            _ => false,
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum BasicInfo {
    IsInvalid,
    IsBoolean,
    IsInteger,
    IsFloat,
    IsComplex,
    IsString,
}

impl BasicInfo {
    pub fn is_ordered(&self) -> bool {
        match self {
            BasicInfo::IsInteger | BasicInfo::IsFloat | BasicInfo::IsString => true,
            _ => false,
        }
    }

    pub fn is_numeric(&self) -> bool {
        match self {
            BasicInfo::IsInteger | BasicInfo::IsFloat | BasicInfo::IsComplex => true,
            _ => false,
        }
    }

    pub fn is_const_type(&self) -> bool {
        match self {
            BasicInfo::IsInteger
            | BasicInfo::IsFloat
            | BasicInfo::IsComplex
            | BasicInfo::IsString => true,
            _ => false,
        }
    }
}

/// A BasicDetail represents a basic type.
#[derive(Copy, Clone, Debug)]
pub struct BasicDetail {
    typ: BasicType,
    info: BasicInfo,
    name: &'static str,
}

impl BasicDetail {
    pub fn new(typ: BasicType, info: BasicInfo, name: &'static str) -> BasicDetail {
        BasicDetail {
            typ: typ,
            info: info,
            name: name,
        }
    }

    pub fn typ(&self) -> BasicType {
        self.typ
    }

    pub fn info(&self) -> BasicInfo {
        self.info
    }

    pub fn name(&self) -> &str {
        self.name
    }

    pub fn size_of(&self) -> usize {
        match self.typ {
            BasicType::Bool | BasicType::Byte | BasicType::Uint8 | BasicType::Int8 => 1,
            BasicType::Int16 | BasicType::Uint16 => 2,
            BasicType::Int32 | BasicType::Uint32 | BasicType::Rune | BasicType::Float32 => 4,
            BasicType::Int64 | BasicType::Uint64 | BasicType::Float64 | BasicType::Complex64 => 8,
            BasicType::Int | BasicType::Uint | BasicType::Uintptr | BasicType::UnsafePointer => {
                std_size_of::<usize>()
            }
            BasicType::Complex128 => 16,
            BasicType::Str => std_size_of::<String>(),
            _ => unreachable!(),
        }
    }
}

/// An ArrayDetail represents an array type.
#[derive(Debug)]
pub struct ArrayDetail {
    len: Option<u64>,
    elem: TypeKey,
}

impl ArrayDetail {
    pub fn new(elem: TypeKey, len: Option<u64>) -> ArrayDetail {
        ArrayDetail {
            len: len,
            elem: elem,
        }
    }

    pub fn len(&self) -> Option<u64> {
        self.len
    }

    pub fn set_len(&mut self, len: u64) {
        self.len = Some(len);
    }

    pub fn elem(&self) -> TypeKey {
        self.elem
    }
}

/// A Slice represents a slice type.
#[derive(Debug)]
pub struct SliceDetail {
    elem: TypeKey,
}

impl SliceDetail {
    pub fn new(elem: TypeKey) -> SliceDetail {
        SliceDetail { elem: elem }
    }

    pub fn elem(&self) -> TypeKey {
        self.elem
    }
}

/// A StructDetail represents a struct type
#[derive(Debug)]
pub struct StructDetail {
    fields: Vec<ObjKey>,               // objects ofr type LangObj::Var
    tags: Option<Vec<Option<String>>>, // None if there are no tags
}

impl StructDetail {
    pub fn new(
        fields: Vec<ObjKey>,
        tags: Option<Vec<Option<String>>>,
        objs: &TCObjects,
    ) -> StructDetail {
        // sanity checks
        if cfg!(debug_assertions) {
            let set = ObjSet::new();
            let result = fields.iter().try_fold(set, |s, x| {
                if !objs.lobjs[*x].entity_type().is_var() {
                    Err(())
                } else if s.insert(*x, objs).is_some() {
                    Err(())
                } else {
                    Ok::<ObjSet, ()>(s)
                }
            });
            assert!(result.is_ok());
            assert!(tags.is_none() || fields.len() >= tags.as_ref().unwrap().len());
        }
        StructDetail {
            fields: fields,
            tags: tags,
        }
    }

    pub fn fields(&self) -> &Vec<ObjKey> {
        &self.fields
    }

    pub fn tag(&self, i: usize) -> Option<&String> {
        self.tags
            .as_ref()
            .map(|x| if i < x.len() { x[i].as_ref() } else { None })
            .flatten()
    }
}

/// A PointerDetail represents a pointer type.
#[derive(Debug)]
pub struct PointerDetail {
    base: TypeKey, // element type
}

impl PointerDetail {
    pub fn new(base: TypeKey) -> PointerDetail {
        PointerDetail { base: base }
    }

    pub fn base(&self) -> TypeKey {
        self.base
    }
}

/// A TupleDetail represents an ordered list of variables
/// Tuples are used as components of signatures and to represent the type of multiple
/// assignments; they are not first class types of Go.
#[derive(Debug)]
pub struct TupleDetail {
    vars: Vec<ObjKey>, // LangObj::Var
}

impl TupleDetail {
    pub fn new(vars: Vec<ObjKey>) -> TupleDetail {
        TupleDetail { vars: vars }
    }

    pub fn vars(&self) -> &Vec<ObjKey> {
        &self.vars
    }

    pub fn vars_mut(&mut self) -> &mut Vec<ObjKey> {
        &mut self.vars
    }
}

/// A SignatureDetail represents a (non-builtin) function or method type.
/// The receiver is ignored when comparing signatures for identity.
#[derive(Copy, Clone, Debug)]
pub struct SignatureDetail {
    scope: Option<ScopeKey>, // function scope, present for package-local signatures
    recv: Option<ObjKey>,    // None if not a method
    params: TypeKey,
    results: TypeKey,
    variadic: bool,
}

impl SignatureDetail {
    // pass in 'objs' for sanity checks
    pub fn new(
        scope: Option<ScopeKey>,
        recv: Option<ObjKey>,
        params: TypeKey,
        results: TypeKey,
        variadic: bool,
        objs: &TCObjects,
    ) -> SignatureDetail {
        if cfg!(debug_assertions) {
            if variadic {
                let typ = &objs.types[params];
                match typ {
                    Type::Tuple(t) => {
                        assert!(t.vars.len() > 1);
                        let okey = t.vars[t.vars.len() - 1];
                        let tkey = &objs.lobjs[okey].typ().unwrap();
                        assert!(objs.types[*tkey].try_as_slice().is_some());
                    }
                    _ => unreachable!(),
                }
            }
        }
        SignatureDetail {
            scope: scope,
            recv: recv,
            params: params,
            results: results,
            variadic: variadic,
        }
    }

    pub fn scope(&self) -> Option<ScopeKey> {
        self.scope
    }

    /// Recv returns the receiver of signature s (if a method), or nil if a
    /// function. It is ignored when comparing signatures for identity.
    ///
    /// For an abstract method, Recv returns the enclosing interface either
    /// as a *Named or an *Interface. Due to embedding, an interface may
    /// contain methods whose receiver type is a different interface.
    pub fn recv(&self) -> &Option<ObjKey> {
        &self.recv
    }

    pub fn set_recv(&mut self, r: Option<ObjKey>) {
        self.recv = r
    }

    pub fn params(&self) -> TypeKey {
        self.params
    }

    pub fn set_params(&mut self, p: TypeKey) {
        self.params = p;
    }

    pub fn results(&self) -> TypeKey {
        self.results
    }

    pub fn variadic(&self) -> bool {
        self.variadic
    }

    pub fn params_count(&self, objs: &TCObjects) -> usize {
        objs.types[self.params].try_as_tuple().unwrap().vars().len()
    }

    pub fn results_count(&self, objs: &TCObjects) -> usize {
        objs.types[self.results]
            .try_as_tuple()
            .unwrap()
            .vars()
            .len()
    }
}

/// An InterfaceDetail represents an interface type.
#[derive(Debug)]
pub struct InterfaceDetail {
    methods: Vec<ObjKey>,
    embeddeds: Vec<TypeKey>,
    all_methods: Rc<RefCell<Option<Vec<ObjKey>>>>,
}

impl InterfaceDetail {
    // pass in 'objs' for sanity checks
    pub fn new(
        mut methods: Vec<ObjKey>,
        mut embeddeds: Vec<TypeKey>,
        objs: &mut TCObjects,
    ) -> InterfaceDetail {
        let set = ObjSet::new();
        let result = methods.iter().try_fold(set, |s, x| {
            let mobj = &objs.lobjs[*x];
            if !mobj.entity_type().is_func() {
                Err(())
            } else if s.insert(*x, &objs).is_some() {
                Err(())
            } else {
                let signature = objs.types[mobj.typ().unwrap()].try_as_signature_mut();
                if let Some(sig) = signature {
                    if sig.recv.is_none() {
                        let var =
                            LangObj::new_var(mobj.pos(), mobj.pkg(), "".to_string(), mobj.typ());
                        sig.recv = Some(objs.lobjs.insert(var));
                    }
                    Ok(s)
                } else {
                    Err(())
                }
            }
        });
        assert!(result.is_ok());
        methods.sort_by(compare_by_method_name!(&objs));
        embeddeds.sort_by(compare_by_type_name!(&objs));
        InterfaceDetail {
            methods: methods,
            embeddeds: embeddeds,
            all_methods: Rc::new(RefCell::new(None)),
        }
    }

    pub fn new_empty() -> InterfaceDetail {
        InterfaceDetail {
            methods: Vec::new(),
            embeddeds: Vec::new(),
            all_methods: Rc::new(RefCell::new(Some(Vec::new()))),
        }
    }

    pub fn methods(&self) -> &Vec<ObjKey> {
        &self.methods
    }

    pub fn methods_mut(&mut self) -> &mut Vec<ObjKey> {
        &mut self.methods
    }

    pub fn embeddeds(&self) -> &Vec<TypeKey> {
        &self.embeddeds
    }

    pub fn embeddeds_mut(&mut self) -> &mut Vec<TypeKey> {
        &mut self.embeddeds
    }

    pub fn all_methods(&self) -> Ref<Option<Vec<ObjKey>>> {
        self.all_methods.borrow()
    }

    pub fn all_methods_mut(&self) -> RefMut<Option<Vec<ObjKey>>> {
        self.all_methods.borrow_mut()
    }

    pub fn all_methods_push(&self, t: ObjKey) {
        if self.all_methods.borrow_mut().is_none() {
            *self.all_methods.borrow_mut() = Some(vec![t]);
        } else {
            self.all_methods.borrow_mut().as_mut().unwrap().push(t);
        }
    }

    pub fn is_empty(&self) -> bool {
        self.all_methods().as_ref().unwrap().len() == 0
    }

    pub fn set_empty_complete(&self) {
        *self.all_methods.borrow_mut() = Some(vec![]);
    }

    pub fn complete(&self, objs: &TCObjects) {
        if self.all_methods.borrow().is_some() {
            return;
        }
        let mut all = self.methods.clone();
        for tkey in self.embeddeds.iter() {
            let embeded = &objs.types[*tkey].try_as_interface().unwrap();
            embeded.complete(objs);
            all.append(&mut embeded.all_methods().as_ref().unwrap().clone());
        }
        all.sort_by(compare_by_method_name!(&objs));
        *self.all_methods.borrow_mut() = Some(all);
    }
}

#[derive(Debug)]
pub struct MapDetail {
    key: TypeKey,
    elem: TypeKey,
}

impl MapDetail {
    pub fn new(key: TypeKey, elem: TypeKey) -> MapDetail {
        MapDetail {
            key: key,
            elem: elem,
        }
    }

    pub fn key(&self) -> TypeKey {
        self.key
    }

    pub fn elem(&self) -> TypeKey {
        self.elem
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum ChanDir {
    SendRecv,
    SendOnly,
    RecvOnly,
}

#[derive(Debug)]
pub struct ChanDetail {
    dir: ChanDir,
    elem: TypeKey,
}

impl ChanDetail {
    pub fn new(dir: ChanDir, elem: TypeKey) -> ChanDetail {
        ChanDetail {
            dir: dir,
            elem: elem,
        }
    }

    pub fn dir(&self) -> ChanDir {
        self.dir
    }

    pub fn elem(&self) -> TypeKey {
        self.elem
    }
}

#[derive(Debug)]
pub struct NamedDetail {
    obj: Option<ObjKey>,         // corresponding declared object
    underlying: Option<TypeKey>, // possibly a Named during setup; never a Named once set up completely
    methods: Vec<ObjKey>, // methods declared for this type (not the method set of this type); signatures are type-checked lazily
}

impl NamedDetail {
    pub fn new(
        obj: Option<ObjKey>,
        underlying: Option<TypeKey>,
        methods: Vec<ObjKey>,
        objs: &TCObjects,
    ) -> NamedDetail {
        if cfg!(debug_assertions) && underlying.is_some() {
            let t = &objs.types[underlying.unwrap()];
            assert!(t.try_as_named().is_none());
        }
        //todo: where to set obj's typ to self?
        NamedDetail {
            obj: obj,
            underlying: underlying,
            methods: methods,
        }
    }

    pub fn obj(&self) -> &Option<ObjKey> {
        &self.obj
    }

    pub fn set_obj(&mut self, obj: ObjKey) {
        self.obj = Some(obj)
    }

    pub fn methods(&self) -> &Vec<ObjKey> {
        &self.methods
    }

    pub fn methods_mut(&mut self) -> &mut Vec<ObjKey> {
        &mut self.methods
    }

    pub fn underlying(&self) -> TypeKey {
        self.underlying.unwrap()
    }

    pub fn set_underlying(&mut self, t: TypeKey) {
        self.underlying = Some(t);
    }
}

// ----------------------------------------------------------------------------
// utilities

/// size_of only works for typed basic types for now
/// it panics with untyped, and return the size of
/// machine word size for all other types
pub fn size_of(t: &TypeKey, objs: &TCObjects) -> usize {
    let typ = &objs.types[*t].underlying_val(objs);
    match typ {
        Type::Basic(detail) => detail.size_of(),
        _ => std_size_of::<usize>(),
    }
}

/// underlying_type returns the underlying type of type 't'
pub fn underlying_type(t: TypeKey, objs: &TCObjects) -> TypeKey {
    let typ = &objs.types[t];
    match typ.underlying() {
        Some(ut) => ut,
        None => t,
    }
}

/// deep_underlying_type returns the 'deep' underlying type of type 't'
/// chains only exist while named types are incomplete.
pub fn deep_underlying_type(t: TypeKey, objs: &TCObjects) -> TypeKey {
    let mut typ = &objs.types[t];
    loop {
        match typ.underlying() {
            Some(ut) => {
                typ = &objs.types[ut];
                continue;
            }
            None => return t,
        }
    }
}

pub fn is_named(t: TypeKey, objs: &TCObjects) -> bool {
    objs.types[t].is_named()
}

pub fn is_boolean(t: TypeKey, objs: &TCObjects) -> bool {
    objs.types[t].is_boolean(objs)
}

pub fn is_integer(t: TypeKey, objs: &TCObjects) -> bool {
    objs.types[t].is_integer(objs)
}

pub fn is_unsigned(t: TypeKey, objs: &TCObjects) -> bool {
    objs.types[t].is_unsigned(objs)
}

pub fn is_float(t: TypeKey, objs: &TCObjects) -> bool {
    objs.types[t].is_float(objs)
}

pub fn is_complex(t: TypeKey, objs: &TCObjects) -> bool {
    objs.types[t].is_complex(objs)
}

pub fn is_numeric(t: TypeKey, objs: &TCObjects) -> bool {
    objs.types[t].is_numeric(objs)
}

pub fn is_string(t: TypeKey, objs: &TCObjects) -> bool {
    objs.types[t].is_string(objs)
}

pub fn is_typed(t: TypeKey, objs: &TCObjects) -> bool {
    objs.types[t].is_typed(objs)
}

pub fn is_untyped(t: TypeKey, objs: &TCObjects) -> bool {
    objs.types[t].is_untyped(objs)
}

pub fn is_ordered(t: TypeKey, objs: &TCObjects) -> bool {
    objs.types[t].is_ordered(objs)
}

pub fn is_const_type(t: TypeKey, objs: &TCObjects) -> bool {
    objs.types[t].is_const_type(objs)
}

pub fn is_interface(t: TypeKey, objs: &TCObjects) -> bool {
    objs.types[t].is_interface(objs)
}

/// has_nil reports whether a type includes the nil value.
pub fn has_nil(t: TypeKey, objs: &TCObjects) -> bool {
    objs.types[t].has_nil(objs)
}

/// comparable reports whether values of type T are comparable.
pub fn comparable(t: TypeKey, objs: &TCObjects) -> bool {
    objs.types[t].comparable(objs)
}

/// untyped_default_type returns the default "typed" type for an "untyped" type;
/// it returns the incoming type for all other types. The default type
/// for untyped nil is untyped nil.
pub fn untyped_default_type(t: TypeKey, objs: &TCObjects) -> TypeKey {
    objs.types[t].try_as_basic().map_or(t, |bt| match bt.typ() {
        BasicType::UntypedBool => objs.universe().types()[&BasicType::Bool],
        BasicType::UntypedInt => objs.universe().types()[&BasicType::Int],
        BasicType::UntypedRune => *objs.universe().rune(),
        BasicType::UntypedFloat => objs.universe().types()[&BasicType::Float64],
        BasicType::UntypedComplex => objs.universe().types()[&BasicType::Complex128],
        BasicType::UntypedString => objs.universe().types()[&BasicType::Str],
        _ => t,
    })
}

/// identical reports whether x and y are identical types.
/// Receivers of Signature types are ignored.
pub fn identical(x: TypeKey, y: TypeKey, objs: &TCObjects) -> bool {
    identical_impl(x, y, true, &mut HashSet::new(), objs)
}

/// identical_option is the same as identical except for the parameters
pub fn identical_option(x: Option<TypeKey>, y: Option<TypeKey>, objs: &TCObjects) -> bool {
    identical_impl_o(x, y, true, &mut HashSet::new(), objs)
}

/// identical_ignore_tags reports whether x and y are identical types if tags are ignored.
/// Receivers of Signature types are ignored.
pub fn identical_ignore_tags(x: Option<TypeKey>, y: Option<TypeKey>, objs: &TCObjects) -> bool {
    identical_impl_o(x, y, false, &mut HashSet::new(), objs)
}

/// identical_impl_o accepts wrapped TypeKeys
fn identical_impl_o(
    x: Option<TypeKey>,
    y: Option<TypeKey>,
    cmp_tags: bool,
    dup: &mut HashSet<(TypeKey, TypeKey)>,
    objs: &TCObjects,
) -> bool {
    if x == y {
        true
    } else if x.is_none() || y.is_none() {
        false
    } else {
        identical_impl(x.unwrap(), y.unwrap(), cmp_tags, dup, objs)
    }
}

/// identical_impl implements the logic of identical
/// 'dup' is used to detect cycles.
fn identical_impl(
    x: TypeKey,
    y: TypeKey,
    cmp_tags: bool,
    dup: &mut HashSet<(TypeKey, TypeKey)>,
    objs: &TCObjects,
) -> bool {
    if x == y {
        return true;
    }
    let tx = &objs.types[x];
    let ty = &objs.types[y];

    match (tx, ty) {
        (Type::Basic(bx), Type::Basic(by)) => bx.typ() == by.typ(),
        (Type::Array(ax), Type::Array(ay)) => {
            ax.len() == ay.len() && identical_impl(ax.elem(), ay.elem(), cmp_tags, dup, objs)
        }
        (Type::Slice(sx), Type::Slice(sy)) => {
            identical_impl(sx.elem(), sy.elem(), cmp_tags, dup, objs)
        }
        (Type::Struct(sx), Type::Struct(sy)) => {
            if sx.fields().len() == sy.fields().len() {
                !sx.fields().iter().enumerate().any(|(i, f)| {
                    let of = &objs.lobjs[*f];
                    let og = &objs.lobjs[sy.fields()[i]];
                    (of.var_embedded() != og.var_embedded())
                        || (cmp_tags && sx.tag(i) != sy.tag(i))
                        || (!of.same_id(og.pkg(), og.name(), objs))
                        || (!identical_impl_o(of.typ(), og.typ(), cmp_tags, dup, objs))
                })
            } else {
                false
            }
        }
        (Type::Pointer(px), Type::Pointer(py)) => {
            identical_impl(px.base(), py.base(), cmp_tags, dup, objs)
        }
        (Type::Tuple(tx), Type::Tuple(ty)) => {
            if tx.vars().len() == ty.vars().len() {
                !tx.vars().iter().enumerate().any(|(i, v)| {
                    let ov = &objs.lobjs[*v];
                    let ow = &objs.lobjs[ty.vars()[i]];
                    !identical_impl_o(ov.typ(), ow.typ(), cmp_tags, dup, objs)
                })
            } else {
                false
            }
        }
        (Type::Signature(sx), Type::Signature(sy)) => {
            sx.variadic() == sy.variadic()
                && identical_impl(sx.params(), sy.params(), cmp_tags, dup, objs)
                && identical_impl(sx.results(), sy.results(), cmp_tags, dup, objs)
        }
        (Type::Interface(ix), Type::Interface(iy)) => {
            match (ix.all_methods().as_ref(), iy.all_methods().as_ref()) {
                (None, None) => true,
                (Some(a), Some(b)) => {
                    if a.len() == b.len() {
                        // Interface types are the only types where cycles can occur
                        // that are not "terminated" via named types; and such cycles
                        // can only be created via method parameter types that are
                        // anonymous interfaces (directly or indirectly) embedding
                        // the current interface. Example:
                        //
                        //    type T interface {
                        //        m() interface{T}
                        //    }
                        //
                        // If two such (differently named) interfaces are compared,
                        // endless recursion occurs if the cycle is not detected.
                        //
                        // If x and y were compared before, they must be equal
                        // (if they were not, the recursion would have stopped);
                        let pair = (x, y);
                        if dup.get(&pair).is_some() {
                            // pair got compared before
                            true
                        } else {
                            dup.insert(pair);
                            !a.iter().enumerate().any(|(i, k)| {
                                let ox = &objs.lobjs[*k];
                                let oy = &objs.lobjs[b[i]];
                                ox.id(objs) != oy.id(objs)
                                    || !identical_impl_o(ox.typ(), oy.typ(), cmp_tags, dup, objs)
                            })
                        }
                    } else {
                        false
                    }
                }
                _ => false,
            }
        }
        (Type::Map(mx), Type::Map(my)) => {
            identical_impl(mx.key(), my.key(), cmp_tags, dup, objs)
                && identical_impl(mx.elem(), mx.elem(), cmp_tags, dup, objs)
        }
        (Type::Chan(cx), Type::Chan(cy)) => {
            cx.dir() == cy.dir() && identical_impl(cx.elem(), cy.elem(), cmp_tags, dup, objs)
        }
        (Type::Named(nx), Type::Named(ny)) => nx.obj() == ny.obj(),
        _ => false,
    }
}

pub fn fmt_type(t: Option<TypeKey>, f: &mut fmt::Formatter<'_>, objs: &TCObjects) -> fmt::Result {
    fmt_type_impl(t, f, &mut HashSet::new(), objs)
}

pub fn fmt_signature(t: TypeKey, f: &mut fmt::Formatter<'_>, objs: &TCObjects) -> fmt::Result {
    fmt_signature_impl(t, f, &mut HashSet::new(), objs)
}

fn fmt_type_impl(
    t: Option<TypeKey>,
    f: &mut fmt::Formatter<'_>,
    visited: &mut HashSet<TypeKey>,
    objs: &TCObjects,
) -> fmt::Result {
    if t.is_none() {
        return f.write_str("<nil>");
    }
    let tkey = t.unwrap();
    if visited.get(&tkey).is_some() {
        return tkey.fmt(f);
    }
    let typ = &objs.types[tkey];
    match typ {
        Type::Basic(detail) => {
            if detail.typ == BasicType::UnsafePointer {
                f.write_str("unsafe.")?;
            }
            write!(f, "{}", detail.name)?;
        }
        Type::Array(detail) => {
            match detail.len {
                Some(i) => write!(f, "[{}]", i)?,
                None => f.write_str("[unknown]")?,
            };
            fmt_type_impl(Some(detail.elem), f, visited, objs)?;
        }
        Type::Slice(detail) => {
            f.write_str("[]")?;
            fmt_type_impl(Some(detail.elem), f, visited, objs)?;
        }
        Type::Struct(detail) => {
            f.write_str("struct{{")?;
            for (i, key) in detail.fields().iter().enumerate() {
                if i > 0 {
                    f.write_str("; ")?;
                }
                let field = &objs.lobjs[*key];
                if !field.var_embedded() {
                    write!(f, "{} ", field.name())?;
                }
                fmt_type_impl(field.typ(), f, visited, objs)?;
                if let Some(tag) = detail.tag(i) {
                    write!(f, " {}", tag)?;
                }
            }
            f.write_str("}}")?;
        }
        Type::Pointer(detail) => {
            f.write_char('*')?;
            fmt_type_impl(Some(detail.base()), f, visited, objs)?;
        }
        Type::Tuple(_) => {
            fmt_tuple(tkey, false, f, visited, objs)?;
        }
        Type::Signature(_) => {
            f.write_str("func")?;
            fmt_signature_impl(t.unwrap(), f, visited, objs)?;
        }
        Type::Interface(detail) => {
            f.write_str("interface{{")?;
            for (i, k) in detail.methods().iter().enumerate() {
                if i > 0 {
                    f.write_str("; ")?;
                }
                let mobj = &objs.lobjs[*k];
                f.write_str(mobj.name())?;
                fmt_signature_impl(mobj.typ().unwrap(), f, visited, objs)?;
            }
            for (i, k) in detail.embeddeds().iter().enumerate() {
                if i > 0 || detail.methods().len() > 0 {
                    f.write_str("; ")?;
                }
                fmt_type_impl(Some(*k), f, visited, objs)?;
            }
            if detail.all_methods().is_none() {
                f.write_str(" /* incomplete */")?;
            }
            f.write_char('}')?;
        }
        Type::Map(detail) => {
            f.write_str("map[")?;
            fmt_type_impl(Some(detail.key()), f, visited, objs)?;
            f.write_char(']')?;
            fmt_type_impl(Some(detail.elem()), f, visited, objs)?;
        }
        Type::Chan(detail) => {
            let (s, paren) = match detail.dir() {
                ChanDir::SendRecv => ("chan ", {
                    // chan (<-chan T) requires parentheses
                    let elm = &objs.types[detail.elem()];
                    if let Some(c) = elm.try_as_chan() {
                        c.dir() == ChanDir::RecvOnly
                    } else {
                        false
                    }
                }),
                ChanDir::SendOnly => ("chan<- ", false),
                ChanDir::RecvOnly => ("<-chan ", false),
            };
            f.write_str(s)?;
            if paren {
                f.write_char('(')?;
            }
            fmt_type_impl(Some(detail.elem()), f, visited, objs)?;
            if paren {
                f.write_char(')')?;
            }
        }
        Type::Named(detail) => {
            if let Some(okey) = detail.obj() {
                let o = &objs.lobjs[*okey];
                if let Some(pkg) = o.pkg() {
                    objs.pkgs[pkg].fmt_with_qualifier(f, objs.fmt_qualifier.as_ref())?;
                }
                f.write_str(o.name())?;
            } else {
                f.write_str("<Named w/o object>")?;
            }
        }
    }
    Ok(())
}

fn fmt_signature_impl(
    t: TypeKey,
    f: &mut fmt::Formatter<'_>,
    visited: &mut HashSet<TypeKey>,
    objs: &TCObjects,
) -> fmt::Result {
    let sig = &objs.types[t].try_as_signature().unwrap();
    fmt_tuple(sig.params(), sig.variadic(), f, visited, &objs)?;
    f.write_char(' ')?;
    let results = &objs.types[sig.results()].try_as_tuple().unwrap();
    if results.vars().len() == 1 {
        let obj = &objs.lobjs[results.vars()[0]];
        if obj.name().is_empty() {
            // single unnamed result
            return fmt_type_impl(obj.typ(), f, visited, objs);
        }
    }
    // multiple or named result(s)
    fmt_tuple(sig.results(), false, f, visited, objs)
}

fn fmt_tuple(
    tkey: TypeKey,
    variadic: bool,
    f: &mut fmt::Formatter<'_>,
    visited: &mut HashSet<TypeKey>,
    objs: &TCObjects,
) -> fmt::Result {
    f.write_char('(')?;
    let tuple = &objs.types[tkey].try_as_tuple().unwrap();
    for (i, v) in tuple.vars().iter().enumerate() {
        if i > 0 {
            f.write_str(", ")?;
        }
        let obj = &objs.lobjs[*v];
        if !obj.name().is_empty() {
            write!(f, "{} ", obj.name())?;
        }
        let tkey = obj.typ();
        if variadic && i == tuple.vars().len() - 1 {
            let utype = underlying_type(tkey.unwrap(), &objs);
            let typ = &objs.types[utype];
            match typ {
                Type::Slice(detail) => {
                    f.write_str("...")?;
                    fmt_type_impl(Some(detail.elem()), f, visited, objs)?;
                }
                Type::Basic(detail) => {
                    // special case:
                    // append(s, "foo"...) leads to signature func([]byte, string...)
                    assert!(detail.typ() == BasicType::Str);
                    fmt_type_impl(tkey, f, visited, objs)?;
                    f.write_str("...")?;
                }
                _ => unreachable!(),
            }
        } else {
            fmt_type_impl(tkey, f, visited, objs)?;
        }
    }
    f.write_char(')')?;
    Ok(())
}
