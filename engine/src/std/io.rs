// Copyright 2022 The Goscript Authors. All rights reserved.
// Use of this source code is governed by a BSD-style
// license that can be found in the LICENSE file.

extern crate self as goscript_engine;
use crate::ffi::*;
use goscript_vm::value::GosValue;
use std::cell::RefCell;
use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;

#[derive(Ffi)]
pub struct IoFfi {}

#[ffi_impl]
impl IoFfi {
    fn new(_v: Vec<GosValue>) -> IoFfi {
        unimplemented!()
    }
}
