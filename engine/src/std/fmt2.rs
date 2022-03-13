use crate::ffi::*;
use crate::non_async_result;
use goscript_vm::value::{GosValue, RtMultiValResult};
use std::cell::RefCell;
use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;

pub struct Fmt2 {}

impl Ffi for Fmt2 {
    fn call(
        &self,
        ctx: &FfiCallCtx,
        params: Vec<GosValue>,
    ) -> Pin<Box<dyn Future<Output = RtMultiValResult> + '_>> {
        match ctx.func_name {
            "println" => self.println(params),
            "printf" => self.printf(params),
            _ => unreachable!(),
        }
        non_async_result![]
    }
}

impl Fmt2 {
    pub fn new(_v: Vec<GosValue>) -> FfiCtorResult<Rc<RefCell<dyn Ffi>>> {
        Ok(Rc::new(RefCell::new(Fmt2 {})))
    }

    fn println(&self, params: Vec<GosValue>) {
        let vec = params[0].as_slice().0.get_vec();
        let strs: Vec<String> = vec
            .iter()
            .map(|x| {
                if x.is_nil() {
                    "<nil>".to_string()
                } else {
                    match x.iface_underlying() {
                        Some(v) => v.to_string(),
                        None => "<ffi>".to_string(),
                    }
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
