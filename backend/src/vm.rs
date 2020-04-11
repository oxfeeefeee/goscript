#![allow(dead_code)]
use super::codegen::ByteCode;
use super::opcode::*;
use super::primitive::Primitive;
use super::types::Objects as VMObjects;
use super::types::*;
use super::value::GosValue;
use std::cell::{Ref, RefCell, RefMut};
use std::collections::HashMap;
use std::rc::Rc;

#[derive(Clone, Debug)]
enum Callable {
    Function(FunctionKey),
    Closure(ClosureKey),
}

impl Callable {
    fn get_func(&self, objs: &VMObjects) -> FunctionKey {
        match self {
            Callable::Function(f) => *f,
            Callable::Closure(c) => {
                let cls = &objs.closures[*c];
                cls.func
            }
        }
    }

    fn get_closure(&self) -> ClosureKey {
        match self {
            Callable::Function(_) => unreachable!(),
            Callable::Closure(c) => *c,
        }
    }

    fn ret_count(&self, objs: &VMObjects) -> usize {
        let fkey = self.get_func(objs);
        objs.functions[fkey].ret_count
    }
}

#[derive(Clone, Debug)]
struct CallFrame {
    callable: Callable,
    pc: usize,
    stack_base: usize,
    ret_count: usize,
}

impl CallFrame {
    fn new_with_func(fkey: FunctionKey, sbase: usize) -> CallFrame {
        CallFrame {
            callable: Callable::Function(fkey),
            pc: 0,
            stack_base: sbase,
            ret_count: 0,
        }
    }

    fn new_with_closure(ckey: ClosureKey, sbase: usize) -> CallFrame {
        CallFrame {
            callable: Callable::Closure(ckey),
            pc: 0,
            stack_base: sbase,
            ret_count: 0,
        }
    }

    fn new_with_gos_value(val: &GosValue, sbase: usize) -> CallFrame {
        dbg!(val);
        match val {
            GosValue::Function(fkey) => CallFrame::new_with_func(fkey.clone(), sbase),
            GosValue::Closure(ckey) => CallFrame::new_with_closure(ckey.clone(), sbase),
            _ => unreachable!(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct Fiber {
    stack: Vec<GosValue>,
    frames: Vec<CallFrame>,
    caller: Option<Rc<RefCell<Fiber>>>,
    next_frame: Option<CallFrame>,
}

impl Fiber {
    fn new(caller: Option<Rc<RefCell<Fiber>>>) -> Fiber {
        Fiber {
            stack: Vec::new(),
            frames: Vec::new(),
            caller: caller,
            next_frame: None,
        }
    }

    fn run(&mut self, fkey: FunctionKey, objs: &mut VMObjects) {
        let frame = CallFrame::new_with_func(fkey, 0);
        self.frames.push(frame);
        self.main_loop(objs);
    }

    fn main_loop(&mut self, objs: &mut VMObjects) {
        let mut frame = self.frames.last_mut().unwrap();
        let fkey = frame.callable.get_func(objs);
        let func = &objs.functions[fkey];
        // allocate local variables
        for _ in 0..func.local_count() {
            self.stack.push(GosValue::Nil);
        }
        let mut consts = &func.consts;
        let mut code = &func.code;
        let mut stack_base = frame.stack_base;

        dbg!(code);

        loop {
            let instruction = code[frame.pc].unwrap_code();
            frame.pc += 1;
            dbg!(instruction);
            match instruction {
                Opcode::PUSH_CONST => {
                    let index = code[frame.pc].unwrap_data();
                    frame.pc += 1;
                    self.stack.push(consts[*index as usize].clone());
                }
                Opcode::PUSH_NIL => {
                    self.stack.push(GosValue::Nil);
                }
                Opcode::PUSH_FALSE => self.stack.push(GosValue::Bool(false)),
                Opcode::PUSH_TRUE => self.stack.push(GosValue::Bool(true)),
                Opcode::POP => {
                    self.stack.pop();
                }
                Opcode::LOAD_LOCAL0
                | Opcode::LOAD_LOCAL1
                | Opcode::LOAD_LOCAL2
                | Opcode::LOAD_LOCAL3
                | Opcode::LOAD_LOCAL4
                | Opcode::LOAD_LOCAL5
                | Opcode::LOAD_LOCAL6
                | Opcode::LOAD_LOCAL7
                | Opcode::LOAD_LOCAL8
                | Opcode::LOAD_LOCAL9
                | Opcode::LOAD_LOCAL10
                | Opcode::LOAD_LOCAL11
                | Opcode::LOAD_LOCAL12
                | Opcode::LOAD_LOCAL13
                | Opcode::LOAD_LOCAL14
                | Opcode::LOAD_LOCAL15 => {
                    let index = instruction.load_local_index();
                    self.stack.push(self.stack[stack_base + index as usize]);
                }
                Opcode::LOAD_LOCAL => {
                    let index = code[frame.pc].unwrap_data();
                    frame.pc += 1;
                    self.stack.push(self.stack[stack_base + *index as usize]);
                }
                Opcode::STORE_LOCAL => {
                    let index = code[frame.pc].unwrap_data();
                    frame.pc += 1;
                    let val = self.stack.last().unwrap().clone();
                    self.stack[stack_base + *index as usize] = val;
                }
                Opcode::LOAD_PKG_VAR => {
                    let index = code[frame.pc].unwrap_data();
                    frame.pc += 1;
                    let pkg = &objs.packages[func.package];
                    self.stack.push(pkg.members[*index as usize]);
                }
                Opcode::STORE_PKG_VAR => {
                    let index = code[frame.pc].unwrap_data();
                    frame.pc += 1;
                    let pkg = &mut objs.packages[func.package];
                    let val = self.stack.last().unwrap().clone();
                    pkg.members[*index as usize] = val;
                }
                Opcode::LOAD_UPVALUE => {
                    let index = code[frame.pc].unwrap_data();
                    frame.pc += 1;
                    let cls = frame.callable.get_closure();
                    let cls_val = &objs.closures[cls];
                    let uv = &cls_val.upvalues[*index as usize];
                    match uv {
                        UpValue::Open(level, ind) => {
                            drop(frame); // temporarily let go of the ownership
                            let frame_ind = self.frames.len() - (*level as usize) - 1 - 1;
                            let stack_ptr = self.frames[frame_ind].stack_base + (*ind as usize);
                            self.stack.push(self.stack[stack_ptr].clone());
                            frame = self.frames.last_mut().unwrap();
                        }
                        UpValue::Closed(key) => {
                            let val = &objs.boxed[*key];
                            self.stack.push(val.clone());
                        }
                    }
                }
                Opcode::STORE_UPVALUE => {}
                Opcode::PRE_CALL => {
                    let val = self.stack.last().unwrap();
                    dbg!(&self.stack);
                    let sbase = self.stack.len() - 1;
                    let frame = CallFrame::new_with_gos_value(val, sbase);
                    let ret_count = frame.callable.ret_count(objs);
                    self.next_frame = Some(frame);
                    self.stack.pop();
                    // placeholders for return values
                    for _ in 0..ret_count {
                        self.stack.push(GosValue::Nil)
                    }
                }
                Opcode::CALL => {
                    self.frames.push(self.next_frame.take().unwrap());
                    frame = self.frames.last_mut().unwrap();
                    stack_base = frame.stack_base;
                    let func = &objs.functions[frame.callable.get_func(objs)];
                    // for recovering the stack when it returns
                    frame.ret_count = func.ret_count;
                    // allocate local variables
                    for _ in 0..func.local_count() {
                        self.stack.push(GosValue::Nil);
                    }
                    consts = &func.consts;
                    code = &func.code;
                }
                Opcode::CALL_PRIMI_1_1 => {
                    unimplemented!();
                }
                Opcode::CALL_PRIMI_2_1 => {
                    let primi = Primitive::from(*code[frame.pc].unwrap_data() as OpIndex);
                    frame.pc += 1;
                    primi.call(&mut self.stack);
                }
                Opcode::RETURN => {
                    dbg!(&self.stack);
                    let garbage = self.stack.len() - (stack_base + frame.ret_count);
                    for _ in 0..garbage {
                        self.stack.pop();
                    }
                    self.frames.pop();
                    if self.frames.is_empty() {
                        break;
                    }
                    frame = self.frames.last_mut().unwrap();
                    stack_base = frame.stack_base;
                    let func = &objs.functions[frame.callable.get_func(objs)];
                    consts = &func.consts;
                    code = &func.code;
                }
                Opcode::NEW_CLOSURE => {
                    let fkey = self.stack.last().unwrap().get_function();
                    let func = &objs.functions[*fkey];
                    let cls = ClosureVal::new(*fkey, func.up_ptrs.clone());
                    let ckey = objs.closures.insert(cls);
                    self.stack.pop();
                    self.stack.push(GosValue::Closure(ckey));
                }
                _ => unimplemented!(),
            };
        }
    }
}

pub struct GosVM {
    fibers: Vec<Rc<RefCell<Fiber>>>,
    current_fiber: Option<Rc<RefCell<Fiber>>>,
    objects: VMObjects,
    packages: HashMap<String, PackageKey>,
}

impl GosVM {
    pub fn new(bc: ByteCode) -> GosVM {
        let mut vm = GosVM {
            fibers: Vec::new(),
            current_fiber: None,
            objects: bc.objects,
            packages: bc.packages,
        };
        let fb = Rc::new(RefCell::new(Fiber::new(None)));
        vm.fibers.push(fb.clone());
        vm.current_fiber = Some(fb);
        vm
    }

    pub fn run(&mut self, entry: FunctionKey) {
        let mut fb = self.current_fiber.as_ref().unwrap().borrow_mut();
        fb.run(entry, &mut self.objects);
    }
}
