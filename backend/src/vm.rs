#![allow(dead_code)]
use super::code_gen::ByteCode;
use super::opcode::*;
use super::types::*;
use super::vm_util;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

/// ClosureVal is a variable containing a pinter to a function and
/// a. a receiver, in which case, it is a bound-method
/// b. upvalues, in which case, it is a "real" closure
///
#[derive(Clone, Debug)]
pub struct ClosureVal {
    pub func: FunctionKey,
    pub receiver: Option<GosValue>,
    pub upvalues: Vec<UpValue>,
}

impl ClosureVal {
    pub fn new(key: FunctionKey, upvalues: Vec<UpValue>) -> ClosureVal {
        ClosureVal {
            func: key,
            receiver: None,
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
    fn func(&self, objs: &VMObjects) -> FunctionKey {
        match self {
            Callable::Function(f) => *f,
            Callable::Closure(c) => {
                let cls = &objs.closures[*c];
                cls.func
            }
        }
    }

    fn closure(&self) -> ClosureKey {
        match self {
            Callable::Function(_) => unreachable!(),
            Callable::Closure(c) => *c,
        }
    }

    fn ret_count(&self, objs: &VMObjects) -> usize {
        let fkey = self.func(objs);
        objs.functions[fkey].ret_count
    }

    fn receiver<'a>(&self, objs: &'a VMObjects) -> Option<&'a GosValue> {
        match self {
            Callable::Function(_) => None,
            Callable::Closure(c) => {
                let cls = &objs.closures[*c];
                cls.receiver.as_ref()
            }
        }
    }
}

#[derive(Clone, Debug)]
struct CallFrame {
    callable: Callable,
    pc: usize,
    stack_base: usize,
    // closures that have upvalues pointing to this frame
    referred_by: Option<HashMap<OpIndex, Vec<ClosureKey>>>,
}

impl CallFrame {
    fn with_func(fkey: FunctionKey, sbase: usize) -> CallFrame {
        CallFrame {
            callable: Callable::Function(fkey),
            pc: 0,
            stack_base: sbase,
            referred_by: None,
        }
    }

    fn with_closure(ckey: ClosureKey, sbase: usize) -> CallFrame {
        CallFrame {
            callable: Callable::Closure(ckey),
            pc: 0,
            stack_base: sbase,
            referred_by: None,
        }
    }

    fn with_gos_value(val: &GosValue, sbase: usize) -> CallFrame {
        match val {
            GosValue::Function(fkey) => CallFrame::with_func(fkey.clone(), sbase),
            GosValue::Closure(ckey) => CallFrame::with_closure(ckey.clone(), sbase),
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

    fn ret_count(&self, objs: &VMObjects) -> usize {
        self.callable.ret_count(objs)
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

    fn run(&mut self, fkey: FunctionKey, pkgs: &Vec<PackageKey>, objs: &mut VMObjects) {
        let frame = CallFrame::with_func(fkey, 0);
        self.frames.push(frame);
        self.main_loop(pkgs, objs);
    }

    fn main_loop(&mut self, pkgs: &Vec<PackageKey>, objs: &mut VMObjects) {
        let mut frame = self.frames.last_mut().unwrap();
        let fkey = frame.callable.func(objs);
        let mut func = &objs.functions[fkey];
        let stack = &mut self.stack;
        // allocate local variables
        for _ in 0..func.local_count() {
            stack.push(GosValue::Nil);
        }
        let mut consts = &func.consts;
        let mut code = &func.code;
        let mut stack_base = frame.stack_base;

        dbg!(code);
        loop {
            let inst = code[frame.pc].unwrap_code();
            frame.pc += 1;
            dbg!(inst);
            match inst {
                Opcode::PUSH_CONST => {
                    let index = read_index!(code, frame);
                    let gos_val = &consts[index as usize];
                    let val = match gos_val {
                        // Slice is a special case here because, this is stored slice literal,
                        // and when it gets "duplicated", the underlying rust vec is not copied
                        // which leads to all function calls shares the same vec instance
                        GosValue::Slice(s) => {
                            let slice = objs.slices[*s].deep_clone();
                            GosValue::Slice(objs.slices.insert(slice))
                        }
                        _ => gos_val.clone(),
                    };
                    stack.push(val);
                }
                Opcode::PUSH_NIL => {
                    stack.push(GosValue::Nil);
                }
                Opcode::PUSH_FALSE => stack.push(GosValue::Bool(false)),
                Opcode::PUSH_TRUE => stack.push(GosValue::Bool(true)),
                Opcode::PUSH_IMM => {
                    let index = read_index!(code, frame);
                    stack.push(GosValue::Int(index as isize));
                }
                Opcode::POP => {
                    stack.pop();
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
                    let index = inst.load_local_index();
                    stack.push(stack[offset_uint!(stack_base, index)]);
                }
                Opcode::LOAD_LOCAL => {
                    let index = offset_uint!(stack_base, read_index!(code, frame));
                    stack.push(stack[index]);
                }
                Opcode::STORE_LOCAL => {
                    let index = offset_uint!(stack_base, read_index!(code, frame));
                    stack[index] = duplicate!(stack[stack.len() - 1], objs);
                }
                Opcode::STORE_LOCAL_NT => {
                    let index = offset_uint!(stack_base, read_index!(code, frame));
                    let i = offset_uint!(stack.len(), read_index!(code, frame));
                    stack[index] = duplicate!(stack[i], objs);
                }
                Opcode::STORE_LOCAL_OP => {
                    let index = offset_uint!(stack_base, read_index!(code, frame));
                    let op = read_index!(code, frame);
                    let operand = stack[stack.len() - 1].clone();
                    vm_util::store_xxx_op(&mut stack[index], op, &operand);
                }
                Opcode::LOAD_UPVALUE => {
                    let index = read_index!(code, frame);
                    match &objs.closures[frame.callable.closure()].upvalues[index as usize] {
                        UpValue::Open(f, ind) => {
                            drop(frame);
                            let upframe = upframe!(self.frames.iter().rev().skip(1), objs, f);
                            let stack_ptr = offset_uint!(upframe.stack_base, *ind);
                            stack.push(stack[stack_ptr].clone());
                            frame = self.frames.last_mut().unwrap();
                        }
                        UpValue::Closed(key) => {
                            let val = &objs.boxed[*key];
                            stack.push(val.clone());
                        }
                    }
                }
                Opcode::STORE_UPVALUE | Opcode::STORE_UPVALUE_NT | Opcode::STORE_UPVALUE_OP => {
                    let index = read_index!(code, frame);
                    let offset = inst.offset(Opcode::STORE_UPVALUE);
                    let (op, val) = get_store_op_val!(stack, code, frame, consts, objs, offset);
                    let is_op_set = *inst == Opcode::STORE_UPVALUE_OP;
                    match &objs.closures[frame.callable.closure()].upvalues[index as usize] {
                        UpValue::Open(f, ind) => {
                            drop(frame);
                            let upframe = upframe!(self.frames.iter().rev().skip(1), objs, f);
                            let stack_ptr = offset_uint!(upframe.stack_base, *ind);
                            set_store_op_val!(&mut stack[stack_ptr], op, val, is_op_set);
                            frame = self.frames.last_mut().unwrap();
                        }
                        UpValue::Closed(key) => {
                            set_store_op_val!(&mut objs.boxed[*key], op, val, is_op_set);
                        }
                    }
                }
                Opcode::LOAD_FIELD | Opcode::LOAD_FIELD_IMM => {
                    let ind = if *inst == Opcode::LOAD_FIELD {
                        stack.pop().unwrap()
                    } else {
                        GosValue::Int(read_index!(code, frame) as isize)
                    };
                    let len = stack.len();
                    let val = try_unbox!(&stack[len - 1], &objs.boxed);
                    stack[len - 1] = match val {
                        GosValue::Slice(skey) => {
                            let slice = &objs.slices[*skey];
                            if let Some(v) = slice.get(*ind.as_int() as usize) {
                                v
                            } else {
                                unimplemented!();
                            }
                        }
                        GosValue::Map(mkey) => (&objs.maps[*mkey]).get(&ind).clone(),
                        GosValue::Struct(skey) => {
                            let sval = &objs.structs[*skey];
                            match ind {
                                GosValue::Int(i) => sval.fields[i as usize],
                                GosValue::Str(s) => {
                                    let str_val = &objs.strings[s];
                                    let index =
                                        sval.field_method_index(str_val.data_as_ref(), objs);
                                    if index < sval.fields.len() as OpIndex {
                                        sval.fields[index as usize]
                                    } else {
                                        bind_method!(sval, val, index, objs)
                                    }
                                }
                                _ => unreachable!(),
                            }
                        }
                        GosValue::Package(pkey) => {
                            let pkg = &objs.packages[*pkey];
                            *pkg.member(*ind.as_int() as OpIndex)
                        }
                        _ => unreachable!(),
                    };
                }
                Opcode::STORE_FIELD
                | Opcode::STORE_FIELD_NT
                | Opcode::STORE_FIELD_OP
                | Opcode::STORE_FIELD_IMM
                | Opcode::STORE_FIELD_IMM_NT
                | Opcode::STORE_FIELD_IMM_OP => {
                    let index = offset_uint!(stack.len(), read_index!(code, frame));
                    let store = try_unbox!(&stack[index], &objs.boxed);
                    let non_imm = inst.offset(Opcode::STORE_FIELD) <= 2;
                    let (key, offset, is_op_set) = if non_imm {
                        (
                            stack.get(index + 1).unwrap().clone(),
                            inst.offset(Opcode::STORE_FIELD),
                            *inst == Opcode::STORE_FIELD_OP,
                        )
                    } else {
                        (
                            GosValue::Int(read_index!(code, frame) as isize),
                            inst.offset(Opcode::STORE_FIELD_IMM),
                            *inst == Opcode::STORE_FIELD_IMM_OP,
                        )
                    };
                    let (op, val) = get_store_op_val!(stack, code, frame, consts, objs, offset);
                    match store {
                        GosValue::Slice(s) => {
                            let target =
                                &mut objs.slices[*s].borrow_data_mut()[*key.as_int() as usize];
                            set_store_op_val!(target, op, val, is_op_set);
                        }
                        GosValue::Map(m) => {
                            let target = objs.maps[*m].get_mut(&key);
                            set_store_op_val!(target, op, val, is_op_set);
                        }
                        GosValue::Struct(s) => {
                            match key {
                                GosValue::Int(i) => {
                                    let target = &mut objs.structs[*s].fields[i as usize];
                                    set_store_op_val!(target, op, val, is_op_set);
                                }
                                GosValue::Str(skey) => {
                                    let str_val = &objs.strings[skey];
                                    let struct_val = &objs.structs[*s];
                                    let i =
                                        struct_val.field_method_index(str_val.data_as_ref(), objs);
                                    let target = &mut objs.structs[*s].fields[i as usize];
                                    set_store_op_val!(target, op, val, is_op_set);
                                }
                                _ => unreachable!(),
                            };
                        }
                        _ => unreachable!(),
                    }
                }
                Opcode::LOAD_THIS_PKG_FIELD => {
                    let index = read_index!(code, frame);
                    let pkg = &objs.packages[func.package];
                    stack.push(*pkg.member(index));
                }
                Opcode::STORE_THIS_PKG_FIELD => {
                    let index = read_index!(code, frame);
                    let pkg = &mut objs.packages[func.package];
                    *pkg.member_mut(index) = duplicate!(stack[stack.len() - 1], objs);
                }
                Opcode::STORE_THIS_PKG_FIELD_NT => {
                    let index = read_index!(code, frame);
                    let i = offset_uint!(stack.len(), read_index!(code, frame));
                    let pkg = &mut objs.packages[func.package];
                    *pkg.member_mut(index) = duplicate!(stack[i], objs);
                }
                Opcode::STORE_THIS_PKG_FIELD_OP => {
                    let index = read_index!(code, frame);
                    let op = read_index!(code, frame);
                    let operand = &consts[read_index!(code, frame) as usize];
                    let pkg = &mut objs.packages[func.package];
                    vm_util::store_xxx_op(pkg.member_mut(index), op, operand);
                }
                Opcode::STORE_DEREF => {
                    let index = offset_uint!(stack.len(), read_index!(code, frame));
                    let store = stack.get(index).unwrap();
                    let i = stack.len() - 1;
                    objs.boxed[*store.as_boxed()] = duplicate!(stack[i], objs);
                }
                Opcode::STORE_DEREF_NT => {
                    let index = offset_uint!(stack.len(), read_index!(code, frame));
                    let store = stack.get(index).unwrap();
                    let i = offset_uint!(stack.len(), read_index!(code, frame));
                    objs.boxed[*store.as_boxed()] = duplicate!(stack[i], objs);
                }
                Opcode::STORE_DEREF_OP => {
                    let index = offset_uint!(stack.len(), read_index!(code, frame));
                    let op = read_index!(code, frame);
                    let operand = &consts[read_index!(code, frame) as usize];
                    let store = stack.get(index).unwrap();
                    vm_util::store_xxx_op(&mut objs.boxed[*store.as_boxed()], op, operand);
                }

                Opcode::ADD => vm_util::add(stack, &mut objs.strings),
                Opcode::SUB => vm_util::sub(stack),
                Opcode::MUL => vm_util::mul(stack),
                Opcode::QUO => vm_util::quo(stack),
                Opcode::REM => vm_util::rem(stack),
                Opcode::AND => vm_util::and(stack),
                Opcode::OR => vm_util::or(stack),
                Opcode::XOR => vm_util::xor(stack),
                Opcode::AND_NOT => vm_util::and_not(stack),
                Opcode::SHL => vm_util::shl(stack),
                Opcode::SHR => vm_util::shr(stack),
                Opcode::UNARY_ADD => vm_util::unary_and(stack),
                Opcode::UNARY_SUB => vm_util::unary_sub(stack),
                Opcode::UNARY_XOR => vm_util::unary_xor(stack),
                Opcode::REF => vm_util::unary_ref(stack, &mut objs.boxed),
                Opcode::DEREF => vm_util::unary_deref(stack, &objs.boxed),
                Opcode::ARROW => unimplemented!(),
                Opcode::LAND => vm_util::logical_and(stack),
                Opcode::LOR => vm_util::logical_or(stack),
                Opcode::LNOT => vm_util::logical_not(stack),
                Opcode::EQL => vm_util::compare_eql(stack, &objs),
                Opcode::LSS => vm_util::compare_lss(stack, &objs),
                Opcode::GTR => vm_util::compare_gtr(stack, &objs),
                Opcode::NEQ => vm_util::compare_neq(stack, &objs),
                Opcode::LEQ => vm_util::compare_leq(stack, &objs),
                Opcode::GEQ => vm_util::compare_geq(stack, &objs),

                Opcode::PRE_CALL => {
                    let val = stack.pop().unwrap();
                    let sbase = stack.len();
                    let next_frame = CallFrame::with_gos_value(&val, sbase);
                    let func_key = next_frame.callable.func(objs);
                    let next_func = &objs.functions[func_key];
                    // init return values
                    if next_func.ret_count > 0 {
                        let type_key = next_func.typ;
                        let (_, rs) = &objs.types[type_key].closure_type_data();
                        let mut returns = rs
                            .iter()
                            .map(|x| x.get_type_val(&objs).zero_val.clone())
                            .collect();
                        stack.append(&mut returns);
                    }
                    // push receiver on stack as the first parameter
                    let receiver = next_frame.callable.receiver(&objs).map(|x| x.clone());
                    if let Some(r) = receiver {
                        stack.push(duplicate!(r, objs));
                    }
                    self.next_frame = Some(next_frame);
                }
                Opcode::CALL | Opcode::CALL_ELLIPSIS => {
                    self.frames.push(self.next_frame.take().unwrap());
                    frame = self.frames.last_mut().unwrap();
                    func = &objs.functions[frame.callable.func(objs)];
                    stack_base = frame.stack_base;
                    consts = &func.consts;
                    code = &func.code;
                    dbg!(&consts);
                    dbg!(&code);
                    dbg!(&stack);

                    if func.variadic() && *inst != Opcode::CALL_ELLIPSIS {
                        let index = stack_base
                            + func.param_count
                            + func.ret_count
                            + if frame.callable.receiver(&objs).is_some() {
                                1
                            } else {
                                0
                            }
                            - 1;
                        pack_variadic!(stack, index, objs);
                    }

                    // allocate placeholders for local variables
                    for _ in 0..func.local_count() {
                        stack.push(GosValue::Nil);
                    }
                }
                Opcode::RETURN | Opcode::RETURN_INIT_PKG => {
                    // first handle upvalues in 2 steps:
                    // 1. clean up any referred_by created by this frame
                    match frame.callable {
                        Callable::Closure(c) => {
                            let cls_val = &objs.closures[c];
                            for uv in cls_val.upvalues.iter() {
                                match uv {
                                    UpValue::Open(f, ind) => {
                                        drop(frame);
                                        let upframe =
                                            upframe!(self.frames.iter_mut().rev().skip(1), objs, f);
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
                            let val = stack[stack_base + *ind as usize].clone();
                            let bkey = objs.boxed.insert(val);
                            let func = frame.callable.func(objs);
                            for r in referrers.iter() {
                                let cls_val = &mut objs.closures[*r];
                                cls_val.close_upvalue(func, *ind, bkey);
                            }
                        }
                    }

                    dbg!(stack.len());
                    for s in stack.iter() {
                        dbg!(GosValueDebug::new(&s, &objs));
                    }

                    match *inst {
                        Opcode::RETURN => {
                            stack.truncate(stack_base + frame.ret_count(objs));
                        }
                        Opcode::RETURN_INIT_PKG => {
                            let index = read_index!(code, frame) as usize;
                            let pkey = pkgs[index];
                            let pkg = &mut objs.packages[pkey];
                            let count = pkg.var_count();
                            // remove garbage first
                            stack.truncate(stack_base + count);
                            // the var values left on the stack are for pkg members
                            for i in 0..count {
                                let val = stack.pop().unwrap();
                                let index = (count - 1 - i) as OpIndex;
                                pkg.init_var(&index, val);
                            }
                            // the one pushed by IMPORT was poped by LOAD_FIELD
                            stack.push(GosValue::Package(pkey));
                        }
                        _ => unreachable!(),
                    }

                    self.frames.pop();
                    if self.frames.is_empty() {
                        break;
                    }
                    frame = self.frames.last_mut().unwrap();
                    stack_base = frame.stack_base;
                    // restore func, consts, code
                    func = &objs.functions[frame.callable.func(objs)];
                    consts = &func.consts;
                    code = &func.code;
                }

                Opcode::JUMP => {
                    frame.pc = offset_uint!(frame.pc, read_index!(code, frame));
                }
                Opcode::JUMP_IF_NOT => {
                    let offset = read_index!(code, frame);
                    let val = stack.last().unwrap();
                    if !*val.as_bool() {
                        frame.pc = offset_uint!(frame.pc, offset);
                    }
                    stack.pop();
                }

                Opcode::IMPORT => {
                    let index = read_index!(code, frame) as usize;
                    let pkey = pkgs[index];
                    stack.push(GosValue::Package(pkey));
                    stack.push(GosValue::Bool(!objs.packages[pkey].inited()));
                }
                Opcode::NEW => {
                    let new_val = match stack.pop().unwrap() {
                        GosValue::Function(fkey) => {
                            // NEW a closure
                            let func = &objs.functions[fkey];
                            let ckey = objs
                                .closures
                                .insert(ClosureVal::new(fkey, func.up_ptrs.clone()));
                            // set referred_by for the frames down in the stack
                            for uv in func.up_ptrs.iter() {
                                match uv {
                                    UpValue::Open(func, ind) => {
                                        drop(frame);
                                        let upframe =
                                            upframe!(self.frames.iter_mut().rev(), objs, func);
                                        upframe.add_referred_by(*ind, ckey.clone());
                                        frame = self.frames.last_mut().unwrap();
                                    }
                                    UpValue::Closed(_) => unreachable!(),
                                }
                            }
                            GosValue::Closure(ckey)
                        }
                        _ => unimplemented!(),
                    };
                    stack.push(new_val);
                }
                Opcode::MAKE => unimplemented!(),
                Opcode::LEN => match stack.pop().unwrap() {
                    GosValue::Slice(skey) => {
                        stack.push(GosValue::Int(objs.slices[skey].len() as isize));
                    }
                    GosValue::Map(mkey) => {
                        stack.push(GosValue::Int(objs.maps[mkey].len() as isize));
                    }
                    _ => unreachable!(),
                },
                Opcode::CAP => match stack.pop().unwrap() {
                    GosValue::Slice(skey) => {
                        stack.push(GosValue::Int(objs.slices[skey].cap() as isize));
                    }
                    _ => unreachable!(),
                },
                Opcode::APPEND => {
                    let index = offset_uint!(stack.len(), read_index!(code, frame));
                    pack_variadic!(stack, index, objs);
                    let b = stack.pop().unwrap();
                    let a = &stack[stack.len() - 1];
                    let vala = &objs.slices[*a.as_slice()];
                    let valb = &objs.slices[*b.as_slice()];
                    vala.borrow_data_mut()
                        .append(&mut valb.borrow_data().clone());
                }
                Opcode::ASSERT => match stack.pop().unwrap() {
                    GosValue::Bool(b) => assert!(b, "Opcode::ASSERT: not true!"),
                    _ => assert!(false, "Opcode::ASSERT: not bool!"),
                },
                _ => unimplemented!(),
            };
        }
    }
}

pub struct GosVM {
    fibers: Vec<Rc<RefCell<Fiber>>>,
    current_fiber: Option<Rc<RefCell<Fiber>>>,
    objects: VMObjects,
    packages: Vec<PackageKey>,
    entry: FunctionKey,
}

impl GosVM {
    pub fn new(bc: ByteCode) -> GosVM {
        let mut vm = GosVM {
            fibers: Vec::new(),
            current_fiber: None,
            objects: bc.objects,
            packages: bc.packages,
            entry: bc.entry,
        };
        let fb = Rc::new(RefCell::new(Fiber::new(None)));
        vm.fibers.push(fb.clone());
        vm.current_fiber = Some(fb);
        vm
    }

    pub fn run(&mut self) {
        let mut fb = self.current_fiber.as_ref().unwrap().borrow_mut();
        fb.run(self.entry, &self.packages, &mut self.objects);
    }
}
