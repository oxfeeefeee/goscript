// Copyright 2022 The Goscript Authors. All rights reserved.
// Use of this source code is governed by a BSD-style
// license that can be found in the LICENSE file.

extern crate self as goscript_engine;
use crate::ffi::*;
use goscript_vm::types::{GosElem, GosValue};

#[derive(Ffi)]
pub struct Fmt2Ffi;

#[ffi_impl]
impl Fmt2Ffi {
    fn ffi_println(args: GosValue) -> RuntimeResult<()> {
        let vec = FfiCtx::slice_as_rust_slice::<GosElem>(&args)?;
        let strs: Vec<String> = vec
            .iter()
            .map(|x| {
                let val = x.borrow();
                let s = if val.is_nil() {
                    "<nil>".to_owned()
                } else {
                    let underlying = val.iface_underlying()?;
                    match underlying {
                        Some(v) => v.to_string(),
                        None => "<ffi>".to_owned(),
                    }
                };
                Ok(s)
            })
            .map(|x: RuntimeResult<String>| x.unwrap())
            .collect();
        #[cfg(feature = "wasm")]
        crate::std::wasm::console_log(&strs.join(", "));
        #[cfg(not(feature = "wasm"))]
        println!("{}", strs.join(", "));
        Ok(())
    }
}
