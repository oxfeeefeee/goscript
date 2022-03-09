use goscript_vm::ffi::{Ffi, FfiCallCtx, FfiCtorResult};
use goscript_vm::value::{GosValue, IfaceUnderlying, PointerObj, RtMultiValResult, UserData};
use std::any::Any;
use std::cell::RefCell;
use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;

pub struct Reflect {}

impl Ffi for Reflect {
    fn call(
        &self,
        ctx: &FfiCallCtx,
        params: Vec<GosValue>,
    ) -> Pin<Box<dyn Future<Output = RtMultiValResult> + '_>> {
        match ctx.func_name {
            "value_of" => {
                let p = PointerObj::UserData(Rc::new(StdValue::value_of(params)));
                Box::pin(async move { Ok(vec![GosValue::new_pointer(p)]) })
            }
            "type_of" => {
                let p = PointerObj::UserData(Rc::new(StdValue::type_of(ctx, params)));
                Box::pin(async move { Ok(vec![GosValue::new_pointer(p)]) })
            }
            _ => unreachable!(),
        }
    }
}

impl Reflect {
    pub fn new(_v: Vec<GosValue>) -> FfiCtorResult<Rc<RefCell<dyn Ffi>>> {
        Ok(Rc::new(RefCell::new(Reflect {})))
    }
}

#[derive(Clone)]
struct StdValue {
    val: GosValue,
}

impl UserData for StdValue {
    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl StdValue {
    fn new(v: GosValue) -> StdValue {
        StdValue { val: v }
    }

    fn value_of(v: Vec<GosValue>) -> StdValue {
        let iface = v[0].as_interface().borrow();
        let v = match &iface.underlying() {
            IfaceUnderlying::Gos(v, _) => v.clone(),
            // todo: should we return something else?
            IfaceUnderlying::Ffi(_) => GosValue::Nil(iface.meta),
            IfaceUnderlying::None => GosValue::Nil(iface.meta),
        };
        StdValue::new(v)
    }

    fn type_of(ctx: &FfiCallCtx, v: Vec<GosValue>) -> StdValue {
        let stdv = StdValue::value_of(v);
        let m = stdv.val.get_meta(ctx.vm_objs, ctx.stack);
        StdValue::new(GosValue::Metadata(m))
    }
}
