use goscript_vm::ffi::{Ffi, FfiCallCtx, FfiCtorResult};
use goscript_vm::instruction::ValueType;
use goscript_vm::value::{GosValue, IfaceUnderlying, PointerObj, RtMultiValResult, UserData};
use std::any::Any;
use std::cell::RefCell;
use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;

enum GosKind {
    Invalid = 0,
    Bool,
    Int,
    Int8,
    Int16,
    Int32,
    Int64,
    Uint,
    Uint8,
    Uint16,
    Uint32,
    Uint64,
    _Uintptr, // do not support for now
    Float32,
    Float64,
    Complex64,
    Complex128,
    Array,
    Chan,
    Func,
    Interface,
    Map,
    Ptr,
    Slice,
    String,
    Struct,
    UnsafePointer,
}
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
                let ud = params[0].as_pointer().as_user_data();
                let val = ud.as_any().downcast_ref::<StdValue>().unwrap().clone();
                let (typ, kind) = val.type_of(ctx);
                let p = PointerObj::UserData(Rc::new(typ));
                Box::pin(async move { Ok(vec![GosValue::new_pointer(p), GosValue::Uint(kind)]) })
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

    fn type_of(&self, ctx: &FfiCallCtx) -> (StdValue, usize) {
        let m = self.val.get_meta(ctx.vm_objs, ctx.stack);
        let typ = StdValue::new(GosValue::Metadata(m));
        let kind = match m
            .get_underlying(&ctx.vm_objs.metas)
            .get_value_type(&ctx.vm_objs.metas)
        {
            ValueType::Bool => GosKind::Bool,
            ValueType::Int => GosKind::Int,
            ValueType::Int8 => GosKind::Int8,
            ValueType::Int16 => GosKind::Int16,
            ValueType::Int32 => GosKind::Int32,
            ValueType::Int64 => GosKind::Int64,
            ValueType::Uint => GosKind::Uint,
            ValueType::Uint8 => GosKind::Uint8,
            ValueType::Uint16 => GosKind::Uint16,
            ValueType::Uint32 => GosKind::Uint32,
            ValueType::Uint64 => GosKind::Uint64,
            ValueType::Float32 => GosKind::Float32,
            ValueType::Float64 => GosKind::Float64,
            ValueType::Complex64 => GosKind::Complex64,
            ValueType::Complex128 => GosKind::Complex128,
            ValueType::Array => GosKind::Array,
            ValueType::Channel => GosKind::Chan,
            ValueType::Function => GosKind::Func,
            ValueType::Interface => GosKind::Interface,
            ValueType::Map => GosKind::Map,
            ValueType::Pointer => {
                let ptr: &PointerObj = &*self.val.as_pointer();
                match ptr {
                    PointerObj::UserData(_) => GosKind::UnsafePointer,
                    _ => GosKind::Ptr,
                }
            }
            ValueType::Slice => GosKind::Slice,
            ValueType::Str => GosKind::String,
            ValueType::Struct => GosKind::Struct,
            _ => GosKind::Invalid,
        };
        (typ, kind as usize)
    }
}
