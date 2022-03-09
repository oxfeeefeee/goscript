use goscript_vm::ffi::{Ffi, FfiCallCtx, FfiCtorResult};
use goscript_vm::value::{GosValue, RtMultiValResult};
use std::cell::RefCell;
use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;

pub struct Bits {}

impl Ffi for Bits {
    fn call(
        &self,
        ctx: &FfiCallCtx,
        params: Vec<GosValue>,
    ) -> Pin<Box<dyn Future<Output = RtMultiValResult> + '_>> {
        let v = match ctx.func_name {
            "f32_to_bits" => self.f32_to_bits(params),
            "f32_from_bits" => self.f32_from_bits(params),
            "f64_to_bits" => self.f64_to_bits(params),
            "f64_from_bits" => self.f64_from_bits(params),
            _ => unreachable!(),
        };
        Box::pin(async move { Ok(v) })
    }
}

impl Bits {
    pub fn new(_v: Vec<GosValue>) -> FfiCtorResult<Rc<RefCell<dyn Ffi>>> {
        Ok(Rc::new(RefCell::new(Bits {})))
    }

    fn f32_to_bits(&self, params: Vec<GosValue>) -> Vec<GosValue> {
        let result = u32::from_be_bytes(params[0].as_float32().to_be_bytes());
        vec![GosValue::Uint32(result)]
    }

    fn f32_from_bits(&self, params: Vec<GosValue>) -> Vec<GosValue> {
        let result = f32::from_be_bytes(params[0].as_uint32().to_be_bytes());
        vec![GosValue::Float32(result.into())]
    }

    fn f64_to_bits(&self, params: Vec<GosValue>) -> Vec<GosValue> {
        let result = u64::from_be_bytes(params[0].as_float().to_be_bytes());
        vec![GosValue::Uint64(result)]
    }

    fn f64_from_bits(&self, params: Vec<GosValue>) -> Vec<GosValue> {
        let result = f64::from_be_bytes(params[0].as_uint64().to_be_bytes());
        vec![GosValue::Float64(result.into())]
    }
}
