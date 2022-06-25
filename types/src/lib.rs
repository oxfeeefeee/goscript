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

#[macro_use]
mod typ;

mod lookup;
mod operand;
mod selection;

mod universe;

mod display;

mod check;

mod importer;

pub use check::{DeclInfo, TypeInfo};
pub use constant::Value as ConstValue;
pub use display::Displayer;
pub use importer::{ImportKey, Importer, SourceRead, TraceConfig};
pub use obj::EntityType;
pub use objects::{DeclInfoKey, ObjKey, PackageKey, ScopeKey, TCObjects, TypeKey};
pub use operand::OperandMode;
pub use selection::SelectionKind;
pub use typ::{identical, identical_ignore_tags, BasicType, ChanDir, Type};
pub use universe::{Builtin, Universe};
