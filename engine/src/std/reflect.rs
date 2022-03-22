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

macro_rules! arg_as {
    ($arg:expr, $t:ty) => {{
        match $arg {
            GosValue::Pointer(p) => match &*p as &PointerObj {
                PointerObj::UserData(ud) => Ok(ud.as_any().downcast_ref::<$t>().unwrap()),
                _ => Err("reflect: expect UserData".to_string()),
            },
            _ => Err("reflect: expect pointer".to_string()),
        }
    }};
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

#[inline]
fn wrap_std_val(v: GosValue) -> GosValue {
    GosValue::new_pointer(PointerObj::UserData(Rc::new(StdValue::new(v))))
}

#[inline]
fn wrap_ptr_std_val(p: Box<PointerObj>) -> GosValue {
    GosValue::new_pointer(PointerObj::UserData(Rc::new(StdValue::Pointer(p))))
}

#[inline]
fn meta_objs(p: *const MetadataObjs) -> &'static MetadataObjs {
    unsafe { &*p }
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
    UintPtr,
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

    fn ffi_value_of(&self, args: Vec<GosValue>) -> GosValue {
        StdValue::value_of(&args[0])
    }

    fn ffi_type_of(&self, ctx: &FfiCallCtx, args: Vec<GosValue>) -> RuntimeResult<Vec<GosValue>> {
        let v = arg_as!(&args[0], StdValue)?;
        let (t, k) = StdType::type_of(&v.val(ctx), ctx);
        Ok(vec![t, k])
    }

    fn ffi_bool_val(&self, ctx: &FfiCallCtx, args: Vec<GosValue>) -> RuntimeResult<GosValue> {
        arg_as!(&args[0], StdValue)?.bool_val(ctx)
    }

    fn ffi_int_val(&self, ctx: &FfiCallCtx, args: Vec<GosValue>) -> RuntimeResult<GosValue> {
        arg_as!(&args[0], StdValue)?.int_val(ctx)
    }

    fn ffi_uint_val(&self, ctx: &FfiCallCtx, args: Vec<GosValue>) -> RuntimeResult<GosValue> {
        arg_as!(&args[0], StdValue)?.uint_val(ctx)
    }

    fn ffi_float_val(&self, ctx: &FfiCallCtx, args: Vec<GosValue>) -> RuntimeResult<GosValue> {
        arg_as!(&args[0], StdValue)?.float_val(ctx)
    }

    fn ffi_bytes_val(&self, ctx: &FfiCallCtx, args: Vec<GosValue>) -> RuntimeResult<GosValue> {
        arg_as!(&args[0], StdValue)?.bytes_val(ctx)
    }

    fn ffi_elem(&self, ctx: &FfiCallCtx, args: Vec<GosValue>) -> RuntimeResult<GosValue> {
        arg_as!(&args[0], StdValue)?.elem(ctx)
    }

    fn ffi_num_field(&self, ctx: &FfiCallCtx, args: Vec<GosValue>) -> RuntimeResult<GosValue> {
        arg_as!(&args[0], StdValue)?.num_field(ctx)
    }

    fn ffi_field(&self, ctx: &FfiCallCtx, args: Vec<GosValue>) -> RuntimeResult<GosValue> {
        arg_as!(&args[0], StdValue)?.field(ctx, &args[1])
    }

    fn ffi_index(&self, ctx: &FfiCallCtx, args: Vec<GosValue>) -> RuntimeResult<GosValue> {
        arg_as!(&args[0], StdValue)?.index(ctx, &args[1])
    }

    fn ffi_is_nil(&self, ctx: &FfiCallCtx, args: Vec<GosValue>) -> RuntimeResult<GosValue> {
        Ok(GosValue::Bool(
            arg_as!(&args[0], StdValue)?.val(ctx).equals_nil(),
        ))
    }

    fn ffi_len(&self, ctx: &FfiCallCtx, args: Vec<GosValue>) -> RuntimeResult<GosValue> {
        arg_as!(&args[0], StdValue)?.len(ctx)
    }

    fn ffi_map_range_init(&self, ctx: &FfiCallCtx, args: Vec<GosValue>) -> RuntimeResult<GosValue> {
        Ok(StdMapIter::map_range(ctx, arg_as!(&args[0], StdValue)?))
    }

    fn ffi_map_range_next(&self, args: Vec<GosValue>) -> RuntimeResult<GosValue> {
        Ok(arg_as!(&args[0], StdMapIter)?.next())
    }

    fn ffi_map_range_key(&self, args: Vec<GosValue>) -> RuntimeResult<GosValue> {
        arg_as!(&args[0], StdMapIter)?.key()
    }

    fn ffi_map_range_value(&self, args: Vec<GosValue>) -> RuntimeResult<GosValue> {
        arg_as!(&args[0], StdMapIter)?.value()
    }

    fn ffi_can_addr(&self, args: Vec<GosValue>) -> RuntimeResult<GosValue> {
        Ok(GosValue::Bool(arg_as!(&args[0], StdValue)?.can_addr()))
    }

    fn ffi_can_set(&self, ctx: &FfiCallCtx, args: Vec<GosValue>) -> RuntimeResult<GosValue> {
        Ok(GosValue::Bool(arg_as!(&args[0], StdValue)?.can_set(ctx)))
    }

    fn ffi_set(&self, ctx: &FfiCallCtx, args: Vec<GosValue>) -> RuntimeResult<()> {
        arg_as!(&args[0], StdValue)?.set(ctx, arg_as!(&args[1], StdValue)?.val(ctx))
    }

    fn ffi_set_bool(&self, ctx: &FfiCallCtx, args: Vec<GosValue>) -> RuntimeResult<()> {
        self.set_with_val(ctx, args, ValueType::Bool)
    }

    fn set_with_val(
        &self,
        ctx: &FfiCallCtx,
        args: Vec<GosValue>,
        vt: ValueType,
    ) -> RuntimeResult<()> {
        let mut i = args.into_iter();
        let to = i.next().unwrap();
        let to = arg_as!(&to, StdValue)?;
        if to.value_type(ctx) != vt {
            Err("reflect: set value with wrong type".to_string())
        } else {
            let val = i.next().unwrap().clone();
            to.set(ctx, val)
        }
    }
}

#[derive(Clone, Debug)]
enum StdValue {
    Value(GosValue),
    Pointer(Box<PointerObj>),
}

impl UserData for StdValue {
    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl StdValue {
    fn new(v: GosValue) -> StdValue {
        match PointerObj::try_new_local(&v) {
            Some(p) => StdValue::Pointer(Box::new(p)),
            None => StdValue::Value(v),
        }
    }

    fn value_of(v: &GosValue) -> GosValue {
        let iface = v.as_interface().borrow();
        let v = match &iface.underlying() {
            IfaceUnderlying::Gos(v, _) => v.clone(),
            // todo: should we return something else?
            IfaceUnderlying::Ffi(_) => GosValue::Nil(iface.meta),
            IfaceUnderlying::None => GosValue::Nil(iface.meta),
        };
        wrap_std_val(v)
    }

    fn val(&self, ctx: &FfiCallCtx) -> GosValue {
        match self {
            Self::Value(v) => v.clone(),
            Self::Pointer(p) => p.deref(&ctx.stack, &ctx.vm_objs.packages),
        }
    }

    fn value_type(&self, ctx: &FfiCallCtx) -> ValueType {
        self.val(ctx).typ()
    }

    fn bool_val(&self, ctx: &FfiCallCtx) -> RuntimeResult<GosValue> {
        let val = self.val(ctx);
        match val.unwrap_named_ref() {
            GosValue::Bool(_) => Ok(val),
            _ => err_wrong_type!(),
        }
    }

    fn int_val(&self, ctx: &FfiCallCtx) -> RuntimeResult<GosValue> {
        let val = self.val(ctx);
        match val.unwrap_named_ref() {
            GosValue::Int(i) => Ok(*i as i64),
            GosValue::Int8(i) => Ok(*i as i64),
            GosValue::Int16(i) => Ok(*i as i64),
            GosValue::Int32(i) => Ok(*i as i64),
            GosValue::Int64(i) => Ok(*i),
            _ => err_wrong_type!(),
        }
        .map(|x| GosValue::Int64(x))
    }

    fn uint_val(&self, ctx: &FfiCallCtx) -> RuntimeResult<GosValue> {
        let val = self.val(ctx);
        match val.unwrap_named_ref() {
            GosValue::Uint(i) => Ok(*i as u64),
            GosValue::Uint8(i) => Ok(*i as u64),
            GosValue::Uint16(i) => Ok(*i as u64),
            GosValue::Uint32(i) => Ok(*i as u64),
            GosValue::Uint64(i) => Ok(*i),
            _ => err_wrong_type!(),
        }
        .map(|x| GosValue::Uint64(x))
    }

    fn float_val(&self, ctx: &FfiCallCtx) -> RuntimeResult<GosValue> {
        let val = self.val(ctx);
        match val.unwrap_named_ref() {
            GosValue::Float32(f) => Ok((Into::<f32>::into(*f) as f64).into()),
            GosValue::Float64(f) => Ok(*f),
            _ => err_wrong_type!(),
        }
        .map(|x| GosValue::Float64(x))
    }

    fn bytes_val(&self, ctx: &FfiCallCtx) -> RuntimeResult<GosValue> {
        let val = self.val(ctx);
        let mobjs = &ctx.vm_objs.metas;
        match val.unwrap_named_ref() {
            GosValue::Slice(s) => {
                let (m, _) = mobjs[s.0.meta.as_non_ptr()].as_slice_or_array();
                match m.get_value_type(mobjs) {
                    ValueType::Uint8 => Ok(val),
                    _ => err_wrong_type!(),
                }
            }
            _ => err_wrong_type!(),
        }
    }

    fn elem(&self, ctx: &FfiCallCtx) -> RuntimeResult<GosValue> {
        let val = self.val(ctx);
        let val = val.unwrap_named_ref();
        match val {
            GosValue::Interface(iface) => Ok(wrap_std_val(
                iface
                    .borrow()
                    .underlying_value()
                    .map(|x| x.clone())
                    .unwrap_or(GosValue::new_nil()),
            )),
            GosValue::Pointer(p) => Ok(wrap_ptr_std_val(p.clone())),
            _ => err_wrong_type!(),
        }
    }

    fn num_field(&self, ctx: &FfiCallCtx) -> RuntimeResult<GosValue> {
        match self.val(ctx).try_as_struct() {
            Some(s) => Ok(GosValue::Int(s.0.borrow().fields.len() as isize)),
            None => err_wrong_type!(),
        }
    }

    fn field(&self, ctx: &FfiCallCtx, ival: &GosValue) -> RuntimeResult<GosValue> {
        let i = *ival.as_int() as usize;
        match self.val(ctx).try_as_struct() {
            Some(s) => {
                let fields = &s.0.borrow().fields;
                if fields.len() <= i {
                    err_index_oor!()
                } else {
                    Ok(wrap_ptr_std_val(Box::new(PointerObj::StructField(
                        s.clone(),
                        i as i32,
                    ))))
                }
            }
            None => err_wrong_type!(),
        }
    }

    fn index(&self, ctx: &FfiCallCtx, ival: &GosValue) -> RuntimeResult<GosValue> {
        let i = *ival.as_int() as i32;
        let iusize = i as usize;
        let container = self.val(ctx);
        let container = container.unwrap_named_ref();
        match container {
            GosValue::Array(arr) => match arr.0.len() > iusize {
                true => Ok(wrap_ptr_std_val(Box::new(PointerObj::new_array_member(
                    container, i, ctx.gcv,
                )))),
                false => err_index_oor!(),
            },
            GosValue::Slice(s) => match s.0.len() > iusize {
                true => Ok(wrap_ptr_std_val(Box::new(PointerObj::SliceMember(
                    s.clone(),
                    i,
                )))),
                false => err_index_oor!(),
            },
            GosValue::Str(s) => match s.len() > iusize {
                true => Ok(wrap_std_val(GosValue::Uint8(*s.get_byte(iusize).unwrap()))),
                false => err_index_oor!(),
            },
            _ => err_wrong_type!(),
        }
    }

    fn len(&self, ctx: &FfiCallCtx) -> RuntimeResult<GosValue> {
        match self.val(ctx).unwrap_named_ref() {
            GosValue::Array(arr) => Ok(arr.0.len()),
            GosValue::Slice(s) => Ok(s.0.len()),
            GosValue::Str(s) => Ok(s.len()),
            GosValue::Channel(c) => Ok(c.len()),
            GosValue::Map(m) => Ok(m.0.len()),
            _ => err_wrong_type!(),
        }
        .map(|x| GosValue::Int(x as isize))
    }

    fn can_addr(&self) -> bool {
        match self {
            Self::Value(_) => false,
            Self::Pointer(_) => true,
        }
    }

    fn can_set(&self, ctx: &FfiCallCtx) -> bool {
        match self {
            Self::Value(_) => false,
            Self::Pointer(p) => match p as &PointerObj {
                PointerObj::SliceMember(_, _) => true,
                PointerObj::StructField(s, i) => {
                    s.0.borrow().is_exported(*i as usize, &ctx.vm_objs.metas)
                }
                PointerObj::UpVal(uv) => !uv.is_open(),
                _ => false,
            },
        }
    }

    fn set(&self, ctx: &FfiCallCtx, val: GosValue) -> RuntimeResult<()> {
        let err = Err("reflect: value is not settable".to_string());
        match self {
            Self::Value(_) => err,
            Self::Pointer(p) => match p as &PointerObj {
                PointerObj::SliceMember(s, i) => {
                    let vborrow = s.0.borrow();
                    *vborrow[s.0.begin() + *i as usize].borrow_mut() = val;
                    Ok(())
                }
                PointerObj::StructField(s, i) => {
                    if s.0.borrow().is_exported(*i as usize, &ctx.vm_objs.metas) {
                        s.0.borrow_mut().fields[*i as usize] = val;
                        Ok(())
                    } else {
                        err
                    }
                }
                PointerObj::UpVal(uv) => match &mut uv.inner.borrow_mut() as &mut UpValueState {
                    UpValueState::Open(_) => err,
                    UpValueState::Closed(c) => {
                        *c = val;
                        Ok(())
                    }
                },
                _ => err,
            },
        }
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
                let objs = meta_objs(self.mobjs);
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
        let m = val.meta(ctx.vm_objs, ctx.stack);
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
            ValueType::UintPtr => GosKind::UintPtr,
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
    fn map_range(ctx: &FfiCallCtx, v: &StdValue) -> GosValue {
        let val = v.val(ctx);
        let mref = val.as_map().0.borrow_data();
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

    fn key(&self) -> RuntimeResult<GosValue> {
        match &self.inner.borrow().item {
            Some(kv) => Ok(kv.0.clone()),
            None => Err("reflect.MapIter: Next not called or iter exhausted".to_string()),
        }
        .map(|x| wrap_std_val(x))
    }

    fn value(&self) -> RuntimeResult<GosValue> {
        match &self.inner.borrow().item {
            Some(kv) => Ok(kv.1.clone()),
            None => Err("reflect.MapIter: Next not called or iter exhausted".to_string()),
        }
        .map(|x| wrap_std_val(x))
    }
}
