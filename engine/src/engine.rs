// Copyright 2022 The Goscript Authors. All rights reserved.
// Use of this source code is governed by a BSD-style
// license that can be found in the LICENSE file.

pub use cg::SourceRead;
use vm::ffi::FfiStatics;

use super::ffi::Ffi;
use std::any::Any;
use std::cell::RefCell;
use std::cell::{Ref, RefMut};
use std::io;
use std::rc::Rc;

extern crate goscript_codegen as cg;
extern crate goscript_parser as fe;
extern crate goscript_types as types;
extern crate goscript_vm as vm;

#[derive(Default)]
pub struct StaticData {
    pub std_in: Option<Box<dyn io::Read>>,

    pub std_out: Option<Box<dyn io::Write>>,

    pub std_err: Option<Box<dyn io::Write>>,
}

#[derive(Default, Clone)]
pub struct Statics {
    inner: Rc<RefCell<StaticData>>,
}

impl Statics {
    pub fn downcast_borrow_data(s: &dyn FfiStatics) -> Ref<StaticData> {
        let sta = s.as_any().downcast_ref::<Statics>();
        sta.as_ref().unwrap().borrow_data()
    }

    pub fn downcast_borrow_data_mut(s: &dyn FfiStatics) -> RefMut<StaticData> {
        let sta = s.as_any().downcast_ref::<Statics>();
        sta.as_ref().unwrap().borrow_data_mut()
    }

    pub fn borrow_data(&self) -> Ref<StaticData> {
        self.inner.borrow()
    }

    pub fn borrow_data_mut(&self) -> RefMut<StaticData> {
        self.inner.borrow_mut()
    }
}

impl FfiStatics for Statics {
    fn as_any(&self) -> &dyn Any {
        self
    }
}

pub struct Engine {
    ffi: vm::ffi::FfiFactory,
    statics: Statics,
}

impl Engine {
    pub fn new() -> Engine {
        let statics = Statics::default();
        let ffi = vm::ffi::FfiFactory::new(Box::new(statics.clone()));
        let mut e = Engine { ffi, statics };
        crate::std::register(&mut e);
        e
    }

    pub fn set_std_io(
        &self,
        std_in: Option<Box<dyn std::io::Read>>,
        std_out: Option<Box<dyn std::io::Write>>,
        std_err: Option<Box<dyn std::io::Write>>,
    ) {
        let mut borrow = self.statics.borrow_data_mut();
        borrow.std_in = std_in;
        borrow.std_out = std_out;
        borrow.std_err = std_err;
    }

    pub fn register_extension(&mut self, name: &'static str, proto: Rc<dyn Ffi>) {
        self.ffi.register(name, proto);
    }

    pub fn run<S: SourceRead>(
        &self,
        trace_parser: bool,
        trace_checker: bool,
        reader: &S,
        path: &str,
    ) -> Result<(), fe::errors::ErrorList> {
        let cfg = types::TraceConfig {
            trace_parser,
            trace_checker,
        };
        let mut fs = fe::FileSet::new();
        let code = cg::entry::parse_check_gen(path, &cfg, reader, &mut fs)?;
        vm::vm::run(&code, &self.ffi, Some(&fs));
        Ok(())
    }
}
