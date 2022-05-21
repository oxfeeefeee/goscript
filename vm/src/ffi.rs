// Copyright 2022 The Goscript Authors. All rights reserved.
// Use of this source code is governed by a BSD-style
// license that can be found in the LICENSE file.

use super::gc::GcoVec;
use super::objects::VMObjects;
use super::stack2::Stack;
use super::value::{GosValue, RuntimeResult};
use std::any::Any;
use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;

/// For user to store statics used by FFI
pub trait FfiStatics {
    fn as_any(&self) -> &dyn Any;
}

pub struct FfiCallCtx<'a> {
    pub func_name: &'a str,
    pub vm_objs: &'a VMObjects,
    pub stack: &'a mut Stack,
    pub gcv: &'a GcoVec,
    pub statics: &'a dyn FfiStatics,
}

/// A FFI Object implemented in Rust for Goscript to call
pub trait Ffi {
    fn call(
        &self,
        ctx: &mut FfiCallCtx,
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
    statics: Box<dyn FfiStatics>,
}

impl FfiFactory {
    pub fn new(statics: Box<dyn FfiStatics>) -> FfiFactory {
        FfiFactory {
            registry: HashMap::new(),
            statics: statics,
        }
    }

    pub fn register(&mut self, name: &'static str, proto: Rc<dyn Ffi>) {
        assert!(self.registry.insert(name, proto).is_none());
    }

    pub fn create_by_name(&self, name: &str) -> RuntimeResult<Rc<dyn Ffi>> {
        match self.registry.get(name) {
            Some(proto) => Ok(proto.clone()),
            None => Err(format!("FFI named {} not found", name)),
        }
    }

    /// Get a reference to the ffi factory's statics.
    pub fn statics(&self) -> &dyn FfiStatics {
        self.statics.as_ref()
    }
}

impl std::fmt::Debug for FfiFactory {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "FfiFactory")
    }
}
