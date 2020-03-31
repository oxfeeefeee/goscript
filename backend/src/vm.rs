#![allow(dead_code)]
use super::codegen::ByteCode;
use super::types::Objects as VMObjects;
use super::types::*;
use super::value::GosValue;
use std::cell::{Ref, RefCell, RefMut};
use std::rc::Rc;

#[derive(Clone, Debug)]
struct CallFrame {
    closure: ClosureKey,
    ip: usize,
    stack_base: usize,
}

#[derive(Clone, Debug)]
pub struct Fiber<'b> {
    stack: Vec<GosValue>,
    frames: Vec<CallFrame>,
    stack_top: usize,
    code: &'b ByteCode,
    objects: Rc<RefCell<VMObjects>>,
    caller: Option<Rc<RefCell<Fiber<'b>>>>,
}

impl<'b> Fiber<'b> {
    pub fn new(
        caller: Option<Rc<RefCell<Fiber<'b>>>>,
        code: &'b ByteCode,
        objects: Rc<RefCell<VMObjects>>,
    ) -> Fiber<'b> {
        Fiber {
            stack: Vec::new(),
            frames: Vec::new(),
            stack_top: 0,
            caller: caller,
            code: code,
            objects: objects,
        }
    }

    pub fn run(&mut self, closure: ClosureKey) {
        let cls = &self.objects().closures[closure];
        let func = &self.code.objects.functions[cls.func];
        dbg!(func);
    }

    fn objects(&self) -> Ref<VMObjects> {
        self.objects.borrow()
    }

    fn mut_objects(&self) -> RefMut<VMObjects> {
        self.objects.borrow_mut()
    }
}

pub struct GosVM<'b> {
    current_fiber: Option<Rc<RefCell<Fiber<'b>>>>,
    objects: Rc<RefCell<VMObjects>>,
    code: &'b ByteCode,
}

impl<'b> GosVM<'b> {
    pub fn new(bc: &'b ByteCode) -> GosVM<'b> {
        let objs = Rc::new(RefCell::new(VMObjects::new()));
        let mut vm = GosVM {
            current_fiber: None,
            objects: objs.clone(),
            code: bc,
        };
        vm.current_fiber = Some(Rc::new(RefCell::new(Fiber::new(None, bc, objs))));
        vm
    }

    pub fn run(&self, entry: FunctionKey) {
        let closure = self
            .borrow_mut_objects()
            .closures
            .insert(ClosureVal::new(entry));
        let fb = self.current_fiber.as_ref().unwrap();
        fb.borrow_mut().run(closure);
    }

    fn borrow_objects(&self) -> Ref<VMObjects> {
        self.objects.borrow()
    }

    fn borrow_mut_objects(&self) -> RefMut<VMObjects> {
        self.objects.borrow_mut()
    }
}
