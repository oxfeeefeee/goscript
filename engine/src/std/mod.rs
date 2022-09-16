// Copyright 2022 The Goscript Authors. All rights reserved.
// Use of this source code is governed by a BSD-style
// license that can be found in the LICENSE file.

mod bits;
mod fmt2;
mod io;
pub(crate) mod os;
mod reflect;
#[cfg(feature = "async")]
mod sync;

pub(crate) fn register(engine: &mut crate::engine::Engine) {
    fmt2::Fmt2Ffi::register(engine);
    bits::BitsFfi::register(engine);
    #[cfg(feature = "async")]
    sync::MutexFfi::register(engine);
    #[cfg(feature = "async")]
    sync::RWMutexFfi::register(engine);
    reflect::ReflectFfi::register(engine);
    io::IoFfi::register(engine);
    os::FileFfi::register(engine);
}
