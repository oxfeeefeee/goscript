// Copyright 2022 The Goscript Authors. All rights reserved.
// Use of this source code is governed by a BSD-style
// license that can be found in the LICENSE file.

use crate::ffi::Ffi;
#[cfg(feature = "go_std")]
use crate::std::os;
use borsh::{BorshDeserialize, BorshSerialize};
use std::path::Path;
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
        #[cfg(not(feature = "go_std"))]
        {
            Engine {
                ffi: vm::FfiFactory::new(),
            }
        }

        #[cfg(feature = "go_std")]
        {
            let mut e = Engine {
                ffi: vm::FfiFactory::new(),
            };
            crate::std::register(&mut e.ffi);
            e
        }
    }

    pub fn with_user_data(data: usize) -> Engine {
        #[cfg(not(feature = "go_std"))]
        {
            Engine {
                ffi: vm::FfiFactory::with_user_data(data),
            }
        }

        #[cfg(feature = "go_std")]
        {
            let mut e = Engine {
                ffi: vm::FfiFactory::with_user_data(data),
            };
            crate::std::register(&mut e.ffi);
            e
        }
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

    #[cfg(feature = "codegen")]
    pub fn compile<S: SourceRead>(
        &self,
        trace_parser: bool,
        trace_checker: bool,
        reader: &S,
        path: &Path,
    ) -> Result<(vm::Bytecode, fe::FileSet), fe::ErrorList> {
        let cfg = types::TraceConfig {
            trace_parser,
            trace_checker,
        };
        let mut fs = fe::FileSet::new();
        cg::parse_check_gen(path, &cfg, reader, &mut fs).map(|x| (x, fs))
    }

    #[cfg(feature = "codegen")]
    pub fn compile_serialize<S: SourceRead>(
        &self,
        trace_parser: bool,
        trace_checker: bool,
        reader: &S,
        path: &Path,
    ) -> Result<Vec<u8>, fe::ErrorList> {
        self.compile(trace_parser, trace_checker, reader, path)
            .map(|(code, _)| code.try_to_vec().unwrap())
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
        path: &Path,
    ) -> Result<(), fe::ErrorList> {
        self.compile(trace_parser, trace_checker, reader, path)
            .map(|(code, fs)| {
                let encoded = code.try_to_vec().unwrap();
                let decoded = goscript_vm::Bytecode::try_from_slice(&encoded).unwrap();
                dbg!(encoded.len());
                //dbg!(base64::encode(encoded));
                vm::run(&decoded, &self.ffi, Some(&fs))
            })
    }
}
