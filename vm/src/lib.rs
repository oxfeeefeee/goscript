// Copyright 2022 The Goscript Authors. All rights reserved.
// Use of this source code is governed by a BSD-style
// license that can be found in the LICENSE file.

//pub mod instruction;
pub mod instruction;

#[macro_use]
pub mod metadata;

mod channel;

pub mod objects;

pub mod ffi;

pub mod value;

//mod stack;
mod stack2;

//#[macro_use]
//pub mod vm;

#[macro_use]
mod vm2;

pub mod gc;
