// Copyright 2022 The Goscript Authors. All rights reserved.
// Use of this source code is governed by a BSD-style
// license that can be found in the LICENSE file.

extern crate self as goscript_engine;
use crate::ffi::*;
use goscript_vm::instruction::ValueType;
use goscript_vm::metadata::{Meta, MetadataType};
use goscript_vm::objects::*;
use goscript_vm::value::{GosValue, InterfaceObj, PointerObj, UnsafePtr};
use std::any::Any;
use std::cell::RefCell;
use std::future::Future;
use std::mem;
use std::pin::Pin;
use std::rc::Rc;

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
fn wrap_std_val(v: GosValue, m: Option<Meta>) -> GosValue {
    GosValue::new_unsafe_ptr(StdValue::new(v, m))
}

#[inline]
fn wrap_ptr_std_val(p: Box<PointerObj>, m: Option<Meta>) -> GosValue {
    GosValue::new_unsafe_ptr(StdValue::Pointer(p, m, None))
}

#[inline]
fn unwrap_set_args(args: &Vec<GosValue>) -> RuntimeResult<(&StdValue, GosValue)> {
    Ok((
        args[0]
            .as_non_nil_unsafe_ptr()?
            .downcast_ref::<StdValue>()?,
        args[1].clone(),
    ))
}

#[inline]
fn val_to_std_val(val: &GosValue) -> RuntimeResult<&StdValue> {
    val.as_non_nil_unsafe_ptr()?.downcast_ref::<StdValue>()
}

#[inline]
fn val_to_map_iter(val: &GosValue) -> RuntimeResult<&StdMapIter> {
    val.as_non_nil_unsafe_ptr()?.downcast_ref::<StdMapIter>()
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
pub struct ReflectFfi {}

#[ffi_impl]
impl ReflectFfi {
    fn ffi_value_of(&self, args: Vec<GosValue>) -> RuntimeResult<GosValue> {
        StdValue::value_from_iface(&args[0])
    }

    fn ffi_type_of(&self, ctx: &FfiCallCtx, args: Vec<GosValue>) -> RuntimeResult<Vec<GosValue>> {
        let v = val_to_std_val(&args[0])?;
        let (t, k) = StdType::type_of(v, ctx);
        Ok(vec![t, k])
    }

    fn ffi_bool_val(&self, ctx: &FfiCallCtx, args: Vec<GosValue>) -> RuntimeResult<GosValue> {
        val_to_std_val(&args[0])?.bool_val(ctx)
    }

    fn ffi_int_val(&self, ctx: &FfiCallCtx, args: Vec<GosValue>) -> RuntimeResult<GosValue> {
        val_to_std_val(&args[0])?.int_val(ctx)
    }

    fn ffi_uint_val(&self, ctx: &FfiCallCtx, args: Vec<GosValue>) -> RuntimeResult<GosValue> {
        val_to_std_val(&args[0])?.uint_val(ctx)
    }

    fn ffi_float_val(&self, ctx: &FfiCallCtx, args: Vec<GosValue>) -> RuntimeResult<GosValue> {
        val_to_std_val(&args[0])?.float_val(ctx)
    }

    fn ffi_bytes_val(&self, ctx: &FfiCallCtx, args: Vec<GosValue>) -> RuntimeResult<GosValue> {
        val_to_std_val(&args[0])?.bytes_val(ctx)
    }

    fn ffi_elem(&self, ctx: &FfiCallCtx, args: Vec<GosValue>) -> RuntimeResult<GosValue> {
        val_to_std_val(&args[0])?.elem(ctx)
    }

    fn ffi_num_field(&self, ctx: &FfiCallCtx, args: Vec<GosValue>) -> RuntimeResult<GosValue> {
        val_to_std_val(&args[0])?.num_field(ctx)
    }

    fn ffi_field(&self, ctx: &FfiCallCtx, args: Vec<GosValue>) -> RuntimeResult<GosValue> {
        val_to_std_val(&args[0])?.field(ctx, &args[1])
    }

    fn ffi_index(&self, ctx: &FfiCallCtx, args: Vec<GosValue>) -> RuntimeResult<GosValue> {
        val_to_std_val(&args[0])?.index(ctx, &args[1])
    }

    fn ffi_is_nil(&self, ctx: &FfiCallCtx, args: Vec<GosValue>) -> RuntimeResult<GosValue> {
        Ok(GosValue::new_bool(
            val_to_std_val(&args[0])?.val(ctx)?.is_nil(),
        ))
    }

    fn ffi_len(&self, ctx: &FfiCallCtx, args: Vec<GosValue>) -> RuntimeResult<GosValue> {
        val_to_std_val(&args[0])?.len(ctx)
    }

    fn ffi_map_range_init(&self, ctx: &FfiCallCtx, args: Vec<GosValue>) -> RuntimeResult<GosValue> {
        StdMapIter::map_range(ctx, val_to_std_val(&args[0])?)
    }

    fn ffi_map_range_next(&self, args: Vec<GosValue>) -> RuntimeResult<GosValue> {
        Ok(val_to_map_iter(&args[0])?.next())
    }

    fn ffi_map_range_key(&self, args: Vec<GosValue>) -> RuntimeResult<GosValue> {
        val_to_map_iter(&args[0])?.key()
    }

    fn ffi_map_range_value(&self, args: Vec<GosValue>) -> RuntimeResult<GosValue> {
        val_to_map_iter(&args[0])?.value()
    }

    fn ffi_can_addr(&self, args: Vec<GosValue>) -> RuntimeResult<GosValue> {
        Ok(GosValue::new_bool(val_to_std_val(&args[0])?.can_addr()))
    }

    fn ffi_can_set(&self, args: Vec<GosValue>) -> RuntimeResult<GosValue> {
        Ok(GosValue::new_bool(val_to_std_val(&args[0])?.can_set()))
    }

    fn ffi_set(&self, ctx: &mut FfiCallCtx, args: Vec<GosValue>) -> RuntimeResult<()> {
        val_to_std_val(&args[0])?.set(ctx, val_to_std_val(&args[1])?.val(ctx)?)
    }

    fn ffi_set_bool(&self, ctx: &mut FfiCallCtx, args: Vec<GosValue>) -> RuntimeResult<()> {
        let (to, val) = unwrap_set_args(&args)?;
        to.set_bool(ctx, val)
    }

    fn ffi_set_string(&self, ctx: &mut FfiCallCtx, args: Vec<GosValue>) -> RuntimeResult<()> {
        let (to, val) = unwrap_set_args(&args)?;
        to.set_string(ctx, val)
    }

    fn ffi_set_int(&self, ctx: &mut FfiCallCtx, args: Vec<GosValue>) -> RuntimeResult<()> {
        let (to, val) = unwrap_set_args(&args)?;
        to.set_int(ctx, val)
    }

    fn ffi_set_uint(&self, ctx: &mut FfiCallCtx, args: Vec<GosValue>) -> RuntimeResult<()> {
        let (to, val) = unwrap_set_args(&args)?;
        to.set_uint(ctx, val)
    }

    fn ffi_set_float(&self, ctx: &mut FfiCallCtx, args: Vec<GosValue>) -> RuntimeResult<()> {
        let (to, val) = unwrap_set_args(&args)?;
        to.set_float(ctx, val)
    }

    fn ffi_set_complex(&self, ctx: &mut FfiCallCtx, args: Vec<GosValue>) -> RuntimeResult<()> {
        let (to, val) = unwrap_set_args(&args)?;
        to.set_complex(ctx, val)
    }

    fn ffi_set_bytes(&self, ctx: &mut FfiCallCtx, args: Vec<GosValue>) -> RuntimeResult<()> {
        let (to, val) = unwrap_set_args(&args)?;
        to.set_bytes(ctx, val)
    }

    fn ffi_set_pointer(&self, ctx: &mut FfiCallCtx, args: Vec<GosValue>) -> RuntimeResult<()> {
        let (to, val) = unwrap_set_args(&args)?;
        to.set_pointer(ctx, val)
    }

    fn ffi_swap(&self, args: Vec<GosValue>) -> RuntimeResult<()> {
        let mut iter = args.into_iter();
        let arg0 = iter.next();
        let arg1 = iter.next();
        let arg2 = iter.next();
        let a = arg1.as_ref().unwrap().as_int();
        let b = arg2.as_ref().unwrap().as_int();
        let iface = arg0.as_ref().unwrap().as_interface().unwrap();
        match iface.underlying_value() {
            Some(obj) => {
                if obj.typ() == ValueType::Slice {
                    obj.dispatcher_a_s()
                        .slice_swap(obj, *a as usize, *b as usize)
                } else {
                    Err("reflect swap: not a slice".to_owned())
                }
            }
            None => Err("reflect swap: nil value".to_owned()),
        }
    }
}

#[derive(Clone, Debug, UnsafePtr)]
enum StdValue {
    Value(GosValue, Option<Meta>),
    Pointer(Box<PointerObj>, Option<Meta>, Option<bool>),
}

impl StdValue {
    fn new(v: GosValue, meta: Option<Meta>) -> StdValue {
        StdValue::Value(v, meta)
    }

    fn value_from_iface(v: &GosValue) -> RuntimeResult<GosValue> {
        let iface = v.as_interface().unwrap();
        match &iface as &InterfaceObj {
            InterfaceObj::Gos(v, m) => Ok(wrap_std_val(v.clone(), m.as_ref().map(|x| x.0))),
            // todo: should we return something else?
            InterfaceObj::Ffi(_) => Err("reflect: ffi objects are not supported".to_owned()),
        }
    }

    fn val(&self, ctx: &FfiCallCtx) -> RuntimeResult<GosValue> {
        match self {
            Self::Value(v, _) => Ok(v.clone()),
            Self::Pointer(p, _, _) => p.deref(&ctx.stack, &ctx.vm_objs.packages),
        }
    }

    fn meta(&self) -> &Option<Meta> {
        match self {
            Self::Value(_, m) | Self::Pointer(_, m, _) => m,
        }
    }

    fn settable_meta(&self) -> RuntimeResult<&Meta> {
        match self {
            Self::Pointer(_, m, _) => m.as_ref().ok_or("reflect: type info missing".to_owned()),
            Self::Value(_, _) => Err("reflect: value not settable".to_owned()),
        }
    }

    fn bool_val(&self, ctx: &FfiCallCtx) -> RuntimeResult<GosValue> {
        let val = self.val(ctx)?;
        match val.typ() {
            ValueType::Bool => Ok(val),
            _ => err_wrong_type!(),
        }
    }

    fn int_val(&self, ctx: &FfiCallCtx) -> RuntimeResult<GosValue> {
        let val = self.val(ctx)?;
        match val.typ() {
            ValueType::Int => Ok(*val.as_int() as i64),
            ValueType::Int8 => Ok(*val.as_int8() as i64),
            ValueType::Int16 => Ok(*val.as_int16() as i64),
            ValueType::Int32 => Ok(*val.as_int32() as i64),
            ValueType::Int64 => Ok(*val.as_int64()),
            _ => err_wrong_type!(),
        }
        .map(|x| GosValue::new_int64(x))
    }

    fn uint_val(&self, ctx: &FfiCallCtx) -> RuntimeResult<GosValue> {
        let val = self.val(ctx)?;
        match val.typ() {
            ValueType::Uint => Ok(*val.as_uint() as u64),
            ValueType::Uint8 => Ok(*val.as_uint8() as u64),
            ValueType::Uint16 => Ok(*val.as_uint16() as u64),
            ValueType::Uint32 => Ok(*val.as_uint32() as u64),
            ValueType::Uint64 => Ok(*val.as_uint64()),
            _ => err_wrong_type!(),
        }
        .map(|x| GosValue::new_uint64(x))
    }

    fn float_val(&self, ctx: &FfiCallCtx) -> RuntimeResult<GosValue> {
        let val = self.val(ctx)?;
        match val.typ() {
            ValueType::Float32 => Ok((Into::<f32>::into(*val.as_float32()) as f64).into()),
            ValueType::Float64 => Ok(*val.as_float64()),
            _ => err_wrong_type!(),
        }
        .map(|x| GosValue::new_float64(x))
    }

    fn bytes_val(&self, ctx: &FfiCallCtx) -> RuntimeResult<GosValue> {
        let val = self.val(ctx)?;
        if val.typ() != ValueType::Slice || val.t_elem() != ValueType::Uint8 {
            return err_wrong_type!();
        }
        Ok(val)
    }

    fn elem(&self, ctx: &FfiCallCtx) -> RuntimeResult<GosValue> {
        let val = self.val(ctx)?;
        match val.typ() {
            ValueType::Interface => StdValue::value_from_iface(&val),
            ValueType::Pointer => {
                let p = val.as_non_nil_pointer()?;
                let meta = self.meta().map(|x| x.unptr_to());
                Ok(wrap_ptr_std_val(Box::new(p.clone()), meta))
            }
            _ => err_wrong_type!(),
        }
    }

    fn num_field(&self, ctx: &FfiCallCtx) -> RuntimeResult<GosValue> {
        let val = self.val(ctx)?;
        if val.typ() != ValueType::Struct {
            err_wrong_type!()
        } else {
            Ok(GosValue::new_int(
                val.as_struct().0.borrow_fields().len() as isize
            ))
        }
    }

    fn field(&self, ctx: &FfiCallCtx, ival: &GosValue) -> RuntimeResult<GosValue> {
        let i = *ival.as_int() as usize;
        let val = self.val(ctx)?;
        match val.typ() {
            ValueType::Struct => {
                let fields = &val.as_struct().0.borrow_fields();
                if fields.len() <= i {
                    err_index_oor!()
                } else {
                    let p = Box::new(PointerObj::StructField(val.clone(), i as i32));
                    let metas = &ctx.vm_objs.metas;
                    let fields = &metas[self.meta().unwrap().underlying(metas).key]
                        .as_struct()
                        .0
                        .all();
                    Ok(GosValue::new_unsafe_ptr(StdValue::Pointer(
                        p,
                        Some(fields[i].meta),
                        Some(fields[i].exported),
                    )))
                }
            }
            _ => err_wrong_type!(),
        }
    }

    fn index(&self, ctx: &FfiCallCtx, ival: &GosValue) -> RuntimeResult<GosValue> {
        let i = *ival.as_int() as i32;
        let iusize = i as usize;
        let container = self.val(ctx)?;
        let t = container.typ();
        match t {
            ValueType::Array | ValueType::Slice => {
                let metas = &ctx.vm_objs.metas;
                let elem_meta = match &metas[self.meta().unwrap().underlying(metas).key] {
                    MetadataType::Array(m, _) | MetadataType::Slice(m) => m,
                    _ => unreachable!(),
                };
                let p = Box::new(PointerObj::new_slice_member(
                    container,
                    i,
                    t,
                    elem_meta.value_type(metas),
                )?);
                Ok(wrap_ptr_std_val(p, Some(elem_meta.clone())))
            }
            // specs: a[x] is the non-constant byte value at index x and the type of a[x] is byte
            ValueType::String => match container.as_string().len() > iusize {
                true => Ok(wrap_std_val(
                    GosValue::new_uint8(StrUtil::index_elem(container.as_string(), iusize)),
                    Some(ctx.vm_objs.s_meta.mint8),
                )),
                false => err_index_oor!(),
            },
            _ => err_wrong_type!(),
        }
    }

    fn len(&self, ctx: &FfiCallCtx) -> RuntimeResult<GosValue> {
        let val = self.val(ctx)?;
        Ok(GosValue::new_int(val.len() as isize))
    }

    fn can_addr(&self) -> bool {
        match self {
            Self::Value(_, _) => false,
            Self::Pointer(_, _, _) => true,
        }
    }

    fn can_set(&self) -> bool {
        match self {
            Self::Value(_, _) => false,
            Self::Pointer(p, _, exported) => match p as &PointerObj {
                PointerObj::SliceMember(_, _) => true,
                PointerObj::StructField(_, _) => exported.unwrap(),
                PointerObj::UpVal(_) => true,
                _ => false,
            },
        }
    }

    fn set(&self, ctx: &mut FfiCallCtx, val: GosValue) -> RuntimeResult<()> {
        if !self.can_set() {
            return Err("reflect: value is not settable".to_owned());
        }
        match self {
            Self::Pointer(p, _, _) => {
                p.set_pointee(&val, ctx.stack, &ctx.vm_objs.packages, &ctx.gcv)
            }
            _ => unreachable!(),
        }
    }

    fn set_bool(&self, ctx: &mut FfiCallCtx, val: GosValue) -> RuntimeResult<()> {
        match self.settable_meta()?.value_type(&ctx.vm_objs.metas) {
            ValueType::Bool => self.set(ctx, val),
            _ => err_set_val_type!(),
        }
    }

    fn set_string(&self, ctx: &mut FfiCallCtx, val: GosValue) -> RuntimeResult<()> {
        match self.settable_meta()?.value_type(&ctx.vm_objs.metas) {
            ValueType::String => self.set(ctx, val),
            _ => err_set_val_type!(),
        }
    }

    fn set_int(&self, ctx: &mut FfiCallCtx, val: GosValue) -> RuntimeResult<()> {
        let ival = *val.as_int64();
        let val = match self.settable_meta()?.value_type(&ctx.vm_objs.metas) {
            ValueType::Int => Ok(GosValue::new_int(ival as isize)),
            ValueType::Int8 => Ok(GosValue::new_int8(ival as i8)),
            ValueType::Int16 => Ok(GosValue::new_int16(ival as i16)),
            ValueType::Int32 => Ok(GosValue::new_int32(ival as i32)),
            ValueType::Int64 => Ok(GosValue::new_int64(ival as i64)),
            _ => err_set_val_type!(),
        }?;
        self.set(ctx, val)
    }

    fn set_uint(&self, ctx: &mut FfiCallCtx, val: GosValue) -> RuntimeResult<()> {
        let ival = *val.as_uint64();
        let val = match self.settable_meta()?.value_type(&ctx.vm_objs.metas) {
            ValueType::Uint => Ok(GosValue::new_uint(ival as usize)),
            ValueType::Uint8 => Ok(GosValue::new_uint8(ival as u8)),
            ValueType::Uint16 => Ok(GosValue::new_uint16(ival as u16)),
            ValueType::Uint32 => Ok(GosValue::new_uint32(ival as u32)),
            ValueType::Uint64 => Ok(GosValue::new_uint64(ival as u64)),
            _ => err_set_val_type!(),
        }?;
        self.set(ctx, val)
    }

    fn set_float(&self, ctx: &mut FfiCallCtx, val: GosValue) -> RuntimeResult<()> {
        let fval = *val.as_float64();
        let val = match self.settable_meta()?.value_type(&ctx.vm_objs.metas) {
            ValueType::Float32 => Ok(GosValue::new_float32((fval.into_inner() as f32).into())),
            ValueType::Float64 => Ok(GosValue::new_float64(fval.into())),
            _ => err_set_val_type!(),
        }?;
        self.set(ctx, val)
    }

    fn set_complex(&self, ctx: &mut FfiCallCtx, val: GosValue) -> RuntimeResult<()> {
        let c = val.as_complex128();
        let val = match self.settable_meta()?.value_type(&ctx.vm_objs.metas) {
            ValueType::Complex64 => Ok(GosValue::new_complex64(
                (Into::<f64>::into(c.r) as f32).into(),
                (Into::<f64>::into(c.i) as f32).into(),
            )),
            ValueType::Complex128 => Ok(GosValue::new_complex128(c.r, c.i)),
            _ => err_set_val_type!(),
        }?;
        self.set(ctx, val)
    }

    fn set_bytes(&self, ctx: &mut FfiCallCtx, val: GosValue) -> RuntimeResult<()> {
        let metas = &ctx.vm_objs.metas;
        let meta = self.settable_meta()?;
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

    fn set_pointer(&self, ctx: &mut FfiCallCtx, val: GosValue) -> RuntimeResult<()> {
        if self.settable_meta()? != &ctx.vm_objs.s_meta.unsafe_ptr {
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

    fn type_of(val: &StdValue, ctx: &FfiCallCtx) -> (GosValue, GosValue) {
        let m = val.meta().unwrap().clone();
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
            ValueType::String => GosKind::String,
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
    key_meta: Meta,
    //val_meta: Meta,
}

impl UnsafePtr for StdMapIter {
    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl StdMapIter {
    fn map_range(ctx: &FfiCallCtx, v: &StdValue) -> RuntimeResult<GosValue> {
        let val = v.val(ctx)?;
        let mref = val.as_map().unwrap().0.borrow_data();
        let iter: GosHashMapIter<'static> = unsafe { mem::transmute(mref.iter()) };
        let metas = &ctx.vm_objs.metas;
        let map_meta = metas[v.meta().unwrap().underlying(metas).key].as_map();
        let (k, _) = (map_meta.0.clone(), map_meta.1.clone());
        let smi = StdMapIter {
            inner: RefCell::new(StdMapIterInner {
                iter: iter,
                item: None,
            }),
            key_meta: k,
            //val_meta: v,
        };
        Ok(GosValue::new_unsafe_ptr(smi))
    }

    fn next(&self) -> GosValue {
        let mut inner = self.inner.borrow_mut();
        inner.item = inner.iter.next().map(|x| (x.0.clone(), x.1.clone()));
        GosValue::new_bool(inner.item.is_some())
    }

    fn key(&self) -> RuntimeResult<GosValue> {
        match &self.inner.borrow().item {
            Some(kv) => Ok(kv.0.clone()),
            None => Err("reflect.MapIter: Next not called or iter exhausted".to_owned()),
        }
        .map(|x| wrap_std_val(x, Some(self.key_meta)))
    }

    fn value(&self) -> RuntimeResult<GosValue> {
        match &self.inner.borrow().item {
            Some(kv) => Ok(kv.1.clone()),
            None => Err("reflect.MapIter: Next not called or iter exhausted".to_owned()),
        }
        .map(|x| wrap_std_val(x, Some(self.key_meta)))
    }
}
