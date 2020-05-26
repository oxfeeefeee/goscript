#![allow(dead_code)]
//use super::obj::LangObj;
use super::obj::ObjSet;
use super::objects::{ObjKey, ScopeKey, TCObjects, TypeKey};
use std::cell::{Ref, RefCell};
use std::collections::HashSet;
use std::fmt;
use std::fmt::Debug;
use std::fmt::Write;
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
        match &self {
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

    pub fn try_as_map(&self) -> Option<&MapDetail> {
        match &self {
            Type::Map(m) => Some(m),
            _ => None,
        }
    }

    pub fn try_as_chan(&self) -> Option<&ChanDetail> {
        match &self {
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

    pub fn underlying(&self) -> Option<&TypeKey> {
        match self {
            Type::Named(detail) => Some(&detail.underlying),
            _ => None,
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
}

/// An ArrayDetail represents an array type.
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

    pub fn len(&self) -> &Option<u64> {
        &self.len
    }

    pub fn elem(&self) -> &TypeKey {
        &self.elem
    }
}

/// A Slice represents a slice type.
pub struct SliceDetail {
    elem: TypeKey,
}

impl SliceDetail {
    pub fn new(elem: TypeKey) -> SliceDetail {
        SliceDetail { elem: elem }
    }

    pub fn elem(&self) -> &TypeKey {
        &self.elem
    }
}

/// A StructDetail represents a struct type
pub struct StructDetail {
    fields: Vec<ObjKey>, // objects ofr type LangObj::Var
    tags: Vec<String>,
}

impl StructDetail {
    pub fn new(fields: Vec<ObjKey>, tags: Vec<String>, objs: &TCObjects) -> StructDetail {
        // sanity checks
        if objs.debug {
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
            assert!(fields.len() >= tags.len());
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
        self.tags.get(i)
    }
}

/// A PointerDetail represents a pointer type.
pub struct PointerDetail {
    base: TypeKey, // element type
}

impl PointerDetail {
    pub fn base(&self) -> &TypeKey {
        &self.base
    }
}

/// A TupleDetail represents an ordered list of variables
/// Tuples are used as components of signatures and to represent the type of multiple
/// assignments; they are not first class types of Go.
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
}

/// A SignatureDetail represents a (non-builtin) function or method type.
/// The receiver is ignored when comparing signatures for identity.
pub struct SignatureDetail {
    scope: Option<ScopeKey>, // function scope, present for package-local signatures
    recv: Option<ObjKey>,    // None if not a method
    params: Option<TypeKey>,
    results: Option<TypeKey>,
    variadic: bool,
}

impl SignatureDetail {
    // pass in 'objs' for sanity checks
    pub fn new(
        recv: Option<ObjKey>,
        params: Option<TypeKey>,
        results: Option<TypeKey>,
        variadic: bool,
        objs: &TCObjects,
    ) -> SignatureDetail {
        if objs.debug {
            if variadic {
                let typ = &objs.types[params.unwrap()];
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
            scope: None,
            recv: recv,
            params: params,
            results: results,
            variadic: variadic,
        }
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

    pub fn params(&self) -> &Option<TypeKey> {
        &self.params
    }

    pub fn results(&self) -> &Option<TypeKey> {
        &self.results
    }

    pub fn variadic(&self) -> &bool {
        &self.variadic
    }

    pub fn set_recv(&mut self, recv: ObjKey) {
        self.recv = Some(recv)
    }
}

/// An InterfaceDetail represents an interface type.
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
        objs: &TCObjects,
    ) -> InterfaceDetail {
        if objs.debug {
            let set = ObjSet::new();
            let result = methods.iter().try_fold(set, |s, x| {
                let func_obj = &objs.lobjs[*x];
                if !func_obj.entity_type().is_func() {
                    Err(())
                } else if s.insert(*x, &objs).is_some() {
                    Err(())
                } else {
                    let signature = &objs.types[func_obj.typ().unwrap()].try_as_signature();
                    if let Some(sig) = signature {
                        if sig.recv.is_none() {
                            Err(())
                        } else {
                            Ok(s)
                        }
                    } else {
                        Err(())
                    }
                }
            });
            assert!(result.is_ok());
        }
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

    pub fn embeddeds(&self) -> &Vec<TypeKey> {
        &self.embeddeds
    }

    pub fn all_methods(&self) -> Ref<Option<Vec<ObjKey>>> {
        self.all_methods.borrow()
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
        *self.all_methods.borrow_mut() = Some(all);
    }
}

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

    pub fn key(&self) -> &TypeKey {
        &self.key
    }

    pub fn elem(&self) -> &TypeKey {
        &self.elem
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum ChanDir {
    SendRecv,
    SendOnly,
    RecvOnly,
}

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

    pub fn dir(&self) -> &ChanDir {
        &self.dir
    }

    pub fn elem(&self) -> &TypeKey {
        &self.elem
    }
}

pub struct NamedDetail {
    obj: Option<ObjKey>,  // corresponding declared object
    underlying: TypeKey,  // possibly a Named during setup; never a Named once set up completely
    methods: Vec<ObjKey>, // methods declared for this type (not the method set of this type)
}

impl NamedDetail {
    pub fn new(
        obj: Option<ObjKey>,
        underlying: TypeKey,
        methods: Vec<ObjKey>,
        objs: &TCObjects,
    ) -> NamedDetail {
        if objs.debug {
            let t = &objs.types[underlying];
            assert!(t.try_as_named().is_some());
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

    pub fn set_underlying(&mut self, t: TypeKey, objs: &TCObjects) {
        if objs.debug {
            let t = &objs.types[t];
            assert!(t.try_as_named().is_some());
        }
        self.underlying = t;
    }
}

// ----------------------------------------------------------------------------
// utilities

pub fn underlying_type(t: TypeKey, objs: &TCObjects) -> TypeKey {
    let typ = &objs.types[t];
    match typ.underlying() {
        Some(ut) => *ut,
        None => t,
    }
}

pub fn fmt_type(t: &Option<TypeKey>, f: &mut fmt::Formatter<'_>, objs: &TCObjects) -> fmt::Result {
    fmt_type_impl(t, f, &mut HashSet::new(), objs)
}

pub fn fmt_signature(t: TypeKey, f: &mut fmt::Formatter<'_>, objs: &TCObjects) -> fmt::Result {
    fmt_signature_impl(t, f, &mut HashSet::new(), objs)
}

fn fmt_type_impl(
    t: &Option<TypeKey>,
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
            fmt_type_impl(&Some(detail.elem), f, visited, objs)?;
        }
        Type::Slice(detail) => {
            f.write_str("[]")?;
            fmt_type_impl(&Some(detail.elem), f, visited, objs)?;
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
            fmt_type_impl(&Some(*detail.base()), f, visited, objs)?;
        }
        Type::Tuple(_) => {
            fmt_tuple(t, false, f, visited, objs)?;
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
                fmt_type_impl(&Some(*k), f, visited, objs)?;
            }
            if detail.all_methods().is_none() {
                f.write_str(" /* incomplete */")?;
            }
            f.write_char('}')?;
        }
        Type::Map(detail) => {
            f.write_str("map[")?;
            fmt_type_impl(&Some(*detail.key()), f, visited, objs)?;
            f.write_char(']')?;
            fmt_type_impl(&Some(*detail.elem()), f, visited, objs)?;
        }
        Type::Chan(detail) => {
            let (s, paren) = match detail.dir() {
                ChanDir::SendRecv => ("chan ", {
                    // chan (<-chan T) requires parentheses
                    let elm = &objs.types[*detail.elem()];
                    if let Some(c) = elm.try_as_chan() {
                        *c.dir() == ChanDir::RecvOnly
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
            fmt_type_impl(&Some(*detail.elem()), f, visited, objs)?;
            if paren {
                f.write_char(')')?;
            }
        }
        Type::Named(detail) => {
            if let Some(okey) = detail.obj() {
                let o = &objs.lobjs[*okey];
                if let Some(pkg) = o.pkg() {
                    objs.pkgs[*pkg].fmt_with_qualifier(f, objs.fmt_qualifier.as_ref())?;
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
    fmt_tuple(sig.params(), *sig.variadic(), f, visited, &objs)?;
    if sig.results().is_none() {
        // no result
        return Ok(());
    }
    f.write_char(' ')?;
    let results = &objs.types[sig.results().unwrap()].try_as_tuple().unwrap();
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
    t: &Option<TypeKey>,
    variadic: bool,
    f: &mut fmt::Formatter<'_>,
    visited: &mut HashSet<TypeKey>,
    objs: &TCObjects,
) -> fmt::Result {
    f.write_char('(')?;
    if let Some(tkey) = t {
        let tuple = &objs.types[*tkey].try_as_tuple().unwrap();
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
                        fmt_type_impl(&Some(*detail.elem()), f, visited, objs)?;
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
    }
    f.write_char(')')?;
    Ok(())
}
