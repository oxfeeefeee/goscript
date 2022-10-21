// Copyright 2022 The Goscript Authors. All rights reserved.
// Use of this source code is governed by a BSD-style
// license that can be found in the LICENSE file.

use crate::ffi::Ffi;
#[cfg(feature = "go_std")]
use crate::std::os;
use borsh::{BorshDeserialize, BorshSerialize};
pub use cg::SourceRead;
use std::rc::Rc;
extern crate goscript_codegen as cg;
extern crate goscript_parser as fe;
extern crate goscript_types as types;
extern crate goscript_vm as vm;

pub struct Engine {
    ffi: vm::FfiFactory,
}

impl Engine {
    pub fn new() -> Engine {
        let ffi = vm::FfiFactory::new();
        #[cfg(feature = "go_std")]
        {
            let mut e = Engine { ffi };
            crate::std::register(&mut e);
            e
        }
        #[cfg(not(feature = "go_std"))]
        Engine { ffi }
    }

    #[cfg(feature = "go_std")]
    pub fn set_std_io(
        &self,
        std_in: Option<Box<dyn std::io::Read + Sync + Send>>,
        std_out: Option<Box<dyn std::io::Write + Sync + Send>>,
        std_err: Option<Box<dyn std::io::Write + Sync + Send>>,
    ) {
        os::set_std_io(std_in, std_out, std_err);
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
        let encoded = code.try_to_vec().unwrap();
        let decoded = goscript_vm::Bytecode::try_from_slice(&encoded).unwrap();
        dbg!(encoded.len());
        //dbg!(base64::encode(encoded));
        vm::run(&decoded, &self.ffi, Some(&fs));
        Ok(())
    }
}
