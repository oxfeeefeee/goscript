// Copyright 2022 The Goscript Authors. All rights reserved.
// Use of this source code is governed by a BSD-style
// license that can be found in the LICENSE file.

use crate::bytecode::*;
use crate::dispatcher::ArrCaller;
use crate::gc::GcContainer;
use crate::stack::Stack;
use crate::value::*;
use crate::value::{GosValue, RuntimeResult};
#[cfg(feature = "async")]
use futures_lite::future::Future;
use go_parser::Map;
use std::cell::Ref;
#[cfg(feature = "async")]
use std::pin::Pin;
use std::rc::Rc;

pub struct FfiCtx<'a> {
    pub func_name: &'a str,
    pub vm_objs: &'a VMObjects,
    pub user_data: Option<usize>,
    pub stack: &'a mut Stack,
    pub gcc: &'a GcContainer,
    pub(crate) array_slice_caller: &'a ArrCaller,
}

impl<'a> FfiCtx<'a> {
    #[inline]
    pub fn new_nil(t: ValueType) -> GosValue {
        GosValue::new_nil(t)
    }

    #[inline]
    pub fn new_nil_slice(t_elem: ValueType) -> GosValue {
        GosValue::new_nil_slice(t_elem)
    }

    #[inline]
    pub fn new_uint_ptr(u: usize) -> GosValue {
        GosValue::new_uint_ptr(u)
    }

    #[inline]
    pub fn new_complex64(r: f32, i: f32) -> GosValue {
        GosValue::new_complex64(r.into(), i.into())
    }

    #[inline]
    pub fn new_function(f: FunctionKey) -> GosValue {
        GosValue::new_function(f)
    }

    #[inline]
    pub fn new_package(p: PackageKey) -> GosValue {
        GosValue::new_package(p)
    }

    #[inline]
    pub fn new_metadata(m: Meta) -> GosValue {
        GosValue::new_metadata(m)
    }

    #[inline]
    pub fn new_complex128(r: f64, i: f64) -> GosValue {
        GosValue::new_complex128(r.into(), i.into())
    }

    #[inline]
    pub fn new_string(s: &str) -> GosValue {
        GosValue::with_str(s)
    }

    #[inline]
    pub fn new_unsafe_ptr(p: Rc<dyn UnsafePtr>) -> GosValue {
        GosValue::new_unsafe_ptr(p)
    }

    #[inline]
    pub fn new_struct(&self, fields: Vec<GosValue>) -> GosValue {
        GosValue::new_struct(StructObj::new(fields), self.gcc)
    }

    #[inline]
    pub fn new_array(&self, member: Vec<GosValue>, t_elem: ValueType) -> GosValue {
        GosValue::array_with_data(member, self.array_slice_caller.get(t_elem), self.gcc)
    }

    #[inline]
    pub fn new_primitive_array<T>(&self, member: Vec<T>, t_elem: ValueType) -> GosValue
    where
        T: CellData,
    {
        let buf: Vec<CellElem<T>> = unsafe { std::mem::transmute(member) };
        GosValue::new_non_gc_array(ArrayObj::with_raw_data(buf), t_elem)
    }

    #[inline]
    pub fn new_map(&self, m: Map<GosValue, GosValue>) -> GosValue {
        GosValue::map_with_data(m, self.gcc)
    }

    #[inline]
    pub fn new_pointer(pointee: GosValue) -> GosValue {
        let pobj = PointerObj::UpVal(UpValue::new_closed(pointee));
        GosValue::new_pointer(pobj)
    }

    #[inline]
    pub fn new_interface(
        underlying: GosValue,
        meta: Option<(Meta, Vec<Binding4Runtime>)>,
    ) -> GosValue {
        GosValue::new_interface(InterfaceObj::with_value(underlying, meta))
    }

    #[inline]
    pub fn deref_pointer(&self, ptr: &GosValue) -> RuntimeResult<GosValue> {
        ptr.as_non_nil_pointer()?
            .deref(&self.stack, &self.vm_objs.packages)
    }

    #[inline]
    pub fn zero_val(&self, m: &Meta) -> GosValue {
        m.zero(&self.vm_objs.metas, self.gcc)
    }

    #[inline]
    pub fn slice_as_rust_slice<T>(val: &GosValue) -> RuntimeResult<Ref<[T]>>
    where
        T: Element,
    {
        Ok(val.as_non_nil_slice::<T>()?.0.as_rust_slice())
    }

    #[inline]
    pub fn slice_as_primitive_slice<'b, C, D>(val: &'b GosValue) -> RuntimeResult<Ref<[D]>>
    where
        C: CellData + 'b,
        D: Copy,
    {
        Ok(val.as_non_nil_slice::<CellElem<C>>()?.0.as_raw_slice::<D>())
    }

    #[inline]
    pub fn array_as_rust_slice<T>(val: &GosValue) -> Ref<[T]>
    where
        T: Element,
    {
        val.as_array::<T>().0.as_rust_slice()
    }

    #[inline]
    pub fn array_as_primitive_slice<'b, C, D>(val: &'b GosValue) -> Ref<[D]>
    where
        C: CellData + 'b,
        D: Copy,
    {
        val.as_array::<CellElem<C>>().0.as_raw_slice::<D>()
    }
}

/// A FFI Object implemented in Rust for Goscript to call
pub trait Ffi {
    fn call(&self, ctx: &mut FfiCtx, params: Vec<GosValue>) -> RuntimeResult<Vec<GosValue>>;

    #[cfg(feature = "async")]
    fn async_call(
        &self,
        ctx: &mut FfiCtx,
        params: Vec<GosValue>,
    ) -> Pin<Box<dyn Future<Output = RuntimeResult<Vec<GosValue>>> + '_>>;
}

impl std::fmt::Debug for dyn Ffi {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}", "ffi")
    }
}

pub struct FfiFactory {
    registry: Map<&'static str, Rc<dyn Ffi>>,
    /// Down-casting only works for 'static types,
    /// so we just use the good old pointers
    user_data: Option<usize>,
}

impl FfiFactory {
    pub fn new() -> FfiFactory {
        FfiFactory {
            registry: Map::new(),
            user_data: None,
        }
    }

    pub fn with_user_data(ptr: usize) -> FfiFactory {
        FfiFactory {
            registry: Map::new(),
            user_data: Some(ptr),
        }
    }

    pub fn register(&mut self, name: &'static str, proto: Rc<dyn Ffi>) {
        assert!(self.registry.insert(name, proto).is_none());
    }

    pub(crate) fn user_data(&self) -> Option<usize> {
        self.user_data
    }

    pub(crate) fn create(&self, name: &str) -> RuntimeResult<Rc<dyn Ffi>> {
        match self.registry.get(name) {
            Some(proto) => Ok(proto.clone()),
            None => Err(format!("FFI named {} not found", name).into()),
        }
    }
}

impl std::fmt::Debug for FfiFactory {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "FfiFactory")
    }
}

/// Used by CodeGen, so that CodeGen can share the API provided by FFI
pub struct CodeGenVMCtx {
    vm_objs: VMObjects,
    dummy_func_name: &'static str,
    dummy_stack: Stack,
    dummy_gcc: GcContainer,
    caller: ArrCaller,
}

impl CodeGenVMCtx {
    pub fn new(vm_objs: VMObjects) -> CodeGenVMCtx {
        CodeGenVMCtx {
            vm_objs,
            dummy_func_name: "dummy_name",
            dummy_stack: Stack::new(),
            dummy_gcc: GcContainer::new(),
            caller: ArrCaller::new(),
        }
    }

    pub fn ffi_ctx(&mut self) -> FfiCtx {
        FfiCtx {
            func_name: self.dummy_func_name,
            vm_objs: &self.vm_objs,
            user_data: None,
            stack: &mut self.dummy_stack,
            gcc: &&self.dummy_gcc,
            array_slice_caller: &self.caller,
        }
    }

    pub fn objects(&self) -> &VMObjects {
        &self.vm_objs
    }

    pub fn objects_mut(&mut self) -> &mut VMObjects {
        &mut self.vm_objs
    }

    pub fn metas(&self) -> &MetadataObjs {
        &self.vm_objs.metas
    }

    pub fn metas_mut(&mut self) -> &mut MetadataObjs {
        &mut self.vm_objs.metas
    }

    pub fn functions(&self) -> &FunctionObjs {
        &self.vm_objs.functions
    }

    pub fn functions_mut(&mut self) -> &mut FunctionObjs {
        &mut self.vm_objs.functions
    }

    pub fn packages(&self) -> &PackageObjs {
        &self.vm_objs.packages
    }

    pub fn packages_mut(&mut self) -> &mut PackageObjs {
        &mut self.vm_objs.packages
    }

    pub fn prim_meta(&self) -> &PrimitiveMeta {
        &self.vm_objs.prim_meta
    }

    pub fn gc_container(&self) -> &GcContainer {
        &self.dummy_gcc
    }

    pub fn into_vmo(self) -> VMObjects {
        self.vm_objs
    }

    pub fn function_with_meta(
        &mut self,
        package: Option<PackageKey>,
        meta: Meta,
        flag: FuncFlag,
    ) -> GosValue {
        let package = package.unwrap_or_else(|| PackageKey::null());
        let val = FunctionObj::new(package, meta, &self.vm_objs.metas, &self.dummy_gcc, flag);
        GosValue::new_function(self.vm_objs.functions.insert(val))
    }

    pub fn new_struct_meta(&mut self, fields: Fields) -> Meta {
        Meta::new_struct(fields, &mut self.vm_objs)
    }

    #[inline]
    pub fn new_closure_static(
        func: FunctionKey,
        up_ptrs: Option<&Vec<ValueDesc>>,
        meta: Meta,
    ) -> GosValue {
        GosValue::new_closure_static(func, up_ptrs, meta)
    }
}
