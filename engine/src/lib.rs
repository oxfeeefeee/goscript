// Copyright 2022 The Goscript Authors. All rights reserved.
// Use of this source code is governed by a BSD-style
// license that can be found in the LICENSE file.

//! This crate is part of the Goscript project. Please refer to <https://goscript.dev> for more information.
//!
//! It's a wapper of all the parts of the Goscript project. It also implements the standard library,
//! the standard library part is still under development, only a few parts are implemented.
//!  
//! # Example:
//! ```
//! use std::path::{Path, PathBuf};
//! use go_engine::{Config, ErrorList, SourceReader, run};
//!
//!fn run_file(path: &str, trace: bool) -> Result<(), ErrorList> {
//!    let mut cfg = Config::default();
//!    cfg.trace_parser = trace;
//!    cfg.trace_checker = trace;
//!    let sr = SourceReader::local_fs(PathBuf::from("../std/"), PathBuf::from("./"));
//!    let result = run(cfg, &sr, Path::new(path), None);
//!    if let Err(el) = &result {
//!        el.sort();
//!        eprint!("{}", el);
//!    }
//!    result
//!}
//! ```
//!
//! # Feature
//! The project is entended to be enbedded, so it has a lot of feature flags to turn on/off different parts.
//! - `read_fs`: Read source code from local file system
//! - `read_zip`: Read source code from zip file
//! - `async`: Channel and goroutine support
//! - `go_std`: Enable the Go standard library
//! - `btree_map`: Make it use BTreeMap instead of HashMap
//! - `codegen`: Enable codegen
//! - `instruction_pos`: Add instruction position to bytecode for debugging
//! - `serde_borsh`: Serde support for bytecode using Borsh
//! - `wasm`: Enable wasm support
//!

mod engine;

#[cfg(feature = "go_std")]
mod std;

mod vfs;

mod source;

#[macro_use]
pub mod ffi;

#[cfg(feature = "go_std")]
#[macro_use]
extern crate lazy_static;

pub use engine::*;
pub use go_parser::{ErrorList, FileSet};
pub use source::*;

pub use crate::vfs::{compound::CompoundFs, vfs_map::VfsMap, VirtualFs};

#[cfg(feature = "read_fs")]
pub use crate::vfs::vfs_fs::VfsFs;
#[cfg(feature = "read_zip")]
pub use crate::vfs::vfs_zip::VfsZip;
