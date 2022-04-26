// Copyright 2022 The Goscript Authors. All rights reserved.
// Use of this source code is governed by a BSD-style
// license that can be found in the LICENSE file.

extern crate self as goscript_engine;
use crate::ffi::*;
use goscript_vm::instruction::ValueType;
use goscript_vm::value::{GosElem, GosValue};
use std::cell::RefCell;
use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;

#[derive(Ffi)]
pub struct Fmt2Ffi {}

#[ffi_impl]
impl Fmt2Ffi {
    pub fn new(_v: Vec<GosValue>) -> Fmt2Ffi {
        Fmt2Ffi {}
    }

    fn ffi_println(&self, args: Vec<GosValue>) -> RuntimeResult<()> {
        let vec = args[0]
            .as_some_slice::<GosElem>()?
            .0
            .get_vec(ValueType::Interface);
        let strs: Vec<String> = vec
            .iter()
            .map(|x| {
                let s = if x.is_nil() {
                    "<nil>".to_owned()
                } else {
                    let underlying = x.iface_underlying()?;
                    match underlying {
                        Some(v) => v.to_string(),
                        None => "<ffi>".to_owned(),
                    }
                };
                Ok(s)
            })
            .map(|x: RuntimeResult<String>| x.unwrap())
            .collect();
        println!("{}", strs.join(", "));
        Ok(())
    }

    fn ffi_printf(&self, _args: Vec<GosValue>) {
        unimplemented!();
    }
}
