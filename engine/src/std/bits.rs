// Copyright 2022 The Goscript Authors. All rights reserved.
// Use of this source code is governed by a BSD-style
// license that can be found in the LICENSE file.

extern crate self as goscript_engine;
use crate::ffi::*;
use goscript_vm::value::GosValue;
use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;

#[derive(Ffi)]
pub struct BitsFfi {}

#[ffi_impl]
impl BitsFfi {
    fn ffi_f32_to_bits(&self, args: Vec<GosValue>) -> GosValue {
        let result = u32::from_be_bytes(args[0].as_float32().to_be_bytes());
        result.into()
    }

    fn ffi_f32_from_bits(&self, args: Vec<GosValue>) -> GosValue {
        let result = f32::from_be_bytes(args[0].as_uint32().to_be_bytes());
        result.into()
    }

    fn ffi_f64_to_bits(&self, args: Vec<GosValue>) -> GosValue {
        let result = u64::from_be_bytes(args[0].as_float64().to_be_bytes());
        result.into()
    }

    fn ffi_f64_from_bits(&self, args: Vec<GosValue>) -> GosValue {
        let result = f64::from_be_bytes(args[0].as_uint64().to_be_bytes());
        result.into()
    }
}
