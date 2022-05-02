// Copyright 2022 The Goscript Authors. All rights reserved.
// Use of this source code is governed by a BSD-style
// license that can be found in the LICENSE file.

use cg::SourceRead;
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
        let mut e = Engine {
            ffi: ffi,
            statics: statics,
        };
        crate::std::register(&mut e);
        e
    }

    pub fn set_std_in(&self, read: Box<dyn io::Read>) {
        self.statics.borrow_data_mut().std_in = Some(read);
    }

    pub fn set_std_out(&self, write: Box<dyn io::Write>) {
        self.statics.borrow_data_mut().std_out = Some(write);
    }

    pub fn set_std_err(&self, write: Box<dyn io::Write>) {
        self.statics.borrow_data_mut().std_err = Some(write);
    }

    pub fn register_extension(&mut self, name: &'static str, proto: Rc<dyn Ffi>) {
        self.ffi.register(name, proto);
    }

    pub fn run(
        &self,
        trace_parser: bool,
        trace_checker: bool,
        trace_vm: bool,
        reader: &dyn SourceRead,
        path: &str,
    ) -> usize {
        let cfg = types::TraceConfig {
            trace_parser: trace_parser,
            trace_checker: trace_checker,
        };
        let mut fs = fe::FileSet::new();
        let el = &mut fe::errors::ErrorList::new();
        let code = cg::entry::parse_check_gen(path, &cfg, reader, &mut fs, el);
        if let Ok(bc) = code {
            let vm = vm::vm::GosVM::new(bc, &self.ffi, Some(&fs));
            vm.run();
            0
        } else {
            if trace_vm {
                el.sort();
                print!("{}", el);
            }
            code.unwrap_err()
        }
    }
}
