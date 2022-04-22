extern crate self as goscript_engine;
use crate::ffi::*;
use goscript_vm::value::GosValue;
use goscript_vm::value::UnsafePtr;
use std::any::Any;
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

#[derive(Clone, Debug, UnsafePtr)]
pub enum StdIo {
    In,
    Out,
    Err,
}

impl StdIo {
    fn new(v: Vec<GosValue>) -> StdIo {
        let id = *v[0].as_int();
        match id {
            0 => StdIo::In,
            1 => StdIo::Out,
            2 => StdIo::Err,
            _ => unreachable!(),
        }
    }
}
