use goscript_vm::ffi::{Ffi, FfiResult};
use goscript_vm::value::GosValue;
use std::cell::RefCell;
use std::rc::Rc;

pub struct Fmt {}

impl Ffi for Fmt {
    fn call(&self, func_name: &str, params: Vec<GosValue>) -> Vec<GosValue> {
        if func_name == "println" {
            self.println(params)
        }
        vec![]
    }
}

impl Fmt {
    pub fn new(_v: Vec<GosValue>) -> FfiResult<Rc<RefCell<dyn Ffi>>> {
        Ok(Rc::new(RefCell::new(Fmt {})))
    }

    fn println(&self, params: Vec<GosValue>) {
        let vec = params[0].as_slice().get_vec();
        let strs: Vec<String> = vec
            .iter()
            .map(|x| {
                if x.is_nil() {
                    "<nil>".to_string()
                } else {
                    let iface = x.as_interface().borrow();
                    iface.underlying_value().unwrap().to_string()
                }
            })
            .collect();
        println!("{}", strs.join(", "));
    }
}
