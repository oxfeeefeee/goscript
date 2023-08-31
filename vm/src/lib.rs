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

pub mod parser {
    pub use go_parser::*;
}

pub use {
    ffi::*,
    go_parser::{Map, MapIter},
    go_pmacro::{ffi_impl, Ffi, UnsafePtr},
    value::Bytecode,
    vm::run,
    vm::PanicData,
};

pub struct CallStackDisplay<'a> {
    panic_data: &'a PanicData,
    bc: &'a Bytecode,
}

impl<'a> CallStackDisplay<'a> {
    pub fn new(panic_data: &'a PanicData, bc: &'a Bytecode) -> CallStackDisplay<'a> {
        Self { panic_data, bc }
    }
}

impl<'a> std::fmt::Display for CallStackDisplay<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for (fkey, pc) in self.panic_data.call_stack.iter() {
            let func = &self.bc.objects.functions[*fkey];
            if let Some(p) = func.pos[*pc as usize] {
                if let Some(fs) = &self.bc.file_set {
                    writeln!(
                        f,
                        "{}",
                        fs.position(p as usize)
                            .unwrap_or(go_parser::FilePos::null())
                    )?;
                } else {
                    writeln!(f, "fileset not available, pos:{}", p)?;
                }
            } else {
                f.write_str("<no debug info available for current frame>\n")?;
            };
        }
        Ok(())
    }
}
