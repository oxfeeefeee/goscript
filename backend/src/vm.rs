#![allow(dead_code)]
use super::code_gen::ByteCode;
use super::opcode::*;
use super::prim_ops;
use super::types::Objects as VMObjects;
use super::types::*;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

macro_rules! rhs_val_index {
    ($stack:ident, $code:ident, $frame:ident, $instruction:ident, $op:path) => {
        if let $op = $instruction {
            $stack.len() - 1
        } else {
            let val_ind = *$code[$frame.pc].unwrap_data() as i16;
            $frame.pc += 1;
            ($stack.len() as i16 + val_ind) as usize
        }
    };
}

macro_rules! upframe {
    ($iter:expr, $objs:ident, $f:ident) => {
        $iter.find(|x| x.callable.func($objs) == *$f).unwrap();
    };
}

macro_rules! try_unbox {
    ($val:expr, $box_objs:expr) => {
        if let GosValue::Boxed(bkey) = $val {
            &$box_objs[*bkey]
        } else {
            $val
        }
    };
}

macro_rules! bind_method {
    ($sval:ident, $val:ident, $index:ident, $objs:ident) => {
        GosValue::Closure(
            $objs.closures.insert(ClosureVal {
                func: *$objs.types[$sval.typ]
                    .get_struct_member($index)
                    .as_function(),
                receiver: Some($val.clone()),
                upvalues: vec![],
            }),
        )
    };
}

macro_rules! pack_variadic {
    ($index:ident, $stack:ident, $objs:ident) => {
        if $index < $stack.len() {
            let mut v = Vec::new();
            v.append(&mut $stack.split_off($index));
            $stack.push(GosValue::with_slice_val(v, &mut $objs.slices))
        }
    };
}

macro_rules! offset_uint {
    ($uint:expr, $offset:expr) => {
        ($uint as isize + $offset as isize) as usize
    };
}

/// Duplicates the GosValue, primitive types and read-only types are simply cloned
macro_rules! duplicate {
    ($val:expr, $objs:ident) => {
        match &$val {
            GosValue::Nil
            | GosValue::Bool(_)
            | GosValue::Int(_)
            | GosValue::Float64(_)
            | GosValue::Complex64(_, _)
            | GosValue::Str(_)
            | GosValue::Boxed(_)
            | GosValue::Closure(_) => $val.clone(),
            GosValue::Slice(k) => GosValue::Slice($objs.slices.insert($objs.slices[*k].clone())),
            GosValue::Map(k) => GosValue::Map($objs.maps.insert($objs.maps[*k].clone())),
            GosValue::Interface(_) => unimplemented!(),
            GosValue::Struct(k) => {
                GosValue::Struct($objs.structs.insert($objs.structs[*k].clone()))
            }
            GosValue::Channel(_) => unimplemented!(),
            GosValue::Function(_) => $val.clone(),
            GosValue::Package(_) => $val.clone(),
            GosValue::Type(_) => $val.clone(),
        }
    };
}

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
            let instruction = code[frame.pc].unwrap_code();
            frame.pc += 1;
            dbg!(instruction);
            match instruction {
                Opcode::PUSH_CONST => {
                    let index = code[frame.pc].unwrap_data();
                    frame.pc += 1;
                    let gos_val = &consts[*index as usize];
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
                    let short = code[frame.pc].unwrap_data();
                    frame.pc += 1;
                    stack.push(GosValue::Int(*short as isize));
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
                    let index = instruction.load_local_index();
                    stack.push(stack[stack_base + index as usize]);
                }
                Opcode::LOAD_LOCAL => {
                    let index = code[frame.pc].unwrap_data();
                    frame.pc += 1;
                    stack.push(stack[stack_base + *index as usize]);
                }
                Opcode::STORE_LOCAL | Opcode::STORE_LOCAL_NT => {
                    let index = code[frame.pc].unwrap_data();
                    frame.pc += 1;
                    let i = rhs_val_index!(stack, code, frame, instruction, Opcode::STORE_LOCAL);
                    stack[stack_base + *index as usize] = duplicate!(stack[i], objs);
                }
                Opcode::LOAD_UPVALUE => {
                    let index = code[frame.pc].unwrap_data();
                    frame.pc += 1;
                    match &objs.closures[frame.callable.closure()].upvalues[*index as usize] {
                        UpValue::Open(f, ind) => {
                            drop(frame);
                            let upframe = upframe!(self.frames.iter().rev().skip(1), objs, f);
                            let stack_ptr = upframe.stack_base + (*ind as usize);
                            stack.push(stack[stack_ptr].clone());
                            frame = self.frames.last_mut().unwrap();
                        }
                        UpValue::Closed(key) => {
                            let val = &objs.boxed[*key];
                            stack.push(val.clone());
                        }
                    }
                }
                Opcode::STORE_UPVALUE | Opcode::STORE_UPVALUE_NT => {
                    let index = code[frame.pc].unwrap_data();
                    frame.pc += 1;
                    let i = rhs_val_index!(stack, code, frame, instruction, Opcode::STORE_UPVALUE);
                    let val = duplicate!(stack[i], objs);
                    match &objs.closures[frame.callable.closure()].upvalues[*index as usize] {
                        UpValue::Open(f, ind) => {
                            drop(frame);
                            let upframe = upframe!(self.frames.iter().rev().skip(1), objs, f);
                            let stack_ptr = upframe.stack_base + (*ind as usize);
                            stack[stack_ptr] = val;
                            frame = self.frames.last_mut().unwrap();
                        }
                        UpValue::Closed(key) => {
                            objs.boxed[*key] = val;
                        }
                    }
                }
                Opcode::LOAD_FIELD => {
                    let ind = stack.pop().unwrap();
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
                            pkg.member(*ind.as_int() as OpIndex)
                        }
                        _ => unreachable!(),
                    };
                }
                Opcode::LOAD_FIELD_IMM => {
                    let len = stack.len();
                    let val = try_unbox!(&stack[len - 1], &objs.boxed);
                    let index = *code[frame.pc].unwrap_data();
                    frame.pc += 1;
                    stack[len - 1] = match val {
                        GosValue::Slice(skey) => {
                            let slice = &objs.slices[*skey];
                            if let Some(v) = slice.get(index as usize) {
                                v
                            } else {
                                unimplemented!();
                            }
                        }
                        GosValue::Map(mkey) => (&objs.maps[*mkey])
                            .get(&GosValue::Int(index as isize))
                            .clone(),
                        GosValue::Struct(skey) => {
                            let sval = &objs.structs[*skey];
                            if index < sval.fields.len() as OpIndex {
                                sval.fields[index as usize]
                            } else {
                                bind_method!(sval, val, index, objs)
                            }
                        }
                        GosValue::Package(pkey) => {
                            let pkg = &objs.packages[*pkey];
                            pkg.member(index as OpIndex)
                        }
                        _ => unreachable!(),
                    };
                }
                Opcode::STORE_FIELD | Opcode::STORE_FIELD_NT => {
                    let lhs_index = code[frame.pc].unwrap_data();
                    frame.pc += 1;
                    let index = offset_uint!(stack.len(), *lhs_index);
                    let store = try_unbox!(stack.get(index).unwrap(), &objs.boxed);
                    let key = stack.get(index + 1).unwrap();
                    let i = rhs_val_index!(stack, code, frame, instruction, Opcode::STORE_FIELD);
                    let val = duplicate!(stack[i], objs);
                    match store {
                        GosValue::Slice(s) => {
                            objs.slices[*s].set(*key.as_int() as usize, val);
                        }
                        GosValue::Map(m) => {
                            objs.maps[*m].insert(key.clone(), val.clone());
                        }
                        GosValue::Struct(s) => {
                            match key {
                                GosValue::Int(i) => {
                                    objs.structs[*s].fields[*i as usize] = val.clone()
                                }
                                GosValue::Str(skey) => {
                                    let str_val = &objs.strings[*skey];
                                    let struct_val = &objs.structs[*s];
                                    let i =
                                        struct_val.field_method_index(str_val.data_as_ref(), objs);
                                    objs.structs[*s].fields[i as usize] = val.clone();
                                }
                                _ => unreachable!(),
                            };
                        }
                        _ => unreachable!(),
                    }
                }
                Opcode::STORE_FIELD_IMM | Opcode::STORE_FIELD_IMM_NT => {
                    let lhs_index = code[frame.pc].unwrap_data();
                    frame.pc += 1;
                    let index = (stack.len() as i16 + lhs_index) as usize;
                    let store = try_unbox!(stack.get(index).unwrap(), &objs.boxed);
                    let key = code[frame.pc].unwrap_data();
                    frame.pc += 1;
                    let i =
                        rhs_val_index!(stack, code, frame, instruction, Opcode::STORE_FIELD_IMM);
                    let val = duplicate!(stack[i], objs);
                    match store {
                        GosValue::Slice(s) => {
                            objs.slices[*s].set(*key as usize, val);
                        }
                        GosValue::Map(m) => {
                            objs.maps[*m].insert(GosValue::Int(*key as isize), val.clone());
                        }
                        GosValue::Struct(s) => {
                            objs.structs[*s].fields[index] = val.clone();
                        }
                        _ => unreachable!(),
                    }
                }
                Opcode::LOAD_THIS_PKG_FIELD => {
                    let index = code[frame.pc].unwrap_data();
                    frame.pc += 1;
                    let pkg = &objs.packages[func.package];
                    stack.push(pkg.member(*index));
                }
                Opcode::STORE_THIS_PKG_FIELD | Opcode::STORE_THIS_PKG_FIELD_NT => {
                    let index = code[frame.pc].unwrap_data();
                    frame.pc += 1;
                    let i = rhs_val_index!(
                        stack,
                        code,
                        frame,
                        instruction,
                        Opcode::STORE_THIS_PKG_FIELD
                    );
                    let pkg = &mut objs.packages[func.package];
                    pkg.set_member(*index, duplicate!(stack[i], objs));
                }
                Opcode::STORE_DEREF | Opcode::STORE_DEREF_NT => {
                    let lhs_index = code[frame.pc].unwrap_data();
                    frame.pc += 1;
                    let index = (stack.len() as i16 + lhs_index) as usize;
                    let store = stack.get(index).unwrap();
                    let i = rhs_val_index!(stack, code, frame, instruction, Opcode::STORE_DEREF);
                    objs.boxed[*store.as_boxed()] = duplicate!(stack[i], objs);
                }

                Opcode::ADD => prim_ops::add(stack, &mut objs.strings),
                Opcode::SUB => prim_ops::sub(stack),
                Opcode::MUL => prim_ops::mul(stack),
                Opcode::QUO => prim_ops::quo(stack),
                Opcode::REM => prim_ops::rem(stack),
                Opcode::AND => prim_ops::and(stack),
                Opcode::OR => prim_ops::or(stack),
                Opcode::XOR => prim_ops::xor(stack),
                Opcode::AND_NOT => prim_ops::and_not(stack),
                Opcode::SHL => prim_ops::shl(stack),
                Opcode::SHR => prim_ops::shr(stack),
                Opcode::UNARY_ADD => prim_ops::unary_and(stack),
                Opcode::UNARY_SUB => prim_ops::unary_sub(stack),
                Opcode::UNARY_XOR => prim_ops::unary_xor(stack),
                Opcode::REF => prim_ops::unary_ref(stack, &mut objs.boxed),
                Opcode::DEREF => prim_ops::unary_deref(stack, &objs.boxed),
                Opcode::ARROW => unimplemented!(),
                Opcode::LAND => prim_ops::logical_and(stack),
                Opcode::LOR => prim_ops::logical_or(stack),
                Opcode::LNOT => prim_ops::logical_not(stack),
                Opcode::EQL => prim_ops::compare_eql(stack),
                Opcode::LSS => prim_ops::compare_lss(stack),
                Opcode::GTR => prim_ops::compare_gtr(stack),
                Opcode::NEQ => prim_ops::compare_neq(stack),
                Opcode::LEQ => prim_ops::compare_leq(stack),
                Opcode::GEQ => prim_ops::compare_geq(stack),

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

                    if func.variadic() && *instruction != Opcode::CALL_ELLIPSIS {
                        let index = stack_base
                            + func.param_count
                            + func.ret_count
                            + if frame.callable.receiver(&objs).is_some() {
                                1
                            } else {
                                0
                            }
                            - 1;
                        pack_variadic!(index, stack, objs);
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

                    match *instruction {
                        Opcode::RETURN => {
                            stack.truncate(stack_base + frame.ret_count(objs));
                        }
                        Opcode::RETURN_INIT_PKG => {
                            let index = *code[frame.pc].unwrap_data() as usize;
                            frame.pc += 1;
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

                Opcode::JUMP => unimplemented!(),
                Opcode::JUMP_IF => {
                    let val = stack.last().unwrap();
                    if *val.as_bool() {
                        let offset = code[frame.pc].unwrap_data();
                        frame.pc += 1;
                        frame.pc = offset_uint!(frame.pc, *offset);
                    } else {
                        frame.pc += 1;
                    }
                    stack.pop();
                }

                Opcode::IMPORT => {
                    let index = *code[frame.pc].unwrap_data() as usize;
                    frame.pc += 1;
                    let pkey = pkgs[index];
                    stack.push(GosValue::Package(pkey));
                    stack.push(GosValue::Bool(objs.packages[pkey].inited()));
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
                    let index = offset_uint!(stack.len(), *code[frame.pc].unwrap_data());
                    frame.pc += 1;
                    pack_variadic!(index, stack, objs);
                    let b = stack.pop().unwrap();
                    let a = &stack[stack.len() - 1];
                    let vala = &objs.slices[*a.as_slice()];
                    let valb = &objs.slices[*b.as_slice()];
                    vala.borrow_data_mut()
                        .append(&mut valb.borrow_data().clone());
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
