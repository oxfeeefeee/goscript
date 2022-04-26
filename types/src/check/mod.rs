// Copyright 2022 The Goscript Authors. All rights reserved.
// Use of this source code is governed by a BSD-style
// license that can be found in the LICENSE file.

#[macro_use]
mod util;

mod assignment;
mod builtin;
mod call;
pub mod check;
mod conversion;
mod decl;
mod expr;
mod initorder;
mod interface;
mod label;
mod resolver;
mod returns;
mod stmt;
mod typexpr;

pub use check::TypeInfo;
pub use check::{Checker, FilesContext};
pub use interface::{IfaceInfo, MethodInfo};
pub use resolver::DeclInfo;
