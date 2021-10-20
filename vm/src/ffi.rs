use super::value::GosValue;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

pub type FfiCtorResult<T> = std::result::Result<T, String>;

pub type Ctor = dyn Fn(Vec<GosValue>) -> FfiCtorResult<Rc<RefCell<dyn Ffi>>>;

/// A FFI function call
pub trait Ffi {
    fn call(&self, func_name: &str, params: Vec<GosValue>) -> Vec<GosValue>;
}

impl std::fmt::Debug for dyn Ffi {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}", "ffi")
    }
}

/// User data handle
///
pub trait UserData {
    /// Returns true if the user data can make reference cycles, so that GC can
    fn can_make_cycle() -> bool {
        false
    }
    /// If can_make_cycle returns true, implement this to break cycle
    fn break_cycle() {}
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
