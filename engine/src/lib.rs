// Copyright 2022 The Goscript Authors. All rights reserved.
// Use of this source code is governed by a BSD-style
// license that can be found in the LICENSE file.

mod engine;
#[cfg(any(feature = "read_fs", feature = "go_std"))]
pub mod run_fs;
#[cfg(feature = "read_map")]
pub mod run_map;
#[cfg(feature = "read_zip")]
pub mod run_zip;

#[cfg(feature = "go_std")]
mod std;

#[macro_use]
pub mod ffi;

#[cfg(feature = "go_std")]
#[macro_use]
extern crate lazy_static;

pub use engine::*;
pub use goscript_parser::ErrorList;
