// Copyright 2022 The Goscript Authors. All rights reserved.
// Use of this source code is governed by a BSD-style
// license that can be found in the LICENSE file.

use crate::bytecode::{FunctionKey, MetadataKey, MetadataObjs, VMObjects};
use crate::gc::GcContainer;
use crate::instruction::{OpIndex, ValueType};
use crate::objects::{IfaceBinding, StructObj};
use crate::value::ArrCaller;
use crate::value::GosValue;
#[cfg(feature = "serde_borsh")]
use borsh::{
    maybestd::io::Result as BorshResult, maybestd::io::Write as BorshWrite, BorshDeserialize,
    BorshSerialize,
};
use goscript_parser::Map;
use std::cell::RefCell;
use std::rc::Rc;

#[cfg_attr(feature = "serde_borsh", derive(BorshDeserialize, BorshSerialize))]
#[derive(PartialEq, Eq, Clone, Debug)]
pub enum ChannelType {
    Send,
    Recv,
    SendRecv,
}

#[derive(Debug)]
pub struct PrimitiveMeta {
    pub mbool: Meta,
    pub mint: Meta,
    pub mint8: Meta,
    pub mint16: Meta,
    pub mint32: Meta,
    pub mint64: Meta,
    pub muint: Meta,
    pub muint_ptr: Meta,
    pub muint8: Meta,
    pub muint16: Meta,
    pub muint32: Meta,
    pub muint64: Meta,
    pub mfloat32: Meta,
    pub mfloat64: Meta,
    pub mcomplex64: Meta,
    pub mcomplex128: Meta,
    pub mstr: Meta,
    pub unsafe_ptr: Meta,
    pub default_sig: Meta,
    pub empty_iface: Meta,
    pub none: Meta,
}

impl PrimitiveMeta {
    pub fn new(objs: &mut MetadataObjs) -> PrimitiveMeta {
        PrimitiveMeta {
            mbool: Meta::with_type(MetadataType::Bool, objs),
            mint: Meta::with_type(MetadataType::Int, objs),
            mint8: Meta::with_type(MetadataType::Int8, objs),
            mint16: Meta::with_type(MetadataType::Int16, objs),
            mint32: Meta::with_type(MetadataType::Int32, objs),
            mint64: Meta::with_type(MetadataType::Int64, objs),
            muint: Meta::with_type(MetadataType::Uint, objs),
            muint_ptr: Meta::with_type(MetadataType::UintPtr, objs),
            muint8: Meta::with_type(MetadataType::Uint8, objs),
            muint16: Meta::with_type(MetadataType::Uint16, objs),
            muint32: Meta::with_type(MetadataType::Uint32, objs),
            muint64: Meta::with_type(MetadataType::Uint64, objs),
            mfloat32: Meta::with_type(MetadataType::Float32, objs),
            mfloat64: Meta::with_type(MetadataType::Float64, objs),
            mcomplex64: Meta::with_type(MetadataType::Complex64, objs),
            mcomplex128: Meta::with_type(MetadataType::Complex128, objs),
            mstr: Meta::with_type(MetadataType::Str, objs),
            unsafe_ptr: Meta::with_type(MetadataType::UnsafePtr, objs),
            default_sig: Meta::with_type(MetadataType::Signature(SigMetadata::default()), objs),
            empty_iface: Meta::with_type(MetadataType::Interface(Fields::new(vec![])), objs),
            none: Meta::with_type(MetadataType::None, objs),
        }
    }
}

#[cfg_attr(feature = "serde_borsh", derive(BorshDeserialize, BorshSerialize))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Meta {
    pub key: MetadataKey,
    pub ptr_depth: u8,
    pub is_type: bool,
}

impl Meta {
    #[inline]
    pub fn new(key: MetadataKey, pdepth: u8, is_type: bool) -> Meta {
        Meta {
            key: key,
            ptr_depth: pdepth,
            is_type: is_type,
        }
    }

    #[inline]
    pub fn with_type(v: MetadataType, metas: &mut MetadataObjs) -> Meta {
        Meta::new(metas.insert(v), 0, false)
    }

    #[inline]
    pub fn new_array(elem_meta: Meta, size: usize, metas: &mut MetadataObjs) -> Meta {
        let t = MetadataType::Array(elem_meta, size);
        Meta {
            key: metas.insert(t),
            ptr_depth: 0,
            is_type: false,
        }
    }

    #[inline]
    pub fn new_slice(val_meta: Meta, metas: &mut MetadataObjs) -> Meta {
        Meta::with_type(MetadataType::Slice(val_meta), metas)
    }

    #[inline]
    pub fn new_map(kmeta: Meta, vmeta: Meta, metas: &mut MetadataObjs) -> Meta {
        Meta::with_type(MetadataType::Map(kmeta, vmeta), metas)
    }

    #[inline]
    pub fn new_interface(fields: Fields, metas: &mut MetadataObjs) -> Meta {
        Meta::with_type(MetadataType::Interface(fields), metas)
    }

    #[inline]
    pub fn new_channel(typ: ChannelType, val_meta: Meta, metas: &mut MetadataObjs) -> Meta {
        Meta::with_type(MetadataType::Channel(typ, val_meta), metas)
    }

    #[inline]
    pub(crate) fn new_struct(f: Fields, objs: &mut VMObjects) -> Meta {
        let key = objs.metas.insert(MetadataType::Struct(f));
        Meta::new(key, 0, false)
    }

    pub fn new_sig(
        recv: Option<Meta>,
        params: Vec<Meta>,
        results: Vec<Meta>,
        variadic: Option<(Meta, Meta)>,
        metas: &mut MetadataObjs,
    ) -> Meta {
        let params_type = params.iter().map(|x| x.value_type(metas)).collect();
        let t = MetadataType::Signature(SigMetadata {
            recv,
            params,
            results,
            variadic,
            params_type,
        });
        Meta::with_type(t, metas)
    }

    pub fn new_named(underlying: Meta, metas: &mut MetadataObjs) -> Meta {
        //debug_assert!(underlying.value_type(metas) != ValueType::Named);
        Meta::with_type(MetadataType::Named(Methods::new(), underlying), metas)
    }

    #[inline]
    pub fn mtype_unwraped<'a>(&self, metas: &'a MetadataObjs) -> &'a MetadataType {
        metas[self.key].unwrap_named(metas)
    }

    #[inline]
    pub fn ptr_to(&self) -> Meta {
        let mut m = *self;
        m.ptr_depth += 1;
        m
    }

    #[inline]
    pub fn unptr_to(&self) -> Meta {
        assert!(self.ptr_depth > 0);
        let mut m = *self;
        m.ptr_depth -= 1;
        m
    }

    #[inline]
    pub fn into_type_category(mut self) -> Meta {
        self.is_type = true;
        self
    }

    #[inline]
    pub fn into_value_category(mut self) -> Meta {
        self.is_type = false;
        self
    }

    #[inline]
    pub fn value_type(&self, metas: &MetadataObjs) -> ValueType {
        match self.is_type {
            false => match self.ptr_depth {
                0 => match &metas[self.key] {
                    MetadataType::Bool => ValueType::Bool,
                    MetadataType::Int => ValueType::Int,
                    MetadataType::Int8 => ValueType::Int8,
                    MetadataType::Int16 => ValueType::Int16,
                    MetadataType::Int32 => ValueType::Int32,
                    MetadataType::Int64 => ValueType::Int64,
                    MetadataType::Uint => ValueType::Uint,
                    MetadataType::UintPtr => ValueType::UintPtr,
                    MetadataType::Uint8 => ValueType::Uint8,
                    MetadataType::Uint16 => ValueType::Uint16,
                    MetadataType::Uint32 => ValueType::Uint32,
                    MetadataType::Uint64 => ValueType::Uint64,
                    MetadataType::Float32 => ValueType::Float32,
                    MetadataType::Float64 => ValueType::Float64,
                    MetadataType::Complex64 => ValueType::Complex64,
                    MetadataType::Complex128 => ValueType::Complex128,
                    MetadataType::UnsafePtr => ValueType::UnsafePtr,
                    MetadataType::Str => ValueType::String,
                    MetadataType::Struct(_) => ValueType::Struct,
                    MetadataType::Signature(_) => ValueType::Closure,
                    MetadataType::Array(_, _) => ValueType::Array,
                    MetadataType::Slice(_) => ValueType::Slice,
                    MetadataType::Map(_, _) => ValueType::Map,
                    MetadataType::Interface(_) => ValueType::Interface,
                    MetadataType::Channel(_, _) => ValueType::Channel,
                    MetadataType::Named(_, m) => m.value_type(metas),
                    MetadataType::None => ValueType::Void,
                },
                _ => ValueType::Pointer,
            },
            true => ValueType::Metadata,
        }
    }

    #[inline]
    pub(crate) fn zero(&self, mobjs: &MetadataObjs, gcc: &GcContainer) -> GosValue {
        match self.ptr_depth {
            0 => match &mobjs[self.key] {
                MetadataType::Bool => false.into(),
                MetadataType::Int => 0isize.into(),
                MetadataType::Int8 => 0i8.into(),
                MetadataType::Int16 => 0i16.into(),
                MetadataType::Int32 => 0i32.into(),
                MetadataType::Int64 => 0i64.into(),
                MetadataType::Uint => 0isize.into(),
                MetadataType::UintPtr => GosValue::new_uint_ptr(0),
                MetadataType::Uint8 => 0u8.into(),
                MetadataType::Uint16 => 0u16.into(),
                MetadataType::Uint32 => 0u32.into(),
                MetadataType::Uint64 => 0u64.into(),
                MetadataType::Float32 => GosValue::new_float32(0.0.into()),
                MetadataType::Float64 => GosValue::new_float64(0.0.into()),
                MetadataType::Complex64 => GosValue::new_complex64(0.0.into(), 0.0.into()),
                MetadataType::Complex128 => GosValue::new_complex128(0.0.into(), 0.0.into()),
                MetadataType::UnsafePtr => GosValue::new_nil(ValueType::UnsafePtr),
                MetadataType::Str => GosValue::with_str(""),
                MetadataType::Array(m, size) => {
                    let val = m.zero(mobjs, gcc);
                    let t = m.value_type(mobjs);
                    let caller = ArrCaller::get_slow(t);
                    GosValue::array_with_size(*size, *size, &val, &caller, gcc)
                }
                MetadataType::Slice(m) => GosValue::new_nil_slice(m.value_type(mobjs)),
                MetadataType::Struct(f) => {
                    let field_zeros: Vec<GosValue> =
                        f.fields.iter().map(|x| x.meta.zero(mobjs, gcc)).collect();
                    let struct_val = StructObj::new(field_zeros);
                    GosValue::new_struct(struct_val, gcc)
                }
                MetadataType::Signature(_) => GosValue::new_nil(ValueType::Closure),
                MetadataType::Map(_, _) => GosValue::new_nil(ValueType::Map),
                MetadataType::Interface(_) => GosValue::new_nil(ValueType::Interface),
                MetadataType::Channel(_, _) => GosValue::new_nil(ValueType::Channel),
                MetadataType::Named(_, gm) => gm.zero(mobjs, gcc),
                MetadataType::None => unreachable!(),
            },
            _ => GosValue::new_nil(ValueType::Pointer),
        }
    }

    #[inline]
    pub fn field_indices(&self, name: &str, metas: &MetadataObjs) -> Vec<usize> {
        let key = self.recv_meta_key();
        match &metas[Meta::new(key, 0, false).underlying(metas).key] {
            MetadataType::Struct(m) => m.indices_by_name(name),
            _ => unreachable!(),
        }
    }

    #[inline]
    pub fn underlying(&self, metas: &MetadataObjs) -> Meta {
        match &metas[self.key] {
            MetadataType::Named(_, u) => *u,
            _ => *self,
        }
    }

    #[inline]
    pub fn recv_meta_key(&self) -> MetadataKey {
        assert!(self.ptr_depth <= 1);
        self.key
    }

    pub fn add_method(&self, name: String, pointer_recv: bool, metas: &mut MetadataObjs) {
        let k = self.recv_meta_key();
        match &mut metas[k] {
            MetadataType::Named(m, _) => {
                m.members.push(Rc::new(RefCell::new(MethodDesc {
                    pointer_recv: pointer_recv,
                    func: None,
                })));
                m.mapping.insert(name, m.members.len() as OpIndex - 1);
            }
            _ => unreachable!(),
        }
    }

    pub fn set_method_code(&self, name: &String, func: FunctionKey, metas: &mut MetadataObjs) {
        let k = self.recv_meta_key();
        match &mut metas[k] {
            MetadataType::Named(m, _) => {
                let index = m.mapping[name] as usize;
                m.members[index].borrow_mut().func = Some(func);
            }
            _ => unreachable!(),
        }
    }

    /// Depth-first search for method by name
    pub fn get_iface_binding(&self, name: &String, metas: &MetadataObjs) -> Option<IfaceBinding> {
        match &metas[self.key] {
            MetadataType::Named(m, underlying) => match m.mapping.get(name) {
                Some(&i) => Some(IfaceBinding::Struct(m.members[i as usize].clone(), None)),
                None => underlying.get_iface_binding(name, metas),
            },
            MetadataType::Interface(fields) => fields
                .try_index_by_name(name)
                .map(|x| IfaceBinding::Iface(x, None)),
            MetadataType::Struct(fields) => {
                for (i, f) in fields.fields.iter().enumerate() {
                    if let Some(mut re) = f.meta.get_iface_binding(name, metas) {
                        let indices = match &mut re {
                            IfaceBinding::Struct(_, indices) | IfaceBinding::Iface(_, indices) => {
                                indices
                            }
                        };
                        if let Some(x) = indices {
                            x.push(i as OpIndex)
                        } else {
                            *indices = Some(vec![i as OpIndex]);
                        }
                        return Some(re);
                    }
                }
                None
            }
            _ => None,
        }
    }

    #[inline]
    pub fn get_method(&self, index: OpIndex, metas: &MetadataObjs) -> Rc<RefCell<MethodDesc>> {
        let k = self.recv_meta_key();
        let m = match &metas[k] {
            MetadataType::Named(methods, _) => methods,
            _ => unreachable!(),
        };
        m.members[index as usize].clone()
    }

    pub fn identical(&self, other: &Self, metas: &MetadataObjs) -> bool {
        (self.key == other.key) || metas[self.key].identical(&metas[other.key], metas)
    }
}

#[cfg_attr(feature = "serde_borsh", derive(BorshDeserialize, BorshSerialize))]
#[derive(Debug, Clone)]
pub struct FieldInfo {
    pub meta: Meta,
    pub name: String,
    pub tag: Option<String>,
    //pub exported: bool,
    pub embedded_indices: Option<Vec<usize>>,
}

impl FieldInfo {
    pub fn exported(&self) -> bool {
        self.name.chars().next().unwrap().is_uppercase()
    }

    pub fn lookup_tag(&self, key: &str) -> Option<String> {
        if self.tag.is_none() {
            return None;
        }

        let mut tag: &str = self.tag.as_ref().unwrap();
        while !tag.is_empty() {
            // Skip leading space.
            let i = tag.find(|c: char| c != ' ').unwrap_or(tag.len());
            tag = &tag[i..];
            if tag.is_empty() {
                break;
            }

            // Scan to colon. A space, a quote or a control character is a syntax error.
            let i = tag
                .find(|c: char| c <= ' ' || c == ':' || c == '"' || c as u32 == 0x7f)
                .unwrap_or(tag.len());

            if i == 0 || i + 1 >= tag.len() || &tag[i..i + 1] != ":" || &tag[i + 1..i + 2] != "\"" {
                break;
            }

            let name = &tag[..i];
            tag = &tag[i + 1..];

            // Scan quoted string to find value.
            let mut i = 1;
            while i < tag.len() && &tag[i..i + 1] != "\"" {
                if &tag[i..i + 1] == "\\" {
                    i += 1;
                }
                i += 1;
            }

            if i >= tag.len() {
                break;
            }

            let qvalue = &tag[..i + 1];
            tag = &tag[i + 1..];

            if key == name {
                let value = str::replace(qvalue, "\\\"", "\"")
                    .trim_matches('"')
                    .to_string();
                return Some(value);
            }
        }
        None
    }
}

#[cfg_attr(feature = "serde_borsh", derive(BorshDeserialize, BorshSerialize))]
#[derive(Debug, Clone)]
pub struct Fields {
    fields: Vec<FieldInfo>,
}

impl Fields {
    #[inline]
    pub fn new(fields: Vec<FieldInfo>) -> Fields {
        Fields { fields }
    }

    #[inline]
    pub fn infos(&self) -> &[FieldInfo] {
        self.fields.as_ref()
    }

    #[inline]
    pub fn get<'a, 'b: 'a>(&'a self, indices: &[usize], metas: &'b MetadataObjs) -> &'a FieldInfo {
        debug_assert!(indices.len() > 0);
        if indices.len() == 1 {
            self.get_non_embedded(indices[0])
        } else {
            metas[self.fields[indices[0] as usize].meta.key]
                .unwrap_named(metas)
                .as_struct()
                .get(&indices[1..], metas)
        }
    }

    #[inline]
    pub fn get_non_embedded(&self, index: usize) -> &FieldInfo {
        &self.fields[index]
    }

    #[inline]
    pub fn try_index_by_name(&self, name: &str) -> Option<usize> {
        for (i, f) in self.fields.iter().enumerate() {
            if f.name == name {
                return Some(i);
            }
        }
        None
    }

    #[inline]
    pub fn index_by_name(&self, name: &str) -> usize {
        self.try_index_by_name(name).unwrap()
    }

    #[inline]
    pub fn indices_by_name(&self, name: &str) -> Vec<usize> {
        let index = self.index_by_name(name);
        match &self.fields[index].embedded_indices {
            Some(indices) => indices.clone(),
            None => vec![index],
        }
    }

    #[inline]
    pub fn identical(&self, other: &Self, metas: &MetadataObjs) -> bool {
        if self.fields.len() != other.fields.len() {
            return false;
        }
        for (i, f) in self.fields.iter().enumerate() {
            let other_f = &other.fields[i];
            let ok = f.name == other_f.name
                && f.embedded_indices == other_f.embedded_indices
                && f.meta.identical(&other_f.meta, metas);
            if !ok {
                return false;
            }
        }
        true
    }
}

#[cfg_attr(feature = "serde_borsh", derive(BorshDeserialize, BorshSerialize))]
#[derive(Debug, Clone, Copy)]
pub struct MethodDesc {
    pub pointer_recv: bool,
    pub func: Option<FunctionKey>,
}

#[derive(Debug, Clone)]
pub struct Methods {
    pub members: Vec<Rc<RefCell<MethodDesc>>>,
    pub mapping: Map<String, OpIndex>,
}

impl Methods {
    pub fn new() -> Methods {
        Methods {
            members: vec![],
            mapping: Map::new(),
        }
    }
}

#[cfg(feature = "serde_borsh")]
impl BorshSerialize for Methods {
    fn serialize<W: BorshWrite>(&self, writer: &mut W) -> BorshResult<()> {
        let methods: Vec<MethodDesc> = self.members.iter().map(|x| *x.borrow()).collect();
        methods.serialize(writer)?;
        self.mapping.serialize(writer)
    }
}

#[cfg(feature = "serde_borsh")]
impl BorshDeserialize for Methods {
    fn deserialize(buf: &mut &[u8]) -> BorshResult<Self> {
        let methods = Vec::<MethodDesc>::deserialize(buf)?;
        let members = methods
            .into_iter()
            .map(|x| Rc::new(RefCell::new(x)))
            .collect();
        let mapping = Map::<String, OpIndex>::deserialize(buf)?;
        Ok(Methods { members, mapping })
    }
}

#[cfg_attr(feature = "serde_borsh", derive(BorshDeserialize, BorshSerialize))]
#[derive(Debug, Clone)]
pub struct SigMetadata {
    pub recv: Option<Meta>,
    pub params: Vec<Meta>,
    pub results: Vec<Meta>,
    pub variadic: Option<(Meta, Meta)>,
    pub params_type: Vec<ValueType>, // for calling FFI
}

impl Default for SigMetadata {
    fn default() -> SigMetadata {
        Self {
            recv: None,
            params: vec![],
            results: vec![],
            variadic: None,
            params_type: vec![],
        }
    }
}

impl SigMetadata {
    pub fn pointer_recv(&self) -> bool {
        match &self.recv {
            Some(r) => r.ptr_depth > 0,
            None => false,
        }
    }

    pub fn identical(&self, other: &Self, metas: &MetadataObjs) -> bool {
        if !match (&self.recv, &other.recv) {
            (None, None) => true,
            (Some(a), Some(b)) => a.identical(b, metas),
            _ => false,
        } {
            return false;
        }
        if self.params.len() != other.params.len() {
            return false;
        }
        for (i, p) in self.params.iter().enumerate() {
            if !p.identical(&other.params[i], metas) {
                return false;
            }
        }
        if self.results.len() != other.results.len() {
            return false;
        }
        for (i, r) in self.results.iter().enumerate() {
            if !r.identical(&other.results[i], metas) {
                return false;
            }
        }
        if !match (&self.variadic, &other.variadic) {
            (None, None) => true,
            (Some((a, _)), Some((b, _))) => a.identical(b, metas),
            _ => false,
        } {
            return false;
        }
        true
    }
}

#[cfg_attr(feature = "serde_borsh", derive(BorshDeserialize, BorshSerialize))]
#[derive(Debug, Clone)]
pub enum MetadataType {
    Bool,
    Int,
    Int8,
    Int16,
    Int32,
    Int64,
    Uint,
    UintPtr,
    Uint8,
    Uint16,
    Uint32,
    Uint64,
    Float32,
    Float64,
    Complex64,
    Complex128,
    UnsafePtr,
    Str,
    Array(Meta, usize),
    Slice(Meta),
    Struct(Fields),
    Signature(SigMetadata),
    Map(Meta, Meta),
    Interface(Fields),
    Channel(ChannelType, Meta),
    Named(Methods, Meta),
    None,
}

impl MetadataType {
    #[inline]
    pub fn as_signature(&self) -> &SigMetadata {
        match self {
            Self::Signature(s) => s,
            _ => unreachable!(),
        }
    }

    #[inline]
    pub fn as_interface(&self) -> &Fields {
        match self {
            Self::Interface(fields) => fields,
            _ => unreachable!(),
        }
    }

    #[inline]
    pub fn as_channel(&self) -> (&ChannelType, &Meta) {
        match self {
            Self::Channel(t, m) => (t, m),
            _ => unreachable!(),
        }
    }

    #[inline]
    pub fn as_array(&self) -> (&Meta, &usize) {
        match self {
            Self::Array(m, s) => (m, s),
            _ => unreachable!(),
        }
    }

    #[inline]
    pub fn as_slice(&self) -> &Meta {
        match self {
            Self::Slice(m) => m,
            _ => unreachable!(),
        }
    }

    #[inline]
    pub fn as_map(&self) -> (&Meta, &Meta) {
        match self {
            Self::Map(k, v) => (k, v),
            _ => unreachable!(),
        }
    }

    #[inline]
    pub fn as_struct(&self) -> &Fields {
        match self {
            Self::Struct(f) => f,
            _ => unreachable!(),
        }
    }

    #[inline]
    pub fn as_named(&self) -> (&Methods, &Meta) {
        match self {
            Self::Named(meth, meta) => (meth, meta),
            _ => unreachable!(),
        }
    }

    #[inline]
    pub fn as_named_mut(&mut self) -> (&mut Methods, &mut Meta) {
        match self {
            Self::Named(meth, meta) => (meth, meta),
            _ => unreachable!(),
        }
    }

    #[inline]
    pub fn unwrap_named<'a, 'b: 'a>(&'a self, metas: &'b MetadataObjs) -> &'a Self {
        match self {
            Self::Named(_, meta) => &metas[meta.key],
            _ => self,
        }
    }

    pub fn identical(&self, other: &Self, metas: &MetadataObjs) -> bool {
        match (self, other) {
            (Self::Bool, Self::Bool) => true,
            (Self::Int, Self::Int) => true,
            (Self::Int8, Self::Int8) => true,
            (Self::Int16, Self::Int16) => true,
            (Self::Int32, Self::Int32) => true,
            (Self::Int64, Self::Int64) => true,
            (Self::Uint8, Self::Uint8) => true,
            (Self::Uint16, Self::Uint16) => true,
            (Self::Uint32, Self::Uint32) => true,
            (Self::Uint64, Self::Uint64) => true,
            (Self::Float32, Self::Float32) => true,
            (Self::Float64, Self::Float64) => true,
            (Self::Complex64, Self::Complex64) => true,
            (Self::Complex128, Self::Complex128) => true,
            (Self::Str, Self::Str) => true,
            (Self::Struct(a), Self::Struct(b)) => a.identical(b, metas),
            (Self::Signature(a), Self::Signature(b)) => a.identical(b, metas),
            (Self::Array(a, size_a), Self::Array(b, size_b)) => {
                size_a == size_b && a.identical(b, metas)
            }
            (Self::Slice(a), Self::Slice(b)) => a.identical(b, metas),
            (Self::Map(ak, av), Self::Map(bk, bv)) => {
                ak.identical(bk, metas) && av.identical(bv, metas)
            }
            (Self::Interface(a), Self::Interface(b)) => a.identical(b, metas),
            (Self::Channel(at, avt), Self::Channel(bt, bvt)) => {
                at == bt && avt.identical(bvt, metas)
            }
            (Self::Named(_, a), Self::Named(_, b)) => a.identical(b, metas),
            _ => false,
        }
    }
}
