extern crate self as goscript_engine;
use crate::ffi::*;
use goscript_vm::instruction::ValueType;
use goscript_vm::metadata::Meta;
use goscript_vm::objects::*;
use goscript_vm::value::{GosValue, InterfaceObj, PointerObj, UnsafePtr};
use std::any::Any;
use std::cell::RefCell;
use std::future::Future;
use std::mem;
use std::pin::Pin;
use std::rc::Rc;

macro_rules! arg_as {
    ($arg:expr, $t:ty) => {{
        match $arg {
            GosValue::UnsafePtr(ud) => Ok(ud.as_any().downcast_ref::<$t>().unwrap()),
            _ => Err("reflect: expect UnsafePtr".to_owned()),
        }
    }};
}

macro_rules! err_wrong_type {
    () => {
        Err("reflect: wrong type".to_owned())
    };
}

macro_rules! err_index_oor {
    () => {
        Err("reflect: index out of range".to_owned())
    };
}

macro_rules! err_set_val_type {
    () => {
        Err("reflect: set value with wrong type".to_owned())
    };
}

#[inline]
fn wrap_std_val(v: GosValue) -> GosValue {
    GosValue::new_unsafe_ptr(StdValue::new(v))
}

#[inline]
fn wrap_ptr_std_val(p: Box<PointerObj>) -> GosValue {
    GosValue::new_unsafe_ptr(StdValue::Pointer(p))
}

#[inline]
fn unwrap_set_args(args: &Vec<GosValue>) -> RuntimeResult<(&StdValue, GosValue)> {
    Ok((arg_as!(&args[0], StdValue)?, args[1].clone()))
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
    UnsafePtr,
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
        Ok(GosValue::new_bool(
            arg_as!(&args[0], StdValue)?.val(ctx).is_nil(),
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
        Ok(GosValue::new_bool(arg_as!(&args[0], StdValue)?.can_addr()))
    }

    fn ffi_can_set(&self, ctx: &FfiCallCtx, args: Vec<GosValue>) -> RuntimeResult<GosValue> {
        Ok(GosValue::new_bool(
            arg_as!(&args[0], StdValue)?.can_set(ctx),
        ))
    }

    fn ffi_set(&self, ctx: &FfiCallCtx, args: Vec<GosValue>) -> RuntimeResult<()> {
        arg_as!(&args[0], StdValue)?.set(ctx, arg_as!(&args[1], StdValue)?.val(ctx))
    }

    fn ffi_set_bool(&self, ctx: &FfiCallCtx, args: Vec<GosValue>) -> RuntimeResult<()> {
        let (to, val) = unwrap_set_args(&args)?;
        to.set_bool(ctx, val)
    }

    fn ffi_set_string(&self, ctx: &FfiCallCtx, args: Vec<GosValue>) -> RuntimeResult<()> {
        let (to, val) = unwrap_set_args(&args)?;
        to.set_string(ctx, val)
    }

    fn ffi_set_int(&self, ctx: &FfiCallCtx, args: Vec<GosValue>) -> RuntimeResult<()> {
        let (to, val) = unwrap_set_args(&args)?;
        to.set_int(ctx, val)
    }

    fn ffi_set_uint(&self, ctx: &FfiCallCtx, args: Vec<GosValue>) -> RuntimeResult<()> {
        let (to, val) = unwrap_set_args(&args)?;
        to.set_uint(ctx, val)
    }

    fn ffi_set_float(&self, ctx: &FfiCallCtx, args: Vec<GosValue>) -> RuntimeResult<()> {
        let (to, val) = unwrap_set_args(&args)?;
        to.set_float(ctx, val)
    }

    fn ffi_set_complex(&self, ctx: &FfiCallCtx, args: Vec<GosValue>) -> RuntimeResult<()> {
        let (to, val) = unwrap_set_args(&args)?;
        to.set_complex(ctx, val)
    }

    fn ffi_set_bytes(&self, ctx: &FfiCallCtx, args: Vec<GosValue>) -> RuntimeResult<()> {
        let (to, val) = unwrap_set_args(&args)?;
        to.set_bytes(ctx, val)
    }

    fn ffi_set_pointer(&self, ctx: &FfiCallCtx, args: Vec<GosValue>) -> RuntimeResult<()> {
        let (to, val) = unwrap_set_args(&args)?;
        to.set_pointer(ctx, val)
    }

    fn ffi_swap(&self, args: Vec<GosValue>) -> RuntimeResult<()> {
        let mut iter = args.into_iter();
        let arg0 = iter.next();
        let arg1 = iter.next();
        let arg2 = iter.next();
        let iface = arg0.as_ref().unwrap().as_interface().borrow();
        let mut vec = iface
            .underlying_value()
            .unwrap()
            .as_slice()
            .0
            .borrow_all_data_mut();
        let a = arg1.as_ref().unwrap().as_int();
        let b = arg2.as_ref().unwrap().as_int();
        vec.swap(*a as usize, *b as usize);
        Ok(())
    }
}

#[derive(Clone, Debug)]
enum StdValue {
    Value(GosValue),
    Pointer(Box<PointerObj>),
}

impl UnsafePtr for StdValue {
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
        let iface = v.as_interface();
        let v = match &iface.borrow() as &InterfaceObj {
            InterfaceObj::Gos(v, _) => v.clone(),
            // todo: should we return something else?
            InterfaceObj::Ffi(_) => GosValue::new_nil(),
        };
        wrap_std_val(v)
    }

    fn val(&self, ctx: &FfiCallCtx) -> GosValue {
        match self {
            Self::Value(v) => v.clone(),
            Self::Pointer(p) => p.deref(&ctx.stack, &ctx.vm_objs.packages),
        }
    }

    fn settable_meta(&self, ctx: &FfiCallCtx) -> RuntimeResult<Meta> {
        Err("todo".to_owned())
        // match self {
        //     Self::Pointer(p) => Ok(p.point_to_meta(ctx.vm_objs, ctx.stack)),
        //     Self::Value(_) => Err("reflect: value not settable".to_owned()),
        // }
    }

    fn bool_val(&self, ctx: &FfiCallCtx) -> RuntimeResult<GosValue> {
        let val = self.val(ctx);
        match val {
            GosValue::Bool(_) => Ok(val),
            _ => err_wrong_type!(),
        }
    }

    fn int_val(&self, ctx: &FfiCallCtx) -> RuntimeResult<GosValue> {
        let val = self.val(ctx);
        match val {
            GosValue::Int(i) => Ok(*i.as_int() as i64),
            GosValue::Int8(i) => Ok(*i.as_int8() as i64),
            GosValue::Int16(i) => Ok(*i.as_int16() as i64),
            GosValue::Int32(i) => Ok(*i.as_int32() as i64),
            GosValue::Int64(i) => Ok(*i.as_int64()),
            _ => err_wrong_type!(),
        }
        .map(|x| GosValue::new_int64(x))
    }

    fn uint_val(&self, ctx: &FfiCallCtx) -> RuntimeResult<GosValue> {
        let val = self.val(ctx);
        match val {
            GosValue::Uint(i) => Ok(*i.as_uint() as u64),
            GosValue::Uint8(i) => Ok(*i.as_uint8() as u64),
            GosValue::Uint16(i) => Ok(*i.as_uint16() as u64),
            GosValue::Uint32(i) => Ok(*i.as_uint32() as u64),
            GosValue::Uint64(i) => Ok(*i.as_uint64()),
            _ => err_wrong_type!(),
        }
        .map(|x| GosValue::new_uint64(x))
    }

    fn float_val(&self, ctx: &FfiCallCtx) -> RuntimeResult<GosValue> {
        let val = self.val(ctx);
        match val {
            GosValue::Float32(f) => Ok((Into::<f32>::into(*f.as_float32()) as f64).into()),
            GosValue::Float64(f) => Ok(*f.as_float64()),
            _ => err_wrong_type!(),
        }
        .map(|x| GosValue::new_float64(x))
    }

    fn bytes_val(&self, ctx: &FfiCallCtx) -> RuntimeResult<GosValue> {
        err_wrong_type!()
        // todo
        // let val = self.val(ctx);
        // let mobjs = &ctx.vm_objs.metas;
        // match val {
        //     GosValue::Slice(s) => {
        //         let (m, _) = mobjs[s.1.key].as_slice_or_array();
        //         match m.value_type(mobjs) {
        //             ValueType::Uint8 => Ok(val),
        //             _ => err_wrong_type!(),
        //         }
        //     }
        //     _ => err_wrong_type!(),
        // }
    }

    fn elem(&self, ctx: &FfiCallCtx) -> RuntimeResult<GosValue> {
        let val = self.val(ctx);
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
            Some(s) => Ok(GosValue::new_int(s.0.borrow().fields.len() as isize)),
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
        match container {
            // todo: fix later
            // GosValue::Array(arr) => match arr.0.len() > iusize {
            //     true => Ok(wrap_ptr_std_val(Box::new(PointerObj::new_array_member(
            //         container, i, ctx.gcv,
            //     )))),
            //     false => err_index_oor!(),
            // },
            GosValue::Slice(s) => match s.0.len() > iusize {
                true => Ok(wrap_ptr_std_val(Box::new(PointerObj::SliceMember(
                    s.clone(),
                    i,
                )))),
                false => err_index_oor!(),
            },
            GosValue::Str(s) => match s.len() > iusize {
                true => Ok(wrap_std_val(GosValue::new_uint8(
                    *s.get_byte(iusize).unwrap(),
                ))),
                false => err_index_oor!(),
            },
            _ => err_wrong_type!(),
        }
    }

    fn len(&self, ctx: &FfiCallCtx) -> RuntimeResult<GosValue> {
        match self.val(ctx) {
            GosValue::Array(arr) => Ok(arr.0.len()),
            GosValue::Slice(s) => Ok(s.0.len()),
            GosValue::Str(s) => Ok(s.len()),
            GosValue::Channel(c) => Ok(c.len()),
            GosValue::Map(m) => Ok(m.0.len()),
            _ => err_wrong_type!(),
        }
        .map(|x| GosValue::new_int(x as isize))
    }

    fn can_addr(&self) -> bool {
        match self {
            Self::Value(_) => false,
            Self::Pointer(_) => true,
        }
    }

    fn can_set(&self, ctx: &FfiCallCtx) -> bool {
        false // todo
              // match self {
              //     Self::Value(_) => false,
              //     Self::Pointer(p) => match p as &PointerObj {
              //         PointerObj::SliceMember(_, _) => true,
              //         PointerObj::StructField(s, i) => {
              //             StructObj::is_exported(*i as usize, &s.1, &ctx.vm_objs.metas)
              //         }
              //         PointerObj::UpVal(uv) => !uv.is_open(),
              //         _ => false,
              //     },
              // }
    }

    fn set(&self, ctx: &FfiCallCtx, val: GosValue) -> RuntimeResult<()> {
        let err = Err("reflect: value is not settable".to_owned());
        err
        //todo
        // match self {
        //     Self::Value(_) => err,
        //     Self::Pointer(p) => match p as &PointerObj {
        //         PointerObj::SliceMember(s, i) => {
        //             let vborrow = s.0.borrow();
        //             *vborrow[s.0.begin() + *i as usize].borrow_mut() = val;
        //             Ok(())
        //         }
        //         PointerObj::StructField(s, i) => {
        //             if StructObj::is_exported(*i as usize, &s.1, &ctx.vm_objs.metas) {
        //                 s.0.borrow_mut().fields[*i as usize] = val;
        //                 Ok(())
        //             } else {
        //                 err
        //             }
        //         }
        //         PointerObj::UpVal(uv) => match &mut uv.inner.borrow_mut() as &mut UpValueState {
        //             UpValueState::Open(_) => err,
        //             UpValueState::Closed(c) => {
        //                 *c = val;
        //                 Ok(())
        //             }
        //         },
        //         _ => err,
        //     },
    }

    fn set_bool(&self, ctx: &FfiCallCtx, val: GosValue) -> RuntimeResult<()> {
        match self.settable_meta(ctx)?.value_type(&ctx.vm_objs.metas) {
            ValueType::Bool => self.set(ctx, val),
            _ => err_set_val_type!(),
        }
    }

    fn set_string(&self, ctx: &FfiCallCtx, val: GosValue) -> RuntimeResult<()> {
        match self.settable_meta(ctx)?.value_type(&ctx.vm_objs.metas) {
            ValueType::Str => self.set(ctx, val),
            _ => err_set_val_type!(),
        }
    }

    fn set_int(&self, ctx: &FfiCallCtx, val: GosValue) -> RuntimeResult<()> {
        let ival = *val.as_int64();
        let val = match self.settable_meta(ctx)?.value_type(&ctx.vm_objs.metas) {
            ValueType::Int => Ok(GosValue::new_int(ival as isize)),
            ValueType::Int8 => Ok(GosValue::new_int8(ival as i8)),
            ValueType::Int16 => Ok(GosValue::new_int16(ival as i16)),
            ValueType::Int32 => Ok(GosValue::new_int32(ival as i32)),
            ValueType::Int64 => Ok(GosValue::new_int64(ival as i64)),
            _ => err_set_val_type!(),
        }?;
        self.set(ctx, val)
    }

    fn set_uint(&self, ctx: &FfiCallCtx, val: GosValue) -> RuntimeResult<()> {
        let ival = *val.as_uint64();
        let val = match self.settable_meta(ctx)?.value_type(&ctx.vm_objs.metas) {
            ValueType::Uint => Ok(GosValue::new_uint(ival as usize)),
            ValueType::Uint8 => Ok(GosValue::new_uint8(ival as u8)),
            ValueType::Uint16 => Ok(GosValue::new_uint16(ival as u16)),
            ValueType::Uint32 => Ok(GosValue::new_uint32(ival as u32)),
            ValueType::Uint64 => Ok(GosValue::new_uint64(ival as u64)),
            _ => err_set_val_type!(),
        }?;
        self.set(ctx, val)
    }

    fn set_float(&self, ctx: &FfiCallCtx, val: GosValue) -> RuntimeResult<()> {
        let fval = *val.as_float64();
        let val = match self.settable_meta(ctx)?.value_type(&ctx.vm_objs.metas) {
            ValueType::Float32 => Ok(GosValue::new_float32((fval as f32).into())),
            ValueType::Float64 => Ok(GosValue::new_float64(fval.into())),
            _ => err_set_val_type!(),
        }?;
        self.set(ctx, val)
    }

    fn set_complex(&self, ctx: &FfiCallCtx, val: GosValue) -> RuntimeResult<()> {
        let c = val.as_complex128();
        let val = match self.settable_meta(ctx)?.value_type(&ctx.vm_objs.metas) {
            ValueType::Complex64 => Ok(GosValue::new_complex64(
                (Into::<f64>::into(c.0) as f32).into(),
                (Into::<f64>::into(c.1) as f32).into(),
            )),
            ValueType::Complex128 => Ok(GosValue::Complex128(c.clone())),
            _ => err_set_val_type!(),
        }?;
        self.set(ctx, val)
    }

    fn set_bytes(&self, ctx: &FfiCallCtx, val: GosValue) -> RuntimeResult<()> {
        let metas = &ctx.vm_objs.metas;
        let meta = self.settable_meta(ctx)?;
        if meta.value_type(metas) != ValueType::Slice {
            err_wrong_type!()
        } else {
            let elem_meta = metas[meta.key].as_slice();
            if elem_meta.value_type(metas) != ValueType::Uint8 {
                err_wrong_type!()
            } else {
                self.set(ctx, val)
            }
        }
    }

    fn set_pointer(&self, ctx: &FfiCallCtx, val: GosValue) -> RuntimeResult<()> {
        if self.settable_meta(ctx)? != ctx.vm_objs.s_meta.unsafe_ptr {
            err_wrong_type!()
        } else {
            self.set(ctx, val)
        }
    }
}

#[derive(Clone, Debug)]
struct StdType {
    meta: Meta,
    mobjs: *const MetadataObjs,
}

impl UnsafePtr for StdType {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn eq(&self, other: &dyn UnsafePtr) -> bool {
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
    fn new(m: Meta, objs: &MetadataObjs) -> StdType {
        StdType {
            meta: m,
            mobjs: objs,
        }
    }

    fn type_of(val: &GosValue, ctx: &FfiCallCtx) -> (GosValue, GosValue) {
        let m = ctx.vm_objs.s_meta.none; //val.meta(ctx.vm_objs, ctx.stack);
        let typ = StdType::new(m, &ctx.vm_objs.metas);
        let kind = match m
            .underlying(&ctx.vm_objs.metas)
            .value_type(&ctx.vm_objs.metas)
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
            ValueType::Pointer => GosKind::Ptr,
            ValueType::UnsafePtr => GosKind::UnsafePtr,
            ValueType::Slice => GosKind::Slice,
            ValueType::Str => GosKind::String,
            ValueType::Struct => GosKind::Struct,
            _ => GosKind::Invalid,
        };
        (
            GosValue::new_unsafe_ptr(typ),
            GosValue::new_uint(kind as usize),
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

impl UnsafePtr for StdMapIter {
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
        GosValue::new_unsafe_ptr(smi)
    }

    fn next(&self) -> GosValue {
        let mut inner = self.inner.borrow_mut();
        inner.item = inner
            .iter
            .next()
            .map(|x| (x.0.clone(), x.1.clone().into_inner()));
        GosValue::new_bool(inner.item.is_some())
    }

    fn key(&self) -> RuntimeResult<GosValue> {
        match &self.inner.borrow().item {
            Some(kv) => Ok(kv.0.clone()),
            None => Err("reflect.MapIter: Next not called or iter exhausted".to_owned()),
        }
        .map(|x| wrap_std_val(x))
    }

    fn value(&self) -> RuntimeResult<GosValue> {
        match &self.inner.borrow().item {
            Some(kv) => Ok(kv.1.clone()),
            None => Err("reflect.MapIter: Next not called or iter exhausted".to_owned()),
        }
        .map(|x| wrap_std_val(x))
    }
}
