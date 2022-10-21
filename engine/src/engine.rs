// Copyright 2022 The Goscript Authors. All rights reserved.
// Use of this source code is governed by a BSD-style
// license that can be found in the LICENSE file.

use crate::ffi::Ffi;
#[cfg(feature = "go_std")]
use crate::std::os;
use borsh::{BorshDeserialize, BorshSerialize};
use std::rc::Rc;

#[cfg(feature = "codegen")]
pub use cg::SourceRead;
#[cfg(feature = "codegen")]
extern crate goscript_codegen as cg;
#[cfg(feature = "codegen")]
extern crate goscript_parser as fe;
#[cfg(feature = "codegen")]
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
            crate::std::register(&mut e.ffi);
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

    pub fn run_bytecode(&self, bc: &vm::Bytecode) {
        vm::run(bc, &self.ffi, None)
    }

    #[cfg(feature = "codegen")]
    pub fn run_source<S: SourceRead>(
        &self,
        trace_parser: bool,
        trace_checker: bool,
        reader: &S,
        path: &str,
    ) -> Result<(), fe::ErrorList> {
        let cfg = types::TraceConfig {
            trace_parser,
            trace_checker,
        };
        let mut fs = fe::FileSet::new();
        let code = cg::parse_check_gen(path, &cfg, reader, &mut fs)?;
        let encoded = code.try_to_vec().unwrap();
        let decoded = goscript_vm::Bytecode::try_from_slice(&encoded).unwrap();
        dbg!(encoded.len());
        dbg!(base64::encode(encoded));
        vm::run(&decoded, &self.ffi, Some(&fs));
        Ok(())
    }
}
