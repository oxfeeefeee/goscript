// Copyright 2022 The Goscript Authors. All rights reserved.
// Use of this source code is governed by a BSD-style
// license that can be found in the LICENSE file.

mod instruction;

#[macro_use]
mod metadata;

#[cfg(feature = "async")]
mod channel;

mod objects;

pub mod ffi;

#[macro_use]
mod dispatcher;
pub mod value;

mod stack;

pub mod vm;

pub mod gc;
