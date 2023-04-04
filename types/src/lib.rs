// Copyright 2022 The Goscript Authors. All rights reserved.
// Use of this source code is governed by a BSD-style
// license that can be found in the LICENSE file.

//! This crate is part of the Goscript project. Please refer to <https://goscript.dev> for more information.
//!
//! It's a port of the the parser from the Go standard library <https://github.com/golang/go/tree/release-branch.go1.12/src/go/types>
//!
//! # Feature
//! - `btree_map`: Make it use BTreeMap instead of HashMap
//!

mod constant;
mod obj;
mod package;
mod scope;
#[macro_use]
mod objects;
mod display;
mod importer;
mod lookup;
mod operand;
mod selection;
mod universe;

#[macro_use]
pub mod typ;
pub mod check;

pub use constant::Value as ConstValue;
pub use display::Displayer;
pub use importer::*;
pub use obj::*;
pub use objects::*;
pub use operand::OperandMode;
pub use selection::*;
pub use universe::*;
