extern crate goscript_codegen as cg;
extern crate goscript_parser as fe;
extern crate goscript_types as types;
extern crate goscript_vm as vm;
use super::std::{bits, fmt2, reflect, sync};

pub struct Config {
    // working directory
    pub work_dir: Option<String>,
    // base path for non-local imports
    pub base_path: Option<String>,
    // print debug info in parser
    pub trace_parser: bool,
    // print debug info in checker
    pub trace_checker: bool,
    // proint debug info for vm
    pub trace_vm: bool,
}

pub struct Engine {
    config: Config,
    ffi: vm::ffi::FfiFactory,
}

impl Engine {
    pub fn new(config: Config) -> Engine {
        let ffi = vm::ffi::FfiFactory::new();
        Engine {
            config: config,
            ffi: ffi,
        }
    }

    fn register_std(&mut self) {
        fmt2::Fmt2::register(self);
        bits::Bits::register(self);
        sync::Mutex::register(self);
        sync::RWMutex::register(self);
        reflect::Reflect::register(self);
    }

    pub fn run(&mut self, path: &str) -> usize {
        self.register_std();

        let config = types::Config {
            work_dir: self.config.work_dir.clone(),
            base_path: self.config.base_path.clone(),
            trace_parser: self.config.trace_parser,
            trace_checker: self.config.trace_checker,
        };
        let mut fs = fe::FileSet::new();
        let el = &mut fe::errors::ErrorList::new();
        let code = cg::entry::parse_check_gen(path, &config, &mut fs, el);
        if let Ok(bc) = code {
            let vm = vm::vm::GosVM::new(bc, &self.ffi, Some(&fs));
            vm.run();
            0
        } else {
            if self.config.trace_vm {
                el.sort();
                print!("{}", el);
            }
            code.unwrap_err()
        }
    }

    pub fn register_extension(&mut self, name: &'static str, ctor: Box<vm::ffi::Ctor>) {
        self.ffi.register(name, ctor);
    }
}
