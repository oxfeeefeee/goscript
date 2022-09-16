// Copyright 2022 The Goscript Authors. All rights reserved.
// Use of this source code is governed by a BSD-style
// license that can be found in the LICENSE file.

extern crate self as goscript_engine;
use crate::ffi::*;
use goscript_vm::value::*;

#[derive(Ffi)]
pub struct BitsFfi;

#[ffi_impl]
impl BitsFfi {
    fn ffi_f32_to_bits(f: f32) -> u32 {
        u32::from_be_bytes(f.to_be_bytes())
    }

    fn ffi_f32_from_bits(u: u32) -> f32 {
        f32::from_be_bytes(u.to_be_bytes())
    }

    fn ffi_f64_to_bits(f: f64) -> u64 {
        u64::from_be_bytes(f.to_be_bytes())
    }

    fn ffi_f64_from_bits(u: u64) -> f64 {
        f64::from_be_bytes(u.to_be_bytes())
    }
}
