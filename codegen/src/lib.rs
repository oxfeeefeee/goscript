// Copyright 2022 The Goscript Authors. All rights reserved.
// Use of this source code is governed by a BSD-style
// license that can be found in the LICENSE file.

//! This crate is part of the Goscript project. Please refer to <https://goscript.dev> for more information.
//!
//! # Feature
//! - `async`: Channel and goroutine support
//! - `btree_map`: Make it use BTreeMap instead of HashMap

mod branch;
mod consts;
mod context;
//mod emit;
mod package;
//mod selector;
mod codegen;
mod entry;
mod types;

pub use entry::parse_check_gen;
pub use go_types::{SourceRead, TraceConfig};
