extern crate self as goscript_engine;
use crate::ffi::*;
use goscript_vm::value::GosValue;
use std::cell::RefCell;
use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;

#[derive(Ffi)]
pub struct Fmt2 {}

#[ffi_impl]
impl Fmt2 {
    pub fn new(_v: Vec<GosValue>) -> Fmt2 {
        Fmt2 {}
    }

    fn ffi_println(&self, args: Vec<GosValue>) {
        let vec = args[0].as_slice().0.get_vec();
        let strs: Vec<String> = vec
            .iter()
            .map(|x| {
                if x.is_nil() {
                    "<nil>".to_string()
                } else {
                    match x.iface_underlying() {
                        Some(v) => v.to_string(),
                        None => "<ffi>".to_string(),
                    }
                }
            })
            .collect();
        println!("{}", strs.join(", "));
    }

    fn ffi_printf(&self, args: Vec<GosValue>) {
        let _vec = args[0].as_slice().0.get_vec();
        unimplemented!();
    }
}
