use super::gc::GcoVec;
use super::instruction::{OpIndex, ValueType};
use super::objects::{FunctionKey, IfaceBinding, MetadataKey, MetadataObjs, StructObj, VMObjects};
use super::value::GosValue;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

#[macro_export]
macro_rules! zero_val {
    ($meta:expr, $objs:expr, $gcv:expr) => {
        $meta.zero_val(&$objs.metas, $gcv)
    };
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub enum ChannelType {
    Send,
    Recv,
    SendRecv,
}

#[derive(Debug)]
pub struct Metadata {
    pub mbool: GosMetadata,
    pub mint: GosMetadata,
    pub mint8: GosMetadata,
    pub mint16: GosMetadata,
    pub mint32: GosMetadata,
    pub mint64: GosMetadata,
    pub muint: GosMetadata,
    pub muint_ptr: GosMetadata,
    pub muint8: GosMetadata,
    pub muint16: GosMetadata,
    pub muint32: GosMetadata,
    pub muint64: GosMetadata,
    pub mfloat32: GosMetadata,
    pub mfloat64: GosMetadata,
    pub mcomplex64: GosMetadata,
    pub mcomplex128: GosMetadata,
    pub mstr: GosMetadata,
    pub unsafe_ptr: GosMetadata,
    pub default_sig: GosMetadata,
    pub empty_iface: GosMetadata,
    pub none: GosMetadata,
}

impl Metadata {
    pub fn new(objs: &mut MetadataObjs) -> Metadata {
        Metadata {
            mbool: GosMetadata::NonPtr(objs.insert(MetadataType::Bool), MetaCategory::Default),
            mint: GosMetadata::NonPtr(objs.insert(MetadataType::Int), MetaCategory::Default),
            mint8: GosMetadata::NonPtr(objs.insert(MetadataType::Int8), MetaCategory::Default),
            mint16: GosMetadata::NonPtr(objs.insert(MetadataType::Int16), MetaCategory::Default),
            mint32: GosMetadata::NonPtr(objs.insert(MetadataType::Int32), MetaCategory::Default),
            mint64: GosMetadata::NonPtr(objs.insert(MetadataType::Int64), MetaCategory::Default),
            muint: GosMetadata::NonPtr(objs.insert(MetadataType::Uint), MetaCategory::Default),
            muint_ptr: GosMetadata::NonPtr(
                objs.insert(MetadataType::UintPtr),
                MetaCategory::Default,
            ),
            muint8: GosMetadata::NonPtr(objs.insert(MetadataType::Uint8), MetaCategory::Default),
            muint16: GosMetadata::NonPtr(objs.insert(MetadataType::Uint16), MetaCategory::Default),
            muint32: GosMetadata::NonPtr(objs.insert(MetadataType::Uint32), MetaCategory::Default),
            muint64: GosMetadata::NonPtr(objs.insert(MetadataType::Uint64), MetaCategory::Default),
            mfloat32: GosMetadata::NonPtr(
                objs.insert(MetadataType::Float32),
                MetaCategory::Default,
            ),
            mfloat64: GosMetadata::NonPtr(
                objs.insert(MetadataType::Float64),
                MetaCategory::Default,
            ),
            mcomplex64: GosMetadata::NonPtr(
                objs.insert(MetadataType::Complex64),
                MetaCategory::Default,
            ),
            mcomplex128: GosMetadata::NonPtr(
                objs.insert(MetadataType::Complex128),
                MetaCategory::Default,
            ),
            mstr: GosMetadata::NonPtr(
                objs.insert(MetadataType::Str(GosValue::new_str("".to_owned()))),
                MetaCategory::Default,
            ),
            // todo: do we need a dedicated MetadataType::udata for it?
            unsafe_ptr: GosMetadata::Ptr1(objs.insert(MetadataType::Uint), MetaCategory::Default),
            default_sig: GosMetadata::NonPtr(
                objs.insert(MetadataType::Signature(SigMetadata::default())),
                MetaCategory::Default,
            ),
            empty_iface: GosMetadata::NonPtr(
                objs.insert(MetadataType::Interface(Fields::new(vec![], HashMap::new()))),
                MetaCategory::Default,
            ),
            none: GosMetadata::NonPtr(objs.insert(MetadataType::None), MetaCategory::Default),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MetaCategory {
    Default,
    Array,
    Type,
    ArrayType,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum GosMetadata {
    NonPtr(MetadataKey, MetaCategory),
    Ptr1(MetadataKey, MetaCategory),
    Ptr2(MetadataKey, MetaCategory),
    Ptr3(MetadataKey, MetaCategory),
    Ptr4(MetadataKey, MetaCategory),
    Ptr5(MetadataKey, MetaCategory),
    Ptr6(MetadataKey, MetaCategory),
    Ptr7(MetadataKey, MetaCategory),
}

impl GosMetadata {
    #[inline]
    pub fn new(v: MetadataType, metas: &mut MetadataObjs) -> GosMetadata {
        GosMetadata::NonPtr(metas.insert(v), MetaCategory::Default)
    }

    #[inline]
    pub fn new_array(elem_meta: GosMetadata, size: usize, metas: &mut MetadataObjs) -> GosMetadata {
        let t = MetadataType::SliceOrArray(elem_meta, size);
        GosMetadata::NonPtr(metas.insert(t), MetaCategory::Array)
    }

    #[inline]
    pub fn new_slice(val_meta: GosMetadata, metas: &mut MetadataObjs) -> GosMetadata {
        GosMetadata::new(MetadataType::SliceOrArray(val_meta, 0), metas)
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
    pub fn new_channel(
        typ: ChannelType,
        val_meta: GosMetadata,
        metas: &mut MetadataObjs,
    ) -> GosMetadata {
        GosMetadata::new(MetadataType::Channel(typ, val_meta), metas)
    }

    #[inline]
    pub fn new_struct(f: Fields, objs: &mut VMObjects, gcv: &mut GcoVec) -> GosMetadata {
        let field_zeros: Vec<GosValue> =
            f.fields.iter().map(|x| zero_val!(x.0, objs, gcv)).collect();
        let struct_val = StructObj {
            meta: objs.metadata.none, // placeholder, will be set below
            fields: field_zeros,
        };
        let gos_struct = GosValue::new_struct(struct_val, gcv);
        let key = objs.metas.insert(MetadataType::Struct(f, gos_struct));
        let gosm = GosMetadata::NonPtr(key, MetaCategory::Default);
        match &mut objs.metas[key] {
            MetadataType::Struct(_, v) => match v {
                GosValue::Struct(s) => s.0.borrow_mut().meta = gosm,
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
        variadic: Option<(GosMetadata, GosMetadata)>,
        metas: &mut MetadataObjs,
    ) -> GosMetadata {
        let ptypes = params.iter().map(|x| x.value_type(metas)).collect();
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
        //debug_assert!(underlying.value_type(metas) != ValueType::Named);
        GosMetadata::new(MetadataType::Named(Methods::new(), underlying), metas)
    }

    pub fn new_slice_from_array(array: GosMetadata) -> GosMetadata {
        GosMetadata::NonPtr(array.as_non_ptr(), MetaCategory::Default)
    }

    #[inline]
    pub fn ptr_to(&self) -> GosMetadata {
        match self {
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

    #[inline]
    pub fn as_non_ptr(&self) -> MetadataKey {
        match self {
            GosMetadata::NonPtr(k, _) => *k,
            _ => unreachable!(),
        }
    }

    #[inline]
    pub fn unwrap_non_ptr(&self) -> (MetadataKey, MetaCategory) {
        match self {
            GosMetadata::NonPtr(k, mc) => (*k, *mc),
            _ => unreachable!(),
        }
    }

    #[inline]
    pub fn unwrap_non_ptr_or_prt1(&self) -> (MetadataKey, MetaCategory) {
        match self {
            GosMetadata::NonPtr(k, mc) => (*k, *mc),
            GosMetadata::Ptr1(k, mc) => (*k, *mc),
            _ => unreachable!(),
        }
    }

    #[inline]
    pub fn into_type_category(self) -> GosMetadata {
        let convert = |c| match c {
            MetaCategory::Default => MetaCategory::Type,
            MetaCategory::Array => MetaCategory::ArrayType,
            _ => c,
        };
        match self {
            GosMetadata::NonPtr(k, c) => GosMetadata::NonPtr(k, convert(c)),
            GosMetadata::Ptr1(k, c) => GosMetadata::Ptr1(k, convert(c)),
            GosMetadata::Ptr2(k, c) => GosMetadata::Ptr2(k, convert(c)),
            GosMetadata::Ptr3(k, c) => GosMetadata::Ptr3(k, convert(c)),
            GosMetadata::Ptr4(k, c) => GosMetadata::Ptr4(k, convert(c)),
            GosMetadata::Ptr5(k, c) => GosMetadata::Ptr5(k, convert(c)),
            GosMetadata::Ptr6(k, c) => GosMetadata::Ptr6(k, convert(c)),
            GosMetadata::Ptr7(k, c) => GosMetadata::Ptr7(k, convert(c)),
        }
    }

    #[inline]
    pub fn into_value_category(self) -> GosMetadata {
        let convert = |c| match c {
            MetaCategory::Type => MetaCategory::Default,
            MetaCategory::ArrayType => MetaCategory::Array,
            _ => c,
        };
        match self {
            GosMetadata::NonPtr(k, c) => GosMetadata::NonPtr(k, convert(c)),
            GosMetadata::Ptr1(k, c) => GosMetadata::Ptr1(k, convert(c)),
            GosMetadata::Ptr2(k, c) => GosMetadata::Ptr2(k, convert(c)),
            GosMetadata::Ptr3(k, c) => GosMetadata::Ptr3(k, convert(c)),
            GosMetadata::Ptr4(k, c) => GosMetadata::Ptr4(k, convert(c)),
            GosMetadata::Ptr5(k, c) => GosMetadata::Ptr5(k, convert(c)),
            GosMetadata::Ptr6(k, c) => GosMetadata::Ptr6(k, convert(c)),
            GosMetadata::Ptr7(k, c) => GosMetadata::Ptr7(k, convert(c)),
        }
    }

    #[inline]
    pub fn value_type(&self, metas: &MetadataObjs) -> ValueType {
        match self {
            GosMetadata::NonPtr(k, mc) => match mc {
                MetaCategory::Default => match &metas[*k] {
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
                    MetadataType::Str(_) => ValueType::Str,
                    MetadataType::Struct(_, _) => ValueType::Struct,
                    MetadataType::Signature(_) => ValueType::Closure,
                    MetadataType::SliceOrArray(_, _) => ValueType::Slice,
                    MetadataType::Map(_, _) => ValueType::Map,
                    MetadataType::Interface(_) => ValueType::Interface,
                    MetadataType::Channel(_, _) => ValueType::Channel,
                    MetadataType::Named(_, _) => ValueType::Named,
                    MetadataType::None => ValueType::Nil,
                },
                MetaCategory::Type | MetaCategory::ArrayType => ValueType::Metadata,
                MetaCategory::Array => ValueType::Array,
            },
            _ => ValueType::Pointer,
        }
    }

    #[inline]
    pub fn zero_val(&self, mobjs: &MetadataObjs, gcos: &GcoVec) -> GosValue {
        self.zero_val_impl(mobjs, gcos)
    }

    #[inline]
    fn zero_val_impl(&self, mobjs: &MetadataObjs, gcos: &GcoVec) -> GosValue {
        match &self {
            GosMetadata::NonPtr(k, mc) => match &mobjs[*k] {
                MetadataType::Bool => GosValue::new_bool(false),
                MetadataType::Int => GosValue::new_int(0),
                MetadataType::Int8 => GosValue::new_int8(0),
                MetadataType::Int16 => GosValue::new_int16(0),
                MetadataType::Int32 => GosValue::new_int32(0),
                MetadataType::Int64 => GosValue::new_int64(0),
                MetadataType::Uint => GosValue::new_uint(0),
                MetadataType::UintPtr => GosValue::new_uint_ptr(0),
                MetadataType::Uint8 => GosValue::new_uint8(0),
                MetadataType::Uint16 => GosValue::new_uint16(0),
                MetadataType::Uint32 => GosValue::new_uint32(0),
                MetadataType::Uint64 => GosValue::new_uint64(0),
                MetadataType::Float32 => GosValue::new_float32(0.0.into()),
                MetadataType::Float64 => GosValue::new_float64(0.0.into()),
                MetadataType::Complex64 => GosValue::new_complex64(0.0.into(), 0.0.into()),
                MetadataType::Complex128 => {
                    GosValue::Complex128(Box::new((0.0.into(), 0.0.into())))
                }
                MetadataType::Str(s) => s.clone(),
                MetadataType::SliceOrArray(m, size) => match mc {
                    MetaCategory::Array => {
                        let val = m.zero_val_impl(mobjs, gcos);
                        GosValue::array_with_size(*size, &val, *self, gcos)
                    }
                    MetaCategory::Default => GosValue::new_slice_nil(*self, gcos),
                    _ => unreachable!(),
                },
                MetadataType::Struct(_, s) => s.copy_semantic(gcos),
                MetadataType::Signature(_) => GosValue::Nil(Some(*self)),
                MetadataType::Map(_, v) => {
                    GosValue::new_map_nil(*self, v.zero_val_impl(mobjs, gcos), gcos)
                }
                MetadataType::Interface(_) => GosValue::Nil(Some(*self)),
                MetadataType::Channel(_, _) => GosValue::Nil(Some(*self)),
                MetadataType::Named(_, gm) => {
                    let val = gm.zero_val_impl(mobjs, gcos);
                    GosValue::Named(Box::new((val, *gm)))
                }
                MetadataType::None => GosValue::Nil(Some(*self)),
            },
            _ => GosValue::Nil(Some(*self)),
        }
    }

    #[inline]
    pub fn field_index(&self, name: &str, metas: &MetadataObjs) -> OpIndex {
        let key = self.recv_meta_key();
        match &metas[GosMetadata::NonPtr(key, MetaCategory::Default)
            .underlying(metas)
            .as_non_ptr()]
        {
            MetadataType::Struct(m, _) => m.mapping[name] as OpIndex,
            _ => unreachable!(),
        }
    }

    #[inline]
    pub fn underlying(&self, metas: &MetadataObjs) -> GosMetadata {
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
        let (mkey, _) = self.unwrap_non_ptr_or_prt1();
        match &metas[mkey] {
            MetadataType::Named(m, underlying) => match m.mapping.get(name) {
                Some(&i) => Some(IfaceBinding::Struct(m.members[i as usize].clone(), None)),
                None => underlying.get_iface_binding(name, metas),
            },
            MetadataType::Interface(fields) => fields
                .mapping
                .get(name)
                .map(|x| IfaceBinding::Iface(*x, None)),
            MetadataType::Struct(fields, _) => {
                for (i, f) in fields.fields.iter().enumerate() {
                    if let Some(mut re) = f.0.get_iface_binding(name, metas) {
                        let indices = match &mut re {
                            IfaceBinding::Struct(_, indices) | IfaceBinding::Iface(_, indices) => {
                                indices
                            }
                        };
                        if let Some(x) = indices {
                            x.push(i)
                        } else {
                            *indices = Some(vec![i]);
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
        match (self, other) {
            (Self::NonPtr(ak, ac), Self::NonPtr(bk, bc)) => {
                Self::semantic_eq_impl(ak, ac, bk, bc, metas)
            }
            (Self::Ptr1(ak, ac), Self::Ptr1(bk, bc)) => {
                Self::semantic_eq_impl(ak, ac, bk, bc, metas)
            }
            (Self::Ptr2(ak, ac), Self::Ptr2(bk, bc)) => {
                Self::semantic_eq_impl(ak, ac, bk, bc, metas)
            }
            (Self::Ptr3(ak, ac), Self::Ptr3(bk, bc)) => {
                Self::semantic_eq_impl(ak, ac, bk, bc, metas)
            }
            (Self::Ptr4(ak, ac), Self::Ptr4(bk, bc)) => {
                Self::semantic_eq_impl(ak, ac, bk, bc, metas)
            }
            (Self::Ptr5(ak, ac), Self::Ptr5(bk, bc)) => {
                Self::semantic_eq_impl(ak, ac, bk, bc, metas)
            }
            (Self::Ptr6(ak, ac), Self::Ptr6(bk, bc)) => {
                Self::semantic_eq_impl(ak, ac, bk, bc, metas)
            }
            (Self::Ptr7(ak, ac), Self::Ptr7(bk, bc)) => {
                Self::semantic_eq_impl(ak, ac, bk, bc, metas)
            }
            _ => false,
        }
    }

    #[inline]
    fn semantic_eq_impl(
        ak: &MetadataKey,
        ac: &MetaCategory,
        bk: &MetadataKey,
        bc: &MetaCategory,
        metas: &MetadataObjs,
    ) -> bool {
        (ac == bc) && ((ak == bk) || metas[*ak].identical(&metas[*bk], *ac, metas))
    }
}

#[derive(Debug, Clone)]
pub struct Fields {
    pub fields: Vec<(GosMetadata, String, bool)>,
    pub mapping: HashMap<String, usize>,
}

impl Fields {
    #[inline]
    pub fn new(
        fields: Vec<(GosMetadata, String, bool)>,
        mapping: HashMap<String, usize>,
    ) -> Fields {
        Fields {
            fields: fields,
            mapping: mapping,
        }
    }

    #[inline]
    pub fn is_exported(&self, i: usize) -> bool {
        self.fields[i].2
    }

    pub fn iface_methods_info(&self) -> Vec<(String, GosMetadata)> {
        let mut ret = vec![];
        for f in self.fields.iter() {
            ret.push((String::new(), f.0));
        }
        for (name, index) in self.mapping.iter() {
            ret[*index as usize].0 = name.clone();
        }
        ret
    }

    pub fn identical(&self, other: &Self, metas: &MetadataObjs) -> bool {
        if self.fields.len() != other.fields.len() {
            return false;
        }
        for (i, f) in self.fields.iter().enumerate() {
            if f.1 == other.fields[i].1 && !f.0.identical(&other.fields[i].0, metas) {
                return false;
            }
        }
        true
    }
}

#[derive(Debug, Clone)]
pub struct MethodDesc {
    pub pointer_recv: bool,
    pub func: Option<FunctionKey>,
}

#[derive(Debug, Clone)]
pub struct Methods {
    pub members: Vec<Rc<RefCell<MethodDesc>>>,
    pub mapping: HashMap<String, OpIndex>,
}

impl Methods {
    pub fn new() -> Methods {
        Methods {
            members: vec![],
            mapping: HashMap::new(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct SigMetadata {
    pub recv: Option<GosMetadata>,
    pub params: Vec<GosMetadata>,
    pub results: Vec<GosMetadata>,
    pub variadic: Option<(GosMetadata, GosMetadata)>,
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
        if let Some(r) = &self.recv {
            match r {
                GosMetadata::NonPtr(_, _) => false,
                _ => true,
            }
        } else {
            false
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
    Str(GosValue),
    SliceOrArray(GosMetadata, usize),
    Struct(Fields, GosValue),
    Signature(SigMetadata),
    Map(GosMetadata, GosMetadata),
    Interface(Fields),
    Channel(ChannelType, GosMetadata),
    Named(Methods, GosMetadata),
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
    pub fn as_channel(&self) -> (&ChannelType, &GosMetadata) {
        match self {
            Self::Channel(t, m) => (t, m),
            _ => unreachable!(),
        }
    }

    #[inline]
    pub fn as_slice_or_array(&self) -> (&GosMetadata, &usize) {
        match self {
            Self::SliceOrArray(m, s) => (m, s),
            _ => unreachable!(),
        }
    }

    #[inline]
    pub fn as_struct(&self) -> (&Fields, &GosValue) {
        match self {
            Self::Struct(f, v) => (f, v),
            _ => unreachable!(),
        }
    }

    #[inline]
    pub fn as_named_mut(&mut self) -> (&mut Methods, &mut GosMetadata) {
        match self {
            Self::Named(meth, meta) => (meth, meta),
            _ => unreachable!(),
        }
    }

    pub fn identical(&self, other: &Self, mc: MetaCategory, metas: &MetadataObjs) -> bool {
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
            (Self::Str(_), Self::Str(_)) => true,
            (Self::Struct(a, _), Self::Struct(b, _)) => a.identical(b, metas),
            (Self::Signature(a), Self::Signature(b)) => a.identical(b, metas),
            (Self::SliceOrArray(a, size_a), Self::SliceOrArray(b, size_b)) => {
                match mc {
                    MetaCategory::Array | MetaCategory::ArrayType => {
                        if size_a != size_b {
                            return false;
                        }
                    }
                    _ => {}
                }
                a.identical(b, metas)
            }
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
