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

    fn ffi_println(&self, args: Vec<GosValue>) -> RuntimeResult<()> {
        let vec = args[0].as_some_slice()?.0.get_vec();
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

    fn ffi_printf(&self, args: Vec<GosValue>) {
        unimplemented!();
    }
}
