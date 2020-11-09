use std::cell::RefCell;
use std::rc::Rc;

pub trait Ffi {
    fn name(&self) -> &'static str;
}

impl std::fmt::Debug for dyn Ffi {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}", "ffi obj")
    }
}

pub fn create_by_name(_name: &str) -> Rc<RefCell<dyn Ffi>> {
    Rc::new(RefCell::new(TestStruct {}))
}

pub struct TestStruct {}

impl Ffi for TestStruct {
    fn name(&self) -> &'static str {
        "test"
    }
}
