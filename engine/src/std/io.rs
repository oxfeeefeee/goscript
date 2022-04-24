extern crate self as goscript_engine;
use crate::ffi::*;
use goscript_vm::value::GosValue;
use std::cell::RefCell;
use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;

#[derive(Ffi)]
pub struct IoFfi {}

#[ffi_impl]
impl IoFfi {
    fn new(_v: Vec<GosValue>) -> IoFfi {
        unimplemented!()
    }
}
