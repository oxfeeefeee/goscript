#![allow(dead_code)]
use super::codegen::ByteCode;
use super::opcode::*;
use super::prim_ops::PrimOps;
use super::types::Objects as VMObjects;
use super::types::*;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

macro_rules! get_value {
    ($vm:ident, $code:ident, $frame:ident, $instruction:ident, $op:path) => {
        if let $op = $instruction {
            $vm.stack.last().unwrap().clone()
        } else {
            let val_ind = *$code[$frame.pc].unwrap_data() as i16;
            $frame.pc += 1;
            let index = ($vm.stack.len() as i16 + val_ind - 1) as usize;
            $vm.stack[index].clone()
        }
    };
}

macro_rules! get_upframe {
    ($iter:expr, $objs:ident, $f:ident) => {
        $iter.find(|x| x.callable.get_func($objs) == *$f).unwrap();
    };
}

#[derive(Clone, Debug)]
pub struct ClosureVal {
    pub func: FunctionKey,
    pub upvalues: Vec<UpValue>,
}

impl ClosureVal {
    pub fn new(key: FunctionKey, upvalues: Vec<UpValue>) -> ClosureVal {
        ClosureVal {
            func: key,
            upvalues: upvalues,
        }
    }

    pub fn close_upvalue(&mut self, func: FunctionKey, index: OpIndex, boxed: BoxedKey) {
        for i in 0..self.upvalues.len() {
            if self.upvalues[i] == UpValue::Open(func, index) {
                self.upvalues[i] = UpValue::Closed(boxed);
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum UpValue {
    /// Parent CallFrame is still alive, pointing to a local variable
    Open(FunctionKey, OpIndex), // (what func is the var defined, the index of the var)
    // Parent CallFrame is released, pointing to a Boxed value in the global pool
    Closed(BoxedKey),
}

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
    // closures that have upvalues pointing to this frame
    referred_by: Option<HashMap<OpIndex, Vec<ClosureKey>>>,
}

impl CallFrame {
    fn new_with_func(fkey: FunctionKey, sbase: usize) -> CallFrame {
        CallFrame {
            callable: Callable::Function(fkey),
            pc: 0,
            stack_base: sbase,
            ret_count: 0,
            referred_by: None,
        }
    }

    fn new_with_closure(ckey: ClosureKey, sbase: usize) -> CallFrame {
        CallFrame {
            callable: Callable::Closure(ckey),
            pc: 0,
            stack_base: sbase,
            ret_count: 0,
            referred_by: None,
        }
    }

    fn new_with_gos_value(val: &GosValue, sbase: usize) -> CallFrame {
        match val {
            GosValue::Function(fkey) => CallFrame::new_with_func(fkey.clone(), sbase),
            GosValue::Closure(ckey) => CallFrame::new_with_closure(ckey.clone(), sbase),
            _ => unreachable!(),
        }
    }

    fn add_referred_by(&mut self, index: OpIndex, ckey: ClosureKey) {
        if self.referred_by.is_none() {
            self.referred_by = Some(HashMap::new());
        }
        let map = self.referred_by.as_mut().unwrap();
        match map.get_mut(&index) {
            Some(v) => {
                v.push(ckey);
            }
            None => {
                map.insert(index, vec![ckey]);
            }
        }
    }

    fn remove_referred_by(&mut self, index: OpIndex, ckey: ClosureKey) {
        let map = self.referred_by.as_mut().unwrap();
        let v = map.get_mut(&index).unwrap();
        for i in 0..v.len() {
            if v[i] == ckey {
                v.swap_remove(i);
                break;
            }
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
                Opcode::STORE_LOCAL | Opcode::STORE_LOCAL_NON_TOP => {
                    let index = code[frame.pc].unwrap_data();
                    frame.pc += 1;
                    let val = get_value!(self, code, frame, instruction, Opcode::STORE_LOCAL);
                    self.stack[stack_base + *index as usize] = val;
                }
                Opcode::LOAD_PKG_VAR => {
                    let index = code[frame.pc].unwrap_data();
                    frame.pc += 1;
                    let pkg = &objs.packages[func.package];
                    self.stack.push(pkg.members[*index as usize]);
                }
                Opcode::STORE_PKG_VAR | Opcode::STORE_PKG_VAR_NON_TOP => {
                    let index = code[frame.pc].unwrap_data();
                    frame.pc += 1;
                    let pkg = &mut objs.packages[func.package];
                    let val = get_value!(self, code, frame, instruction, Opcode::STORE_PKG_VAR);
                    pkg.members[*index as usize] = val;
                }
                Opcode::LOAD_UPVALUE => {
                    let index = code[frame.pc].unwrap_data();
                    frame.pc += 1;
                    match &objs.closures[frame.callable.get_closure()].upvalues[*index as usize] {
                        UpValue::Open(f, ind) => {
                            drop(frame); // temporarily let go of the ownership
                            let upframe = get_upframe!(self.frames.iter().rev().skip(1), objs, f);
                            let stack_ptr = upframe.stack_base + (*ind as usize);
                            self.stack.push(self.stack[stack_ptr].clone());
                            frame = self.frames.last_mut().unwrap();
                        }
                        UpValue::Closed(key) => {
                            let val = &objs.boxed[*key];
                            dbg!(&val);
                            self.stack.push(val.clone());
                        }
                    }
                }
                Opcode::STORE_UPVALUE | Opcode::STORE_UPVALUE_NON_TOP => {
                    let index = code[frame.pc].unwrap_data();
                    frame.pc += 1;
                    let val = get_value!(self, code, frame, instruction, Opcode::STORE_UPVALUE);
                    match &objs.closures[frame.callable.get_closure()].upvalues[*index as usize] {
                        UpValue::Open(f, ind) => {
                            drop(frame); // temporarily let go of the ownership
                            let upframe = get_upframe!(self.frames.iter().rev().skip(1), objs, f);
                            let stack_ptr = upframe.stack_base + (*ind as usize);
                            self.stack[stack_ptr] = val;
                            frame = self.frames.last_mut().unwrap();
                        }
                        UpValue::Closed(key) => {
                            objs.boxed[*key] = val;
                        }
                    }
                }
                Opcode::PRE_CALL => {
                    let val = self.stack.last().unwrap();
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
                    dbg!(&code);
                }
                Opcode::CALL_PRIM_1_1 => {
                    unimplemented!();
                }
                Opcode::CALL_PRIM_2_1 => {
                    let prim = PrimOps::from(*code[frame.pc].unwrap_data() as OpIndex);
                    frame.pc += 1;
                    prim.call(&mut self.stack, objs);
                }
                Opcode::RETURN => {
                    // first handle upvalues in 3 steps:
                    // 1. clean up any referred_by created by this frame
                    match frame.callable {
                        Callable::Closure(c) => {
                            let cls_val = &objs.closures[c];
                            for uv in cls_val.upvalues.iter() {
                                match uv {
                                    UpValue::Open(f, ind) => {
                                        drop(frame); // temporarily let go of the ownership
                                        let upframe = get_upframe!(
                                            self.frames.iter_mut().rev().skip(1),
                                            objs,
                                            f
                                        );
                                        upframe.remove_referred_by(*ind, c.clone());
                                        frame = self.frames.last_mut().unwrap();
                                    }
                                    // Do nothing for closed ones for now, Will be delt with it by GC.
                                    UpValue::Closed(_) => {}
                                }
                            }
                        }
                        Callable::Function(_) => {}
                    }
                    // 2. close any active upvalue this frame contains
                    if let Some(referred) = &frame.referred_by {
                        for (ind, referrers) in referred {
                            if referrers.len() == 0 {
                                continue;
                            }
                            let val = self.stack[stack_base + *ind as usize].clone();
                            let bkey = objs.boxed.insert(val);
                            let func = frame.callable.get_func(objs);
                            for r in referrers.iter() {
                                let cls_val = &mut objs.closures[*r];
                                cls_val.close_upvalue(func, *ind, bkey);
                            }
                        }
                    }

                    let ret_count = frame.ret_count;
                    drop(frame);
                    self.frames.pop();
                    if self.frames.is_empty() {
                        dbg!(&self.stack);
                        break;
                    }
                    let garbage = self.stack.len() - (stack_base + ret_count);
                    for _ in 0..garbage {
                        self.stack.pop();
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
                    let ckey = objs
                        .closures
                        .insert(ClosureVal::new(*fkey, func.up_ptrs.clone()));
                    // set referred_by for the frames down in the stack
                    for uv in func.up_ptrs.iter() {
                        match uv {
                            UpValue::Open(func, ind) => {
                                drop(frame); // temporarily let go of the ownership
                                let upframe =
                                    get_upframe!(self.frames.iter_mut().rev(), objs, func);
                                upframe.add_referred_by(*ind, ckey.clone());
                                frame = self.frames.last_mut().unwrap();
                            }
                            UpValue::Closed(_) => unreachable!(),
                        }
                    }
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
