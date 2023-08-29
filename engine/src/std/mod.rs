// Copyright 2022 The Goscript Authors. All rights reserved.
// Use of this source code is governed by a BSD-style
// license that can be found in the LICENSE file.

/// Go 1.12
///
mod bits;
mod fmt2;
mod io;
pub(crate) mod os;
mod reflect;
#[cfg(feature = "async")]
mod sync;
#[cfg(target_arch = "wasm32")]
mod wasm;

pub(crate) fn register(factory: &mut go_vm::FfiFactory) {
    fmt2::Fmt2Ffi::register(factory);
    bits::BitsFfi::register(factory);
    #[cfg(feature = "async")]
    sync::MutexFfi::register(factory);
    #[cfg(feature = "async")]
    sync::RWMutexFfi::register(factory);
    reflect::ReflectFfi::register(factory);
    io::IoFfi::register(factory);
    os::FileFfi::register(factory);
}
