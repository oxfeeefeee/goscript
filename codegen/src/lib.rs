// Copyright 2022 The Goscript Authors. All rights reserved.
// Use of this source code is governed by a BSD-style
// license that can be found in the LICENSE file.

mod branch;
mod call;
mod emit;
mod interface;
mod package;
mod types;

pub mod codegen;
pub mod entry;
pub use entry::parse_check_gen;
pub use goscript_types::Config;
