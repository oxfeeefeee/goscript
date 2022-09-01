// Copyright 2022 The Goscript Authors. All rights reserved.
// Use of this source code is governed by a BSD-style
// license that can be found in the LICENSE file.

mod engine;
#[cfg(feature = "run_fs")]
pub mod run_fs;
#[cfg(feature = "run_zip")]
pub mod run_zip;
mod std;

#[macro_use]
mod ffi;

#[macro_use]
extern crate lazy_static;

pub use engine::*;
pub use goscript_parser::errors::ErrorList;
