// Copyright 2022 The Goscript Authors. All rights reserved.
// Use of this source code is governed by a BSD-style
// license that can be found in the LICENSE file.

use super::gc::GcContainer;
use super::objects::VMObjects;
use super::stack::Stack;
use super::value::{GosValue, RuntimeResult};
use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;

pub struct FfiCtx<'a> {
    pub func_name: &'a str,
    pub vm_objs: &'a VMObjects,
    pub stack: &'a mut Stack,
    pub gcc: &'a GcContainer,
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
