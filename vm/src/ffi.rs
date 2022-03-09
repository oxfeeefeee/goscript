use super::objects::VMObjects;
use super::stack::Stack;
use super::value::{GosValue, RtMultiValResult};
use std::cell::RefCell;
use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;

pub type FfiCtorResult<T> = std::result::Result<T, String>;

pub type Ctor = dyn Fn(Vec<GosValue>) -> FfiCtorResult<Rc<RefCell<dyn Ffi>>>;

pub struct FfiCallCtx<'a> {
    pub func_name: &'a str,
    pub vm_objs: &'a VMObjects,
    pub stack: &'a Stack,
}

/// A FFI function call
pub trait Ffi {
    fn call(
        &self,
        ctx: &FfiCallCtx,
        params: Vec<GosValue>,
    ) -> Pin<Box<dyn Future<Output = RtMultiValResult> + '_>>;
}

impl std::fmt::Debug for dyn Ffi {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}", "ffi")
    }
}

pub struct FfiFactory {
    registry: HashMap<&'static str, Box<Ctor>>,
}

impl FfiFactory {
    pub fn new() -> FfiFactory {
        FfiFactory {
            registry: HashMap::new(),
        }
    }

    pub fn register(&mut self, name: &'static str, ctor: Box<Ctor>) {
        self.registry.insert(name, ctor);
    }

    pub fn create_by_name(
        &self,
        name: &str,
        params: Vec<GosValue>,
    ) -> FfiCtorResult<Rc<RefCell<dyn Ffi>>> {
        match self.registry.get(name) {
            Some(ctor) => (*ctor)(params),
            None => Err(format!("FFI named {} not found", name)),
        }
    }
}

impl std::fmt::Debug for FfiFactory {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "FfiFactory")
    }
}
