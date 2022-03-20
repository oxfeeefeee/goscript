extern crate self as goscript_engine;
use crate::ffi::*;
use goscript_vm::instruction::ValueType;
use goscript_vm::metadata::GosMetadata;
use goscript_vm::objects::*;
use goscript_vm::value::{GosValue, IfaceUnderlying, PointerObj, UserData};
use std::any::Any;
use std::cell::RefCell;
use std::future::Future;
use std::mem;
use std::pin::Pin;
use std::rc::Rc;

const WRONG_TYPE_MSG: &str = "reflect: wrong type";
const INDEX_OOR_MSG: &str = "reflect: index out of range";

macro_rules! args_as {
    ($args:expr, $t:ty) => {{
        match &$args[0] {
            GosValue::Pointer(p) => match &*p as &PointerObj {
                PointerObj::UserData(ud) => Ok(ud.as_any().downcast_ref::<$t>().unwrap()),
                _ => Err("reflect: expect UserData".to_string()),
            },
            _ => Err("reflect: expect pointer".to_string()),
        }
    }};
}

macro_rules! wrap_std_val {
    ($v:expr, $metas:expr) => {
        GosValue::new_pointer(PointerObj::UserData(Rc::new(StdValue::new($v, &$metas))))
    };
}

macro_rules! meta_objs {
    ($ptr:expr) => {
        unsafe { &*$ptr }
    };
}

macro_rules! err_wrong_type {
    () => {
        Err(WRONG_TYPE_MSG.to_string())
    };
}

macro_rules! err_index_oor {
    () => {
        Err(INDEX_OOR_MSG.to_string())
    };
}

enum GosKind {
    Invalid = 0,
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
    _Uintptr, // do not support for now
    Float32,
    Float64,
    Complex64,
    Complex128,
    Array,
    Chan,
    Func,
    Interface,
    Map,
    Ptr,
    Slice,
    String,
    Struct,
    UnsafePointer,
}

#[derive(Ffi)]
pub struct Reflect {}

#[ffi_impl]
impl Reflect {
    pub fn new(_v: Vec<GosValue>) -> Reflect {
        Reflect {}
    }

    fn ffi_value_of(&self, ctx: &FfiCallCtx, args: Vec<GosValue>) -> GosValue {
        StdValue::value_of(&args[0], ctx)
    }

    fn ffi_type_of(&self, ctx: &FfiCallCtx, args: Vec<GosValue>) -> RuntimeResult<Vec<GosValue>> {
        let v = args_as!(args, StdValue)?;
        let (t, k) = StdType::type_of(&v.val, ctx);
        Ok(vec![t, k])
    }

    fn ffi_bool_val(&self, args: Vec<GosValue>) -> RuntimeResult<GosValue> {
        args_as!(args, StdValue)?.bool_val()
    }

    fn ffi_int_val(&self, args: Vec<GosValue>) -> RuntimeResult<GosValue> {
        args_as!(args, StdValue)?.int_val()
    }

    fn ffi_uint_val(&self, args: Vec<GosValue>) -> RuntimeResult<GosValue> {
        args_as!(args, StdValue)?.uint_val()
    }

    fn ffi_float_val(&self, args: Vec<GosValue>) -> RuntimeResult<GosValue> {
        args_as!(args, StdValue)?.float_val()
    }

    fn ffi_bytes_val(&self, args: Vec<GosValue>) -> RuntimeResult<GosValue> {
        args_as!(args, StdValue)?.bytes_val()
    }

    fn ffi_elem(&self, ctx: &FfiCallCtx, args: Vec<GosValue>) -> RuntimeResult<GosValue> {
        args_as!(args, StdValue)?.elem(ctx)
    }

    fn ffi_num_field(&self, args: Vec<GosValue>) -> RuntimeResult<GosValue> {
        args_as!(args, StdValue)?.num_field()
    }

    fn ffi_field(&self, ctx: &FfiCallCtx, args: Vec<GosValue>) -> RuntimeResult<GosValue> {
        args_as!(args, StdValue)?.field(ctx, &args[1])
    }

    fn ffi_index(&self, ctx: &FfiCallCtx, args: Vec<GosValue>) -> RuntimeResult<GosValue> {
        args_as!(args, StdValue)?.index(ctx, &args[1])
    }

    fn ffi_is_nil(&self, args: Vec<GosValue>) -> RuntimeResult<GosValue> {
        Ok(GosValue::Bool(args_as!(args, StdValue)?.val.equals_nil()))
    }

    fn ffi_len(&self, args: Vec<GosValue>) -> RuntimeResult<GosValue> {
        args_as!(args, StdValue)?.len()
    }

    fn ffi_map_range_init(&self, args: Vec<GosValue>) -> RuntimeResult<GosValue> {
        Ok(StdMapIter::map_range(args_as!(args, StdValue)?))
    }

    fn ffi_map_range_next(&self, args: Vec<GosValue>) -> RuntimeResult<GosValue> {
        Ok(args_as!(args, StdMapIter)?.next())
    }

    fn ffi_map_range_key(&self, ctx: &FfiCallCtx, args: Vec<GosValue>) -> RuntimeResult<GosValue> {
        args_as!(args, StdMapIter)?.key(ctx)
    }

    fn ffi_map_range_value(
        &self,
        ctx: &FfiCallCtx,
        args: Vec<GosValue>,
    ) -> RuntimeResult<GosValue> {
        args_as!(args, StdMapIter)?.value(ctx)
    }
}

#[derive(Clone, Debug)]
struct StdValue {
    val: GosValue,
    mobjs: *const MetadataObjs,
}

impl UserData for StdValue {
    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl StdValue {
    fn new(v: GosValue, objs: &MetadataObjs) -> StdValue {
        StdValue {
            val: v,
            mobjs: objs,
        }
    }

    fn value_of(v: &GosValue, ctx: &FfiCallCtx) -> GosValue {
        let iface = v.as_interface().borrow();
        let v = match &iface.underlying() {
            IfaceUnderlying::Gos(v, _) => v.clone(),
            // todo: should we return something else?
            IfaceUnderlying::Ffi(_) => GosValue::Nil(iface.meta),
            IfaceUnderlying::None => GosValue::Nil(iface.meta),
        };
        wrap_std_val!(v, &ctx.vm_objs.metas)
    }

    fn bool_val(&self) -> RuntimeResult<GosValue> {
        match self.val.unwrap_named_ref() {
            GosValue::Bool(_) => Ok(self.val.clone()),
            _ => err_wrong_type!(),
        }
    }

    fn int_val(&self) -> RuntimeResult<GosValue> {
        match self.val.unwrap_named_ref() {
            GosValue::Int(i) => Ok(*i as i64),
            GosValue::Int8(i) => Ok(*i as i64),
            GosValue::Int16(i) => Ok(*i as i64),
            GosValue::Int32(i) => Ok(*i as i64),
            GosValue::Int64(i) => Ok(*i),
            _ => err_wrong_type!(),
        }
        .map(|x| GosValue::Int64(x))
    }

    fn uint_val(&self) -> RuntimeResult<GosValue> {
        match self.val.unwrap_named_ref() {
            GosValue::Uint(i) => Ok(*i as u64),
            GosValue::Uint8(i) => Ok(*i as u64),
            GosValue::Uint16(i) => Ok(*i as u64),
            GosValue::Uint32(i) => Ok(*i as u64),
            GosValue::Uint64(i) => Ok(*i),
            _ => err_wrong_type!(),
        }
        .map(|x| GosValue::Uint64(x))
    }

    fn float_val(&self) -> RuntimeResult<GosValue> {
        match self.val.unwrap_named_ref() {
            GosValue::Float32(f) => Ok((Into::<f32>::into(*f) as f64).into()),
            GosValue::Float64(f) => Ok(*f),
            _ => err_wrong_type!(),
        }
        .map(|x| GosValue::Float64(x))
    }

    fn bytes_val(&self) -> RuntimeResult<GosValue> {
        match self.val.unwrap_named_ref() {
            GosValue::Slice(s) => {
                let metas = meta_objs!(self.mobjs);
                let (m, _) = metas[s.0.meta.as_non_ptr()].as_slice_or_array();
                match m.get_value_type(metas) {
                    ValueType::Uint8 => Ok(self.val.clone()),
                    _ => err_wrong_type!(),
                }
            }
            _ => err_wrong_type!(),
        }
    }

    fn elem(&self, ctx: &FfiCallCtx) -> RuntimeResult<GosValue> {
        match self.val.unwrap_named_ref() {
            GosValue::Interface(iface) => Ok(iface
                .borrow()
                .underlying_value()
                .map(|x| x.clone())
                .unwrap_or(GosValue::new_nil())),
            GosValue::Pointer(p) => Ok(p.deref(&ctx.stack, &ctx.vm_objs.packages)),
            _ => err_wrong_type!(),
        }
        .map(|x| wrap_std_val!(x, &ctx.vm_objs.metas))
    }

    fn num_field(&self) -> RuntimeResult<GosValue> {
        match self.val.try_as_struct() {
            Some(s) => Ok(GosValue::Int(s.0.borrow().fields.len() as isize)),
            None => err_wrong_type!(),
        }
    }

    fn field(&self, ctx: &FfiCallCtx, ival: &GosValue) -> RuntimeResult<GosValue> {
        let i = *ival.as_int() as usize;
        match self.val.try_as_struct() {
            Some(s) => {
                let fields = &s.0.borrow().fields;
                if fields.len() <= i {
                    err_index_oor!()
                } else {
                    Ok(fields[i].clone())
                }
            }
            None => err_wrong_type!(),
        }
        .map(|x| wrap_std_val!(x, &ctx.vm_objs.metas))
    }

    fn index(&self, ctx: &FfiCallCtx, ival: &GosValue) -> RuntimeResult<GosValue> {
        let i = *ival.as_int() as usize;
        match self.val.unwrap_named_ref() {
            GosValue::Array(arr) => match arr.0.len() > i {
                true => Ok(arr.0.get(i).unwrap()),
                false => err_index_oor!(),
            },
            GosValue::Slice(s) => match s.0.len() > i {
                true => Ok(s.0.get(i).unwrap()),
                false => err_index_oor!(),
            },
            GosValue::Str(s) => match s.len() > i {
                true => Ok(GosValue::Uint8(*s.get_byte(i).unwrap())),
                false => err_index_oor!(),
            },
            _ => err_wrong_type!(),
        }
        .map(|x| wrap_std_val!(x, &ctx.vm_objs.metas))
    }

    fn len(&self) -> RuntimeResult<GosValue> {
        match self.val.unwrap_named_ref() {
            GosValue::Array(arr) => Ok(arr.0.len()),
            GosValue::Slice(s) => Ok(s.0.len()),
            GosValue::Str(s) => Ok(s.len()),
            GosValue::Channel(c) => Ok(c.len()),
            GosValue::Map(m) => Ok(m.0.len()),
            _ => err_wrong_type!(),
        }
        .map(|x| GosValue::Int(x as isize))
    }
}

#[derive(Clone, Debug)]
struct StdType {
    meta: GosMetadata,
    mobjs: *const MetadataObjs,
}

impl UserData for StdType {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn eq(&self, other: &dyn UserData) -> bool {
        match other.as_any().downcast_ref::<StdType>() {
            Some(other_type) => {
                let objs = meta_objs!(self.mobjs);
                self.meta.identical(&other_type.meta, objs)
            }
            None => false,
        }
    }
}

impl StdType {
    fn new(m: GosMetadata, objs: &MetadataObjs) -> StdType {
        StdType {
            meta: m,
            mobjs: objs,
        }
    }

    fn type_of(val: &GosValue, ctx: &FfiCallCtx) -> (GosValue, GosValue) {
        let m = val.get_meta(ctx.vm_objs, ctx.stack);
        let typ = StdType::new(m, &ctx.vm_objs.metas);
        let kind = match m
            .get_underlying(&ctx.vm_objs.metas)
            .get_value_type(&ctx.vm_objs.metas)
        {
            ValueType::Bool => GosKind::Bool,
            ValueType::Int => GosKind::Int,
            ValueType::Int8 => GosKind::Int8,
            ValueType::Int16 => GosKind::Int16,
            ValueType::Int32 => GosKind::Int32,
            ValueType::Int64 => GosKind::Int64,
            ValueType::Uint => GosKind::Uint,
            ValueType::Uint8 => GosKind::Uint8,
            ValueType::Uint16 => GosKind::Uint16,
            ValueType::Uint32 => GosKind::Uint32,
            ValueType::Uint64 => GosKind::Uint64,
            ValueType::Float32 => GosKind::Float32,
            ValueType::Float64 => GosKind::Float64,
            ValueType::Complex64 => GosKind::Complex64,
            ValueType::Complex128 => GosKind::Complex128,
            ValueType::Array => GosKind::Array,
            ValueType::Channel => GosKind::Chan,
            ValueType::Closure => GosKind::Func,
            ValueType::Interface => GosKind::Interface,
            ValueType::Map => GosKind::Map,
            ValueType::Pointer => {
                let ptr: &PointerObj = &*val.as_pointer();
                match ptr {
                    PointerObj::UserData(_) => GosKind::UnsafePointer,
                    _ => GosKind::Ptr,
                }
            }
            ValueType::Slice => GosKind::Slice,
            ValueType::Str => GosKind::String,
            ValueType::Struct => GosKind::Struct,
            _ => GosKind::Invalid,
        };
        (
            GosValue::new_pointer(PointerObj::UserData(Rc::new(typ))),
            GosValue::Uint(kind as usize),
        )
    }
}

#[derive(Clone, Debug)]
struct StdMapIterInner {
    iter: GosHashMapIter<'static>,
    item: Option<(GosValue, GosValue)>,
}

#[derive(Clone, Debug)]
struct StdMapIter {
    inner: RefCell<StdMapIterInner>,
}

impl UserData for StdMapIter {
    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl StdMapIter {
    fn map_range(v: &StdValue) -> GosValue {
        let mref = v.val.as_map().0.borrow_data();
        let iter: GosHashMapIter<'static> = unsafe { mem::transmute(mref.iter()) };
        let smi = StdMapIter {
            inner: RefCell::new(StdMapIterInner {
                iter: iter,
                item: None,
            }),
        };
        GosValue::new_pointer(PointerObj::UserData(Rc::new(smi)))
    }

    fn next(&self) -> GosValue {
        let mut inner = self.inner.borrow_mut();
        inner.item = inner
            .iter
            .next()
            .map(|x| (x.0.clone(), x.1.clone().into_inner()));
        GosValue::Bool(inner.item.is_some())
    }

    fn key(&self, ctx: &FfiCallCtx) -> RuntimeResult<GosValue> {
        match &self.inner.borrow().item {
            Some(kv) => Ok(kv.0.clone()),
            None => Err("reflect.MapIter: Next not called or iter exhausted".to_string()),
        }
        .map(|x| wrap_std_val!(x, &ctx.vm_objs.metas))
    }

    fn value(&self, ctx: &FfiCallCtx) -> RuntimeResult<GosValue> {
        match &self.inner.borrow().item {
            Some(kv) => Ok(kv.1.clone()),
            None => Err("reflect.MapIter: Next not called or iter exhausted".to_string()),
        }
        .map(|x| wrap_std_val!(x, &ctx.vm_objs.metas))
    }
}
