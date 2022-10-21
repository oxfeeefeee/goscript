// Copyright 2022 The Goscript Authors. All rights reserved.
// Use of this source code is governed by a BSD-style
// license that can be found in the LICENSE file.

/// Goscirpt's type checker
/// This is a port of the offical Go 1.12 type checker
/// A lot of comments are directly taken from Go source file.
///
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
