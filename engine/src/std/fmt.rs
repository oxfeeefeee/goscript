use goscript_vm::ffi::{Ffi, FfiCtorResult};
use goscript_vm::value::{GosValue, RtMultiValResult};
use std::cell::RefCell;
use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;

pub struct Fmt {}

impl Ffi for Fmt {
    fn call(
        &self,
        func_name: &str,
        params: Vec<GosValue>,
    ) -> Pin<Box<dyn Future<Output = RtMultiValResult> + '_>> {
        match func_name {
            "println" => self.println(params),
            "printf" => self.printf(params),
            _ => unreachable!(),
        }
        Box::pin(async move { Ok(vec![]) })
    }
}

impl Fmt {
    pub fn new(_v: Vec<GosValue>) -> FfiCtorResult<Rc<RefCell<dyn Ffi>>> {
        Ok(Rc::new(RefCell::new(Fmt {})))
    }

    fn println(&self, params: Vec<GosValue>) {
        let vec = params[0].as_slice().0.get_vec();
        let strs: Vec<String> = vec
            .iter()
            .map(|x| {
                if x.is_nil() {
                    "<nil>".to_string()
                } else {
                    x.iface_underlying().unwrap().to_string()
                }
            })
            .collect();
        println!("{}", strs.join(", "));
    }

    fn printf(&self, params: Vec<GosValue>) {
        let _vec = params[0].as_slice().0.get_vec();
        unimplemented!();
    }
}
