// Copyright 2022 The Goscript Authors. All rights reserved.
// Use of this source code is governed by a BSD-style
// license that can be found in the LICENSE file.

use crate::gc::GcContainer;
use crate::objects::VMObjects;
use crate::stack::Stack;
use crate::value::*;
use crate::value::{GosValue, RuntimeResult};
use futures_lite::future::Future;
use std::collections::HashMap;
use std::pin::Pin;
use std::rc::Rc;

pub struct FfiCtx<'a> {
    pub func_name: &'a str,
    pub vm_objs: &'a VMObjects,
    pub stack: &'a mut Stack,
    pub gcc: &'a GcContainer,
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

    pub fn new_unsafe_ptr<T: 'static + UnsafePtr>(p: T) -> GosValue {
        GosValue::new_unsafe_ptr(p)
    }

    pub fn zero_val(&self, m: &Meta) -> GosValue {
        m.zero(&self.vm_objs.metas, self.gcc)
    }
}

/// A FFI Object implemented in Rust for Goscript to call
pub trait Ffi {
    fn call(
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
    registry: HashMap<&'static str, Rc<dyn Ffi>>,
}

impl FfiFactory {
    pub fn new() -> FfiFactory {
        FfiFactory {
            registry: HashMap::new(),
        }
    }

    pub fn register(&mut self, name: &'static str, proto: Rc<dyn Ffi>) {
        assert!(self.registry.insert(name, proto).is_none());
    }

    pub(crate) fn create(&self, name: &str) -> RuntimeResult<Rc<dyn Ffi>> {
        match self.registry.get(name) {
            Some(proto) => Ok(proto.clone()),
            None => Err(format!("FFI named {} not found", name)),
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
}

impl CodeGenVMCtx {
    pub fn new(vm_objs: VMObjects) -> CodeGenVMCtx {
        CodeGenVMCtx {
            vm_objs,
            dummy_func_name: "dummy_name",
            dummy_stack: Stack::new(),
            dummy_gcc: GcContainer::new(),
        }
    }

    pub fn ffi_ctx(&mut self) -> FfiCtx {
        FfiCtx {
            func_name: self.dummy_func_name,
            vm_objs: &self.vm_objs,
            stack: &mut self.dummy_stack,
            gcc: &&self.dummy_gcc,
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

    pub fn s_meta(&self) -> &StaticMeta {
        &self.vm_objs.s_meta
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
        let val = FunctionVal::new(package, meta, &self.vm_objs.metas, &self.dummy_gcc, flag);
        GosValue::new_function(self.vm_objs.functions.insert(val))
    }

    pub fn new_struct_meta(&mut self, fields: Fields) -> Meta {
        Meta::new_struct(fields, &mut self.vm_objs, &self.dummy_gcc)
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
