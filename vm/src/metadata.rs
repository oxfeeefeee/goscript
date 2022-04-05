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
pub struct StaticMeta {
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

impl StaticMeta {
    pub fn new(objs: &mut MetadataObjs) -> StaticMeta {
        StaticMeta {
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
            mstr: Meta::with_type(MetadataType::Str(GosValue::new_str("".to_owned())), objs),
            unsafe_ptr: Meta::new(objs.insert(MetadataType::Uint), MetaCategory::Default, 1),
            default_sig: Meta::with_type(MetadataType::Signature(SigMetadata::default()), objs),
            empty_iface: Meta::with_type(
                MetadataType::Interface(Fields::new(vec![], HashMap::new())),
                objs,
            ),
            none: Meta::with_type(MetadataType::None, objs),
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
pub struct Meta {
    pub key: MetadataKey,
    pub category: MetaCategory,
    pub ptr_depth: u8,
}

impl Meta {
    #[inline]
    pub fn new(key: MetadataKey, category: MetaCategory, pdepth: u8) -> Meta {
        Meta {
            key: key,
            category: category,
            ptr_depth: pdepth,
        }
    }

    #[inline]
    pub fn with_type(v: MetadataType, metas: &mut MetadataObjs) -> Meta {
        Meta::new(metas.insert(v), MetaCategory::Default, 0)
    }

    #[inline]
    pub fn new_array(elem_meta: Meta, size: usize, metas: &mut MetadataObjs) -> Meta {
        let t = MetadataType::SliceOrArray(elem_meta, size);
        Meta {
            key: metas.insert(t),
            category: MetaCategory::Array,
            ptr_depth: 0,
        }
    }

    #[inline]
    pub fn new_slice(val_meta: Meta, metas: &mut MetadataObjs) -> Meta {
        Meta::with_type(MetadataType::SliceOrArray(val_meta, 0), metas)
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
    pub fn new_struct(f: Fields, objs: &mut VMObjects, gcv: &mut GcoVec) -> Meta {
        let field_zeros: Vec<GosValue> =
            f.fields.iter().map(|x| zero_val!(x.0, objs, gcv)).collect();
        let struct_val = StructObj {
            meta: objs.s_meta.none, // placeholder, will be set below
            fields: field_zeros,
        };
        let gos_struct = GosValue::new_struct(struct_val, gcv);
        let key = objs.metas.insert(MetadataType::Struct(f, gos_struct));
        let gosm = Meta::new(key, MetaCategory::Default, 0);
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
        recv: Option<Meta>,
        params: Vec<Meta>,
        results: Vec<Meta>,
        variadic: Option<(Meta, Meta)>,
        metas: &mut MetadataObjs,
    ) -> Meta {
        let ptypes = params.iter().map(|x| x.value_type(metas)).collect();
        let t = MetadataType::Signature(SigMetadata {
            recv: recv,
            params: params,
            results: results,
            variadic: variadic,
            params_type: ptypes,
        });
        Meta::with_type(t, metas)
    }

    pub fn new_named(underlying: Meta, metas: &mut MetadataObjs) -> Meta {
        //debug_assert!(underlying.value_type(metas) != ValueType::Named);
        Meta::with_type(MetadataType::Named(Methods::new(), underlying), metas)
    }

    pub fn new_slice_from_array(array: Meta) -> Meta {
        Meta::new(array.key, MetaCategory::Default, 0)
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
        self.category = match self.category {
            MetaCategory::Default => MetaCategory::Type,
            MetaCategory::Array => MetaCategory::ArrayType,
            _ => self.category,
        };
        self
    }

    #[inline]
    pub fn into_value_category(mut self) -> Meta {
        self.category = match self.category {
            MetaCategory::Type => MetaCategory::Default,
            MetaCategory::ArrayType => MetaCategory::Array,
            _ => self.category,
        };
        self
    }

    #[inline]
    pub fn value_type(&self, metas: &MetadataObjs) -> ValueType {
        match self.ptr_depth {
            0 => match self.category {
                MetaCategory::Default => match &metas[self.key] {
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
        match self.ptr_depth {
            0 => match &mobjs[self.key] {
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
                MetadataType::SliceOrArray(m, size) => match self.category {
                    MetaCategory::Array => {
                        let val = m.zero_val_impl(mobjs, gcos);
                        GosValue::array_with_size(*size, &val, *self, gcos)
                    }
                    MetaCategory::Default => GosValue::Nil(Some(*self)),
                    _ => unreachable!(),
                },
                MetadataType::Struct(_, s) => s.copy_semantic(gcos),
                MetadataType::Signature(_) => GosValue::Nil(Some(*self)),
                MetadataType::Map(_, _) => GosValue::Nil(Some(*self)),
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
        match &metas[Meta::new(key, MetaCategory::Default, 0)
            .underlying(metas)
            .key]
        {
            MetadataType::Struct(m, _) => m.mapping[name] as OpIndex,
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
        (self.category == other.category)
            && ((self.key == other.key)
                || metas[self.key].identical(&metas[other.key], self.category, metas))
    }
}

#[derive(Debug, Clone)]
pub struct Fields {
    pub fields: Vec<(Meta, String, bool)>,
    pub mapping: HashMap<String, usize>,
}

impl Fields {
    #[inline]
    pub fn new(fields: Vec<(Meta, String, bool)>, mapping: HashMap<String, usize>) -> Fields {
        Fields {
            fields: fields,
            mapping: mapping,
        }
    }

    #[inline]
    pub fn is_exported(&self, i: usize) -> bool {
        self.fields[i].2
    }

    pub fn iface_methods_info(&self) -> Vec<(String, Meta)> {
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
    SliceOrArray(Meta, usize),
    Struct(Fields, GosValue),
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
    pub fn as_slice_or_array(&self) -> (&Meta, &usize) {
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
    pub fn as_named_mut(&mut self) -> (&mut Methods, &mut Meta) {
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
