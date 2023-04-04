// Copyright 2022 The Goscript Authors. All rights reserved.
// Use of this source code is governed by a BSD-style
// license that can be found in the LICENSE file.

//! This crate is part of the Goscript project. Please refer to <https://goscript.dev> for more information.
//!
//! # Feature
//! - `async`: Channel and goroutine support
//! - `btree_map`: Make it use BTreeMap instead of HashMap
//! - `instruction_pos`: Add instruction position to bytecode for debugging
//! - `serde_borsh`: Serde support for bytecode using Borsh

mod instruction;
#[macro_use]
mod metadata;
#[cfg(feature = "async")]
mod channel;
mod objects;
#[macro_use]
mod dispatcher;
mod bytecode;
mod ffi;
mod stack;
mod value;
mod vm;

pub mod gc;
pub mod types {
    pub use crate::value::*;
}

pub use {
    ffi::*,
    go_pmacro::{ffi_impl, Ffi, UnsafePtr},
    value::Bytecode,
    vm::run,
};
