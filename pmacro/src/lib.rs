// Copyright 2022 The Goscript Authors. All rights reserved.
// Use of this source code is governed by a BSD-style
// license that can be found in the LICENSE file.

//! This crate is part of the Goscript project. Please refer to <https://goscript.dev> for more information.
//!
//! This crate provides some proc-macros for the project.
//!

mod async_fn;
mod ffi;
mod ffi_impl;
mod unsafe_ptr;

#[proc_macro_derive(Ffi)]
pub fn derive_ffi(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    ffi::derive_ffi_implement(input)
}

#[proc_macro_attribute]
pub fn ffi_impl(
    args: proc_macro::TokenStream,
    input: proc_macro::TokenStream,
) -> proc_macro::TokenStream {
    ffi_impl::ffi_impl_implement(args, input)
}

#[proc_macro_derive(UnsafePtr)]
pub fn derive_unsafe_ptr(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    unsafe_ptr::derive_unsafe_ptr_implement(input)
}

#[proc_macro_attribute]
pub fn async_fn(
    _: proc_macro::TokenStream,
    input: proc_macro::TokenStream,
) -> proc_macro::TokenStream {
    async_fn::async_fn_implement(input)
}
