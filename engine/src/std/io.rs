// Copyright 2022 The Goscript Authors. All rights reserved.
// Use of this source code is governed by a BSD-style
// license that can be found in the LICENSE file.

extern crate self as go_engine;
use crate::ffi::*;
use go_vm::types::GosValue;

#[derive(Ffi)]
pub struct IoFfi;

#[ffi_impl]
impl IoFfi {}
