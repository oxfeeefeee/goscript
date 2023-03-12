// Copyright 2022 The Goscript Authors. All rights reserved.
// Use of this source code is governed by a BSD-style
// license that can be found in the LICENSE file.

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
pub use goscript_parser::ErrorList;
pub use source::*;

pub use crate::vfs::{compound::CompoundFs, vfs_map::VfsMap, VirtualFs};

#[cfg(feature = "read_fs")]
pub use crate::vfs::vfs_fs::VfsFs;
#[cfg(feature = "read_zip")]
pub use crate::vfs::vfs_zip::VfsZip;
