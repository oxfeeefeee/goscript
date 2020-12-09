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
            mbool: GosMetadata::NonPtr(objs.insert(MetadataType::Bool)),
            mint: GosMetadata::NonPtr(objs.insert(MetadataType::Int)),
            mint8: GosMetadata::NonPtr(objs.insert(MetadataType::Int8)),
            mint16: GosMetadata::NonPtr(objs.insert(MetadataType::Int16)),
            mint32: GosMetadata::NonPtr(objs.insert(MetadataType::Int32)),
            mint64: GosMetadata::NonPtr(objs.insert(MetadataType::Int64)),
            muint: GosMetadata::NonPtr(objs.insert(MetadataType::Uint)),
            muint8: GosMetadata::NonPtr(objs.insert(MetadataType::Uint8)),
            muint16: GosMetadata::NonPtr(objs.insert(MetadataType::Uint16)),
            muint32: GosMetadata::NonPtr(objs.insert(MetadataType::Uint32)),
            muint64: GosMetadata::NonPtr(objs.insert(MetadataType::Uint64)),
            mfloat32: GosMetadata::NonPtr(objs.insert(MetadataType::Float32)),
            mfloat64: GosMetadata::NonPtr(objs.insert(MetadataType::Float64)),
            mcomplex64: GosMetadata::NonPtr(objs.insert(MetadataType::Complex64)),
            mcomplex128: GosMetadata::NonPtr(objs.insert(MetadataType::Complex128)),
            mstr: GosMetadata::NonPtr(
                objs.insert(MetadataType::Str(GosValue::new_str("".to_string()))),
            ),
            default_sig: GosMetadata::NonPtr(
                objs.insert(MetadataType::Signature(SigMetadata::default())),
            ),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum GosMetadata {
    Untyped,
    NonPtr(MetadataKey),
    Ptr1(MetadataKey),
    Ptr2(MetadataKey),
    Ptr3(MetadataKey),
    Ptr4(MetadataKey),
    Ptr5(MetadataKey),
    Ptr6(MetadataKey),
    Ptr7(MetadataKey),
    Metadata(MetadataKey),
}

impl GosMetadata {
    #[inline]
    pub fn new(v: MetadataType, metas: &mut MetadataObjs) -> GosMetadata {
        GosMetadata::NonPtr(metas.insert(v))
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
        let gosm = GosMetadata::NonPtr(key);
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
            GosMetadata::NonPtr(k) => GosMetadata::Ptr1(*k),
            GosMetadata::Ptr1(k) => GosMetadata::Ptr2(*k),
            GosMetadata::Ptr2(k) => GosMetadata::Ptr3(*k),
            GosMetadata::Ptr3(k) => GosMetadata::Ptr4(*k),
            GosMetadata::Ptr4(k) => GosMetadata::Ptr5(*k),
            GosMetadata::Ptr5(k) => GosMetadata::Ptr6(*k),
            GosMetadata::Ptr6(k) => GosMetadata::Ptr7(*k),
            GosMetadata::Ptr7(_) => {
                unreachable!() /* todo: panic */
            }
            GosMetadata::Metadata(_) => {
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
            GosMetadata::NonPtr(_) => {
                unreachable!() /* todo: panic */
            }
            GosMetadata::Ptr1(k) => GosMetadata::NonPtr(*k),
            GosMetadata::Ptr2(k) => GosMetadata::Ptr1(*k),
            GosMetadata::Ptr3(k) => GosMetadata::Ptr2(*k),
            GosMetadata::Ptr4(k) => GosMetadata::Ptr3(*k),
            GosMetadata::Ptr5(k) => GosMetadata::Ptr4(*k),
            GosMetadata::Ptr6(k) => GosMetadata::Ptr5(*k),
            GosMetadata::Ptr7(k) => GosMetadata::Ptr6(*k),
            GosMetadata::Metadata(_) => {
                unreachable!() /* todo: panic */
            }
        }
    }

    // todo: change name
    #[inline]
    pub fn as_non_ptr(&self) -> MetadataKey {
        match self {
            GosMetadata::NonPtr(k) => *k,
            GosMetadata::Metadata(k) => *k,
            _ => unreachable!(),
        }
    }

    #[inline]
    pub fn as_metadata(&self) -> MetadataKey {
        match self {
            GosMetadata::Metadata(k) => *k,
            _ => unreachable!(),
        }
    }

    #[inline]
    pub fn get_value_type(&self, metas: &MetadataObjs) -> ValueType {
        match self {
            GosMetadata::Untyped => unreachable!(),
            GosMetadata::NonPtr(k) => match &metas[*k] {
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
            GosMetadata::Metadata(_) => ValueType::Metadata,
            _ => ValueType::Boxed,
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
            GosMetadata::NonPtr(k) => match &mobjs[*k] {
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
            GosMetadata::NonPtr(k) => match &mobjs[*k] {
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
        match &metas[GosMetadata::NonPtr(key).get_underlying(metas).as_non_ptr()] {
            MetadataType::Struct(m, _) => m.mapping[name] as OpIndex,
            _ => unreachable!(),
        }
    }

    #[inline]
    pub fn get_underlying(&self, metas: &MetadataObjs) -> GosMetadata {
        match self {
            GosMetadata::NonPtr(k) => match &metas[*k] {
                MetadataType::Named(_, u) => *u,
                _ => *self,
            },
            _ => *self,
        }
    }

    #[inline]
    pub fn recv_meta_key(&self) -> MetadataKey {
        match self {
            GosMetadata::Metadata(k) => *k,
            GosMetadata::NonPtr(k) => *k,
            GosMetadata::Ptr1(k) => *k,
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
                GosMetadata::NonPtr(_) => false,
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
