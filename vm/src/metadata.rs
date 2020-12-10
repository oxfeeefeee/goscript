#![macro_use]
use super::instruction::{OpIndex, ValueType};
use super::objects::{
    FunctionKey, MapObjs, MetadataKey, MetadataObjs, SliceObjs, StructObj, VMObjects,
};
use super::value::GosValue;
use std::collections::HashMap;

#[derive(Debug)]
pub struct Metadata {
    pub mbool: GosMetadata,
    pub mint: GosMetadata,
    pub mint8: GosMetadata,
    pub mint16: GosMetadata,
    pub mint32: GosMetadata,
    pub mint64: GosMetadata,
    pub muint: GosMetadata,
    pub muint8: GosMetadata,
    pub muint16: GosMetadata,
    pub muint32: GosMetadata,
    pub muint64: GosMetadata,
    pub mfloat32: GosMetadata,
    pub mfloat64: GosMetadata,
    pub mcomplex64: GosMetadata,
    pub mcomplex128: GosMetadata,
    pub mstr: GosMetadata,
    pub default_sig: GosMetadata,
}

impl Metadata {
    pub fn new(objs: &mut MetadataObjs) -> Metadata {
        Metadata {
            mbool: GosMetadata::NonPtr(objs.insert(MetadataType::Bool), false),
            mint: GosMetadata::NonPtr(objs.insert(MetadataType::Int), false),
            mint8: GosMetadata::NonPtr(objs.insert(MetadataType::Int8), false),
            mint16: GosMetadata::NonPtr(objs.insert(MetadataType::Int16), false),
            mint32: GosMetadata::NonPtr(objs.insert(MetadataType::Int32), false),
            mint64: GosMetadata::NonPtr(objs.insert(MetadataType::Int64), false),
            muint: GosMetadata::NonPtr(objs.insert(MetadataType::Uint), false),
            muint8: GosMetadata::NonPtr(objs.insert(MetadataType::Uint8), false),
            muint16: GosMetadata::NonPtr(objs.insert(MetadataType::Uint16), false),
            muint32: GosMetadata::NonPtr(objs.insert(MetadataType::Uint32), false),
            muint64: GosMetadata::NonPtr(objs.insert(MetadataType::Uint64), false),
            mfloat32: GosMetadata::NonPtr(objs.insert(MetadataType::Float32), false),
            mfloat64: GosMetadata::NonPtr(objs.insert(MetadataType::Float64), false),
            mcomplex64: GosMetadata::NonPtr(objs.insert(MetadataType::Complex64), false),
            mcomplex128: GosMetadata::NonPtr(objs.insert(MetadataType::Complex128), false),
            mstr: GosMetadata::NonPtr(
                objs.insert(MetadataType::Str(GosValue::new_str("".to_string()))),
                false,
            ),
            default_sig: GosMetadata::NonPtr(
                objs.insert(MetadataType::Signature(SigMetadata::default())),
                false,
            ),
        }
    }
}

// bool indicates if it's meta of a type
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum GosMetadata {
    Untyped,
    NonPtr(MetadataKey, bool),
    Ptr1(MetadataKey, bool),
    Ptr2(MetadataKey, bool),
    Ptr3(MetadataKey, bool),
    Ptr4(MetadataKey, bool),
    Ptr5(MetadataKey, bool),
    Ptr6(MetadataKey, bool),
    Ptr7(MetadataKey, bool),
}

impl GosMetadata {
    #[inline]
    pub fn new(v: MetadataType, metas: &mut MetadataObjs) -> GosMetadata {
        GosMetadata::NonPtr(metas.insert(v), false)
    }

    #[inline]
    pub fn new_slice(val_meta: GosMetadata, metas: &mut MetadataObjs) -> GosMetadata {
        GosMetadata::new(MetadataType::Slice(val_meta), metas)
    }

    #[inline]
    pub fn new_map(
        kmeta: GosMetadata,
        vmeta: GosMetadata,
        metas: &mut MetadataObjs,
    ) -> GosMetadata {
        GosMetadata::new(MetadataType::Map(kmeta, vmeta), metas)
    }

    #[inline]
    pub fn new_interface(fields: Fields, metas: &mut MetadataObjs) -> GosMetadata {
        GosMetadata::new(MetadataType::Interface(fields), metas)
    }

    #[inline]
    pub fn new_struct(f: Fields, objs: &mut VMObjects) -> GosMetadata {
        let field_zeros: Vec<GosValue> = f.fields.iter().map(|x| x.zero_val(objs)).collect();
        let struct_val = StructObj {
            dark: false,
            meta: GosMetadata::Untyped, // placeholder, w'll be set below
            fields: field_zeros,
        };
        let gos_struct = GosValue::new_struct(struct_val, &mut objs.structs);
        let key = objs.metas.insert(MetadataType::Struct(f, gos_struct));
        let gosm = GosMetadata::NonPtr(key, false);
        match &mut objs.metas[key] {
            MetadataType::Struct(_, v) => match v {
                GosValue::Struct(s) => s.borrow_mut().meta = gosm,
                _ => unreachable!(),
            },
            _ => unreachable!(),
        }
        gosm
    }

    pub fn new_sig(
        recv: Option<GosMetadata>,
        params: Vec<GosMetadata>,
        results: Vec<GosMetadata>,
        variadic: Option<GosMetadata>,
        metas: &mut MetadataObjs,
    ) -> GosMetadata {
        let ptypes = params.iter().map(|x| x.get_value_type(metas)).collect();
        let t = MetadataType::Signature(SigMetadata {
            recv: recv,
            params: params,
            results: results,
            variadic: variadic,
            params_type: ptypes,
        });
        GosMetadata::new(t, metas)
    }

    pub fn new_named(underlying: GosMetadata, metas: &mut MetadataObjs) -> GosMetadata {
        GosMetadata::new(
            MetadataType::Named(Methods::new(vec![], HashMap::new()), underlying),
            metas,
        )
    }

    #[inline]
    pub fn ptr_to(&self) -> GosMetadata {
        match self {
            GosMetadata::Untyped => {
                unreachable!() /* todo: panic */
            }
            GosMetadata::NonPtr(k, t) => GosMetadata::Ptr1(*k, *t),
            GosMetadata::Ptr1(k, t) => GosMetadata::Ptr2(*k, *t),
            GosMetadata::Ptr2(k, t) => GosMetadata::Ptr3(*k, *t),
            GosMetadata::Ptr3(k, t) => GosMetadata::Ptr4(*k, *t),
            GosMetadata::Ptr4(k, t) => GosMetadata::Ptr5(*k, *t),
            GosMetadata::Ptr5(k, t) => GosMetadata::Ptr6(*k, *t),
            GosMetadata::Ptr6(k, t) => GosMetadata::Ptr7(*k, *t),
            GosMetadata::Ptr7(_, _) => {
                unreachable!() /* todo: panic */
            }
        }
    }

    #[inline]
    pub fn unptr_to(&self) -> GosMetadata {
        match self {
            GosMetadata::Untyped => {
                unreachable!() /* todo: panic */
            }
            GosMetadata::NonPtr(_, _) => {
                unreachable!() /* todo: panic */
            }
            GosMetadata::Ptr1(k, t) => GosMetadata::NonPtr(*k, *t),
            GosMetadata::Ptr2(k, t) => GosMetadata::Ptr1(*k, *t),
            GosMetadata::Ptr3(k, t) => GosMetadata::Ptr2(*k, *t),
            GosMetadata::Ptr4(k, t) => GosMetadata::Ptr3(*k, *t),
            GosMetadata::Ptr5(k, t) => GosMetadata::Ptr4(*k, *t),
            GosMetadata::Ptr6(k, t) => GosMetadata::Ptr5(*k, *t),
            GosMetadata::Ptr7(k, t) => GosMetadata::Ptr6(*k, *t),
        }
    }

    // todo: change name
    #[inline]
    pub fn as_non_ptr(&self) -> MetadataKey {
        match self {
            GosMetadata::NonPtr(k, _) => *k,
            _ => unreachable!(),
        }
    }

    #[inline]
    pub fn unwrap_key_and_is_type(&self) -> (MetadataKey, bool) {
        match self {
            GosMetadata::Untyped => {
                unreachable!() /* todo: panic */
            }
            GosMetadata::NonPtr(k, t)
            | GosMetadata::Ptr1(k, t)
            | GosMetadata::Ptr2(k, t)
            | GosMetadata::Ptr3(k, t)
            | GosMetadata::Ptr4(k, t)
            | GosMetadata::Ptr5(k, t)
            | GosMetadata::Ptr6(k, t)
            | GosMetadata::Ptr7(k, t) => (*k, *t),
        }
    }

    #[inline]
    pub fn set_is_type(&mut self, is_type: bool) {
        *self = match self {
            GosMetadata::Untyped => {
                unreachable!() /* todo: panic */
            }
            GosMetadata::NonPtr(k, _) => GosMetadata::NonPtr(*k, is_type),
            GosMetadata::Ptr1(k, _) => GosMetadata::Ptr1(*k, is_type),
            GosMetadata::Ptr2(k, _) => GosMetadata::Ptr2(*k, is_type),
            GosMetadata::Ptr3(k, _) => GosMetadata::Ptr3(*k, is_type),
            GosMetadata::Ptr4(k, _) => GosMetadata::Ptr4(*k, is_type),
            GosMetadata::Ptr5(k, _) => GosMetadata::Ptr5(*k, is_type),
            GosMetadata::Ptr6(k, _) => GosMetadata::Ptr6(*k, is_type),
            GosMetadata::Ptr7(k, _) => GosMetadata::Ptr7(*k, is_type),
        }
    }

    #[inline]
    pub fn get_value_type(&self, metas: &MetadataObjs) -> ValueType {
        let (key, is_type) = self.unwrap_key_and_is_type();
        if is_type {
            ValueType::Metadata
        } else {
            match self {
                GosMetadata::Untyped => unreachable!(),
                GosMetadata::NonPtr(_, _) => match &metas[key] {
                    MetadataType::Bool => ValueType::Bool,
                    MetadataType::Int => ValueType::Int,
                    MetadataType::Int8 => ValueType::Int8,
                    MetadataType::Int16 => ValueType::Int16,
                    MetadataType::Int32 => ValueType::Int32,
                    MetadataType::Int64 => ValueType::Int64,
                    MetadataType::Uint => ValueType::Uint,
                    MetadataType::Uint8 => ValueType::Uint8,
                    MetadataType::Uint16 => ValueType::Uint16,
                    MetadataType::Uint32 => ValueType::Uint32,
                    MetadataType::Uint64 => ValueType::Uint64,
                    MetadataType::Float32 => ValueType::Float32,
                    MetadataType::Float64 => ValueType::Float64,
                    MetadataType::Complex64 => ValueType::Complex64,
                    MetadataType::Complex128 => ValueType::Complex128,
                    MetadataType::Str(_) => ValueType::Str,
                    MetadataType::Struct(_, _) => ValueType::Struct,
                    MetadataType::Signature(_) => ValueType::Closure,
                    MetadataType::Slice(_) => ValueType::Slice,
                    MetadataType::Map(_, _) => ValueType::Map,
                    MetadataType::Interface(_) => ValueType::Interface,
                    MetadataType::Channel => ValueType::Channel,
                    MetadataType::Named(_, m) => m.get_value_type(metas),
                },
                _ => ValueType::Boxed,
            }
        }
    }

    #[inline]
    pub fn zero_val(&self, objs: &VMObjects) -> GosValue {
        self.zero_val_impl(&objs.metas, &objs.metadata)
    }

    #[inline]
    fn zero_val_impl(&self, mobjs: &MetadataObjs, metadata: &Metadata) -> GosValue {
        match &self {
            GosMetadata::Untyped => GosValue::Nil(*self),
            GosMetadata::NonPtr(k, _) => match &mobjs[*k] {
                MetadataType::Bool => GosValue::Bool(false),
                MetadataType::Int => GosValue::Int(0),
                MetadataType::Int8 => GosValue::Int8(0),
                MetadataType::Int16 => GosValue::Int16(0),
                MetadataType::Int32 => GosValue::Int32(0),
                MetadataType::Int64 => GosValue::Int64(0),
                MetadataType::Uint => GosValue::Uint(0),
                MetadataType::Uint8 => GosValue::Uint8(0),
                MetadataType::Uint16 => GosValue::Uint16(0),
                MetadataType::Uint32 => GosValue::Uint32(0),
                MetadataType::Uint64 => GosValue::Uint64(0),
                MetadataType::Float32 => GosValue::Float32(0.0.into()),
                MetadataType::Float64 => GosValue::Float64(0.0.into()),
                MetadataType::Complex64 => GosValue::Complex64(0.0.into(), 0.0.into()),
                MetadataType::Complex128 => {
                    GosValue::Complex128(Box::new((0.0.into(), 0.0.into())))
                }
                MetadataType::Str(s) => s.clone(),
                MetadataType::Struct(_, s) => s.copy_semantic(None, metadata),
                MetadataType::Signature(_) => GosValue::Nil(*self),
                MetadataType::Slice(_) => GosValue::Nil(*self),
                MetadataType::Map(_, _) => GosValue::Nil(*self),
                MetadataType::Interface(_) => GosValue::Nil(*self),
                MetadataType::Channel => GosValue::Nil(*self),
                MetadataType::Named(_, gm) => gm.zero_val_impl(mobjs, metadata),
            },
            _ => GosValue::Nil(*self),
        }
    }

    #[inline]
    pub fn default_val(
        &self,
        mobjs: &MetadataObjs,
        metadata: &Metadata,
        slices: &mut SliceObjs,
        maps: &mut MapObjs,
    ) -> GosValue {
        match &self {
            GosMetadata::NonPtr(k, _) => match &mobjs[*k] {
                MetadataType::Bool => GosValue::Bool(false),
                MetadataType::Int => GosValue::Int(0),
                MetadataType::Int8 => GosValue::Int8(0),
                MetadataType::Int16 => GosValue::Int16(0),
                MetadataType::Int32 => GosValue::Int32(0),
                MetadataType::Int64 => GosValue::Int64(0),
                MetadataType::Uint => GosValue::Uint(0),
                MetadataType::Uint8 => GosValue::Uint8(0),
                MetadataType::Uint16 => GosValue::Uint16(0),
                MetadataType::Uint32 => GosValue::Uint32(0),
                MetadataType::Uint64 => GosValue::Uint64(0),
                MetadataType::Float32 => GosValue::Float32(0.0.into()),
                MetadataType::Float64 => GosValue::Float64(0.0.into()),
                MetadataType::Complex64 => GosValue::Complex64(0.0.into(), 0.0.into()),
                MetadataType::Complex128 => {
                    GosValue::Complex128(Box::new((0.0.into(), 0.0.into())))
                }
                MetadataType::Str(s) => s.clone(),
                MetadataType::Struct(_, s) => s.copy_semantic(None, metadata),
                MetadataType::Signature(_) => unimplemented!(),
                MetadataType::Slice(_) => GosValue::new_slice(0, 0, None, slices),
                MetadataType::Map(_, v) => {
                    GosValue::new_map(v.zero_val_impl(mobjs, metadata), maps)
                }
                MetadataType::Interface(_) => unimplemented!(),
                MetadataType::Channel => unimplemented!(),
                MetadataType::Named(_, gm) => gm.default_val(mobjs, metadata, slices, maps),
            },
            _ => unreachable!(),
        }
    }

    #[inline]
    pub fn field_index(&self, name: &str, metas: &MetadataObjs) -> OpIndex {
        let key = self.recv_meta_key();
        match &metas[GosMetadata::NonPtr(key, false)
            .get_underlying(metas)
            .as_non_ptr()]
        {
            MetadataType::Struct(m, _) => m.mapping[name] as OpIndex,
            _ => unreachable!(),
        }
    }

    #[inline]
    pub fn get_underlying(&self, metas: &MetadataObjs) -> GosMetadata {
        match self {
            GosMetadata::NonPtr(k, _) => match &metas[*k] {
                MetadataType::Named(_, u) => *u,
                _ => *self,
            },
            _ => *self,
        }
    }

    #[inline]
    pub fn recv_meta_key(&self) -> MetadataKey {
        match self {
            GosMetadata::NonPtr(k, _) => *k,
            GosMetadata::Ptr1(k, _) => *k,
            _ => unreachable!(),
        }
    }

    #[inline]
    pub fn add_method(&self, name: String, f: GosValue, metas: &mut MetadataObjs) {
        let k = self.recv_meta_key();
        dbg!(k, &metas[k]);
        match &mut metas[k] {
            MetadataType::Named(m, _) => {
                m.members.push(f);
                m.mapping.insert(name, m.members.len() as OpIndex - 1);
            }
            _ => unreachable!(),
        }
    }

    #[inline]
    pub fn get_named_metadate<'a>(
        &self,
        metas: &'a MetadataObjs,
    ) -> (&'a Methods, &'a GosMetadata) {
        let k = self.recv_meta_key();
        match &metas[k] {
            MetadataType::Named(methods, md) => (methods, md),
            _ => unreachable!(),
        }
    }

    #[inline]
    pub fn get_method(&self, index: OpIndex, metas: &MetadataObjs) -> GosValue {
        let (m, _) = self.get_named_metadate(metas);
        m.members[index as usize].clone()
    }

    /// method_index returns the index of the method of a non-interface
    #[inline]
    pub fn method_index(&self, name: &str, metas: &MetadataObjs) -> OpIndex {
        let (m, _) = self.get_named_metadate(metas);
        m.mapping[name] as OpIndex
    }

    /// iface_method_index returns the index of the method of an interface
    #[inline]
    pub fn iface_method_index(&self, name: &str, metas: &MetadataObjs) -> OpIndex {
        let (_, under) = self.get_named_metadate(metas);
        if let MetadataType::Interface(m) = &metas[under.as_non_ptr()] {
            m.mapping[name] as OpIndex
        } else {
            unreachable!()
        }
    }
}

#[derive(Debug, Clone)]
pub struct Fields {
    pub fields: Vec<GosMetadata>,
    pub mapping: HashMap<String, OpIndex>,
}

impl Fields {
    #[inline]
    pub fn new(fields: Vec<GosMetadata>, mapping: HashMap<String, OpIndex>) -> Fields {
        Fields {
            fields: fields,
            mapping: mapping,
        }
    }

    #[inline]
    pub fn iface_named_mapping(&self, named_obj: &Methods) -> Vec<FunctionKey> {
        let mut result = vec![null_key!(); self.fields.len()];
        for (n, i) in self.mapping.iter() {
            let f = &named_obj.members[named_obj.mapping[n] as usize];
            result[*i as usize] = f.as_closure().func();
        }
        result
    }

    pub fn iface_ffi_info(&self) -> Vec<(String, MetadataKey)> {
        let mut ret = vec![];
        for f in self.fields.iter() {
            ret.push((String::new(), f.as_non_ptr()));
        }
        for (name, index) in self.mapping.iter() {
            ret[*index as usize].0 = name.clone();
        }
        ret
    }
}

#[derive(Debug, Clone)]
pub struct Methods {
    pub members: Vec<GosValue>,
    pub mapping: HashMap<String, OpIndex>,
}

impl Methods {
    pub fn new(members: Vec<GosValue>, mapping: HashMap<String, OpIndex>) -> Methods {
        Methods {
            members: members,
            mapping: mapping,
        }
    }
}

#[derive(Debug, Clone)]
pub struct SigMetadata {
    pub recv: Option<GosMetadata>,
    pub params: Vec<GosMetadata>,
    pub results: Vec<GosMetadata>,
    pub variadic: Option<GosMetadata>,
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
    pub fn boxed_recv(&self) -> bool {
        if let Some(r) = &self.recv {
            match r {
                GosMetadata::NonPtr(_, _) => false,
                _ => true,
            }
        } else {
            false
        }
    }
}

#[derive(Debug, Clone)]
pub enum MetadataType {
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
    Float32,
    Float64,
    Complex64,
    Complex128,
    Str(GosValue),
    Struct(Fields, GosValue),
    Signature(SigMetadata),
    Slice(GosMetadata),
    Map(GosMetadata, GosMetadata),
    Interface(Fields),
    Channel, //todo
    Named(Methods, GosMetadata),
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
}
