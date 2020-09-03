#![allow(dead_code)]
use super::instruction::*;
use super::objects::{
    ClosureVal, GosHashMap, MetadataType, SliceEnumIter, SliceRef, StringEnumIter, UpValue,
};
use super::stack::Stack;
use super::value::*;
use super::vm_util;
use std::cell::{Ref, RefCell};
use std::collections::HashMap;
use std::pin::Pin;
use std::rc::Rc;

#[derive(Debug)]
pub struct ByteCode {
    pub objects: Pin<Box<VMObjects>>,
    pub packages: Vec<PackageKey>,
    pub entry: FunctionKey,
}

#[derive(Clone, Debug)]
enum Callable {
    Function(FunctionKey),
    Closure(GosValue),
}

impl Callable {
    fn func(&self) -> FunctionKey {
        match self {
            Callable::Function(f) => *f,
            Callable::Closure(c) => c.as_closure().borrow().func,
        }
    }

    fn closure(&self) -> &Rc<RefCell<ClosureVal>> {
        match self {
            Callable::Function(_) => unreachable!(),
            Callable::Closure(c) => c.as_closure(),
        }
    }

    fn ret_count(&self, objs: &VMObjects) -> usize {
        let fkey = self.func();
        objs.functions[fkey].ret_count
    }

    fn receiver(&self) -> Option<GosValue> {
        match self {
            Callable::Function(_) => None,
            Callable::Closure(c) => c.as_closure().borrow().receiver.clone(),
        }
    }
}

#[derive(Clone, Debug)]
struct CallFrame {
    callable: Callable,
    pc: usize,
    stack_base: usize,
    // closures that have upvalues pointing to this frame
    referred_by: Option<HashMap<OpIndex, Vec<GosValue>>>,
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

    fn with_closure(c: GosValue, sbase: usize) -> CallFrame {
        CallFrame {
            callable: Callable::Closure(c),
            pc: 0,
            stack_base: sbase,
            referred_by: None,
        }
    }

    fn with_gos_value(val: &GosValue, sbase: usize) -> CallFrame {
        match val {
            GosValue::Function(fkey) => CallFrame::with_func(fkey.clone(), sbase),
            GosValue::Closure(ckey) => {
                CallFrame::with_closure(GosValue::Closure(ckey.clone()), sbase)
            }
            _ => unreachable!(),
        }
    }

    fn add_referred_by(&mut self, index: OpIndex, closure: GosValue) {
        if self.referred_by.is_none() {
            self.referred_by = Some(HashMap::new());
        }
        let map = self.referred_by.as_mut().unwrap();
        match map.get_mut(&index) {
            Some(v) => {
                v.push(closure);
            }
            None => {
                map.insert(index, vec![closure]);
            }
        }
    }

    fn remove_referred_by(&mut self, index: OpIndex, closure: GosValue) {
        let map = self.referred_by.as_mut().unwrap();
        let v = map.get_mut(&index).unwrap();
        for i in 0..v.len() {
            if v[i] == closure {
                v.swap_remove(i);
                break;
            }
        }
    }

    fn ret_count(&self, objs: &VMObjects) -> usize {
        self.callable.ret_count(objs)
    }
}

pub struct Fiber {
    stack: Stack,
    frames: Vec<CallFrame>,
    caller: Option<Rc<RefCell<Fiber>>>,
    next_frame: Option<CallFrame>,
}

impl Fiber {
    fn new(caller: Option<Rc<RefCell<Fiber>>>) -> Fiber {
        Fiber {
            stack: Stack::new(),
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
        let fkey = frame.callable.func();
        let mut func = &objs.functions[fkey];
        let stack = &mut self.stack;
        // allocate local variables
        for _ in 0..func.local_count() {
            stack.push_nil();
        }
        let mut consts = &func.consts;
        let mut code = &func.code;
        let mut stack_base = frame.stack_base;

        let mut range_slot = 0;
        range_vars!(mr0, mp0, mi0, lr0, lp0, li0, sr0, sp0, si0);
        range_vars!(mr1, mp1, mi1, lr1, lp1, li1, sr1, sp1, si1);
        range_vars!(mr2, mp2, mi2, lr2, lp2, li2, sr2, sp2, si2);
        range_vars!(mr3, mp3, mi3, lr3, lp3, li3, sr3, sp3, si3);
        range_vars!(mr4, mp4, mi4, lr4, lp4, li4, sr4, sp4, si4);
        range_vars!(mr5, mp5, mi5, lr5, lp5, li5, sr5, sp5, si5);
        range_vars!(mr6, mp6, mi6, lr6, lp6, li6, sr6, sp6, si6);
        range_vars!(mr7, mp7, mi7, lr7, lp7, li7, sr7, sp7, si7);
        range_vars!(mr8, mp8, mi8, lr8, lp8, li8, sr8, sp8, si8);
        range_vars!(mr9, mp9, mi9, lr9, lp9, li9, sr9, sp9, si9);
        range_vars!(mr10, mp10, mi10, lr10, lp10, li10, sr10, sp10, si10);
        range_vars!(mr11, mp11, mi11, lr11, lp11, li11, sr11, sp11, si11);
        range_vars!(mr12, mp12, mi12, lr12, lp12, li12, sr12, sp12, si12);
        range_vars!(mr13, mp13, mi13, lr13, lp13, li13, sr13, sp13, si13);
        range_vars!(mr14, mp14, mi14, lr14, lp14, li14, sr14, sp14, si14);
        range_vars!(mr15, mp15, mi15, lr15, lp15, li15, sr15, sp15, si15);

        let mut total_inst = 0;
        //let mut stats: HashMap<Opcode, usize> = HashMap::new();
        loop {
            let inst = code[frame.pc];
            let inst_op = inst.op();
            total_inst += 1;
            //stats.entry(*inst).and_modify(|e| *e += 1).or_insert(1);
            frame.pc += 1;
            //dbg!(inst);
            match inst_op {
                Opcode::PUSH_CONST => {
                    let index = inst.imm();
                    let gos_val = &consts[index as usize];
                    let val = match gos_val {
                        // Slice/Map are special cases here because, they are stored literal,
                        // and when it gets cloned, the underlying rust vec is not copied
                        // which leads to all function calls shares the same vec instance
                        GosValue::Slice(s) => {
                            let slice = s.deep_clone();
                            GosValue::Slice(Rc::new(slice))
                        }
                        GosValue::Map(m) => {
                            let map = m.deep_clone();
                            GosValue::Map(Rc::new(map))
                        }
                        _ => gos_val.clone(),
                    };
                    stack.push(val);
                }
                Opcode::PUSH_NIL => stack.push_nil(),
                Opcode::PUSH_FALSE => stack.push_bool(false),
                Opcode::PUSH_TRUE => stack.push_bool(true),
                Opcode::PUSH_IMM => stack.push_int(inst.imm() as isize),
                Opcode::POP => {
                    stack.pop_discard(inst.t0());
                }
                Opcode::LOAD_LOCAL => {
                    let index = offset_uint!(stack_base, inst.imm());
                    stack.push_from_index(index, inst.t0()); // (index![stack, index]);
                }
                Opcode::STORE_LOCAL => {
                    let (rhs_index, index) = inst.imm2();
                    let s_index = offset_uint!(stack_base, index);
                    if rhs_index < 0 {
                        let rhs_s_index = offset_uint!(stack.len(), rhs_index);
                        stack_set!(stack, s_index, duplicate!(index![stack, rhs_s_index], objs));
                    } else {
                        let operand = index![stack, stack.len() - 1];
                        let mut lhs = index![stack, s_index];
                        let op_ex = Instruction::index2code(rhs_index);
                        vm_util::store_xxx_op(&mut lhs, op_ex, &operand);
                        stack_set!(stack, s_index, lhs);
                    }
                }
                Opcode::LOAD_UPVALUE => {
                    let index = inst.imm();
                    let upvalue =
                        frame.callable.closure().borrow().upvalues[index as usize].clone();
                    match &upvalue {
                        UpValue::Open(f, ind) => {
                            drop(frame);
                            let upframe = upframe!(self.frames.iter().rev().skip(1), f);
                            let stack_ptr = offset_uint!(upframe.stack_base, *ind);
                            stack.push(index![stack, stack_ptr]);
                            frame = self.frames.last_mut().unwrap();
                        }
                        UpValue::Closed(val) => {
                            stack.push(val.clone());
                        }
                    }
                }
                Opcode::STORE_UPVALUE => {
                    let (rhs_index, index) = inst.imm2();
                    let is_op_set = rhs_index > 0;
                    let (val, op_ex) = if is_op_set {
                        (
                            index![stack, stack.len() - 1],
                            Some(Instruction::index2code(rhs_index)),
                        )
                    } else {
                        let rhs_s_index = offset_uint!(stack.len(), rhs_index);
                        (duplicate!(index![stack, rhs_s_index], objs), None)
                    };
                    let closure = frame.callable.closure();
                    let upvalue = closure.borrow().upvalues[index as usize].clone();
                    match &upvalue {
                        UpValue::Open(f, ind) => {
                            let upframe = upframe!(self.frames.iter().rev().skip(1), f);
                            let stack_ptr = offset_uint!(upframe.stack_base, *ind);
                            let mut v = index![stack, stack_ptr];
                            vm_util::set_store_op_val(&mut v, op_ex, val, is_op_set);
                            stack_set!(stack, stack_ptr, v);
                            frame = self.frames.last_mut().unwrap();
                        }
                        UpValue::Closed(_) => {
                            // borrow checker happpy???
                            match &mut closure.borrow_mut().upvalues[index as usize] {
                                UpValue::Open(_, _) => unreachable!(),
                                UpValue::Closed(v) => {
                                    vm_util::set_store_op_val(v, op_ex, val, is_op_set)
                                }
                            }
                        }
                    }
                }
                Opcode::LOAD_FIELD | Opcode::LOAD_FIELD_IMM => {
                    let ind = if inst_op == Opcode::LOAD_FIELD {
                        stack.pop_with_type(inst.t1())
                    } else {
                        GosValue::Int(inst.imm() as isize)
                    };
                    let val = &stack.pop_with_type(inst.t0());
                    stack.push(match val {
                        GosValue::Boxed(b) => vm_util::load_field(&*b.borrow(), &ind, objs),
                        _ => vm_util::load_field(val, &ind, objs),
                    });
                }
                Opcode::STORE_FIELD | Opcode::STORE_FIELD_IMM => {
                    let (rhs_index, index) = inst.imm2();
                    let s_index = offset_uint!(stack.len(), index);
                    let key = if inst_op == Opcode::STORE_FIELD {
                        stack.get(s_index + 1)
                    } else {
                        GosValue::Int(index as isize)
                    };
                    let is_op_set = rhs_index > 0;
                    let (val, op_ex) = if is_op_set {
                        (
                            index![stack, stack.len() - 1],
                            Some(Instruction::index2code(rhs_index)),
                        )
                    } else {
                        let rhs_s_index = offset_uint!(stack.len(), rhs_index);
                        (duplicate!(index![stack, rhs_s_index], objs), None)
                    };

                    let store = index![stack, s_index];
                    match store {
                        GosValue::Boxed(b) => vm_util::store_field(
                            &*b.borrow(),
                            &key,
                            op_ex,
                            val,
                            is_op_set,
                            &objs.metas,
                        ),
                        _ => vm_util::store_field(&store, &key, op_ex, val, is_op_set, &objs.metas),
                    }
                }
                Opcode::LOAD_THIS_PKG_FIELD => {
                    let index = inst.imm();
                    let pkg = &objs.packages[func.package];
                    stack.push(pkg.member(index).clone());
                }
                Opcode::STORE_THIS_PKG_FIELD => {
                    let (rhs_index, index) = inst.imm2();
                    let pkg = &mut objs.packages[func.package];
                    if rhs_index < 0 {
                        let rhs_s_index = offset_uint!(stack.len(), rhs_index);
                        *pkg.member_mut(index) = duplicate!(index![stack, rhs_s_index], objs)
                    } else {
                        let operand = &index![stack, stack.len() - 1];
                        vm_util::store_xxx_op(
                            pkg.member_mut(index),
                            Instruction::index2code(rhs_index),
                            operand,
                        );
                    };
                }
                Opcode::STORE_DEREF => {
                    let (rhs_index, index) = inst.imm2();
                    let s_index = offset_uint!(stack.len(), index);
                    let store = stack.get(s_index);
                    if rhs_index < 0 {
                        let rhs_s_index = offset_uint!(stack.len(), rhs_index);
                        store
                            .as_boxed()
                            .replace(duplicate!(index![stack, rhs_s_index], objs));
                    } else {
                        let operand = &index![stack, stack.len() - 1];
                        vm_util::store_xxx_op(
                            &mut store.as_boxed().borrow_mut(),
                            Instruction::index2code(rhs_index),
                            operand,
                        );
                    }
                }
                Opcode::ADD => vm_util::add(stack),
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
                Opcode::REF => vm_util::unary_ref(stack),
                Opcode::DEREF => vm_util::unary_deref(stack),
                Opcode::ARROW => unimplemented!(),
                Opcode::NOT => vm_util::logical_not(stack),
                Opcode::EQL => vm_util::compare_eql(stack),
                Opcode::LSS => vm_util::compare_lss(stack),
                Opcode::GTR => vm_util::compare_gtr(stack),
                Opcode::NEQ => vm_util::compare_neq(stack),
                Opcode::LEQ => vm_util::compare_leq(stack),
                Opcode::GEQ => vm_util::compare_geq(stack),

                Opcode::PRE_CALL => {
                    let val = stack.pop();
                    let sbase = stack.len();
                    let next_frame = CallFrame::with_gos_value(&val, sbase);
                    let func_key = next_frame.callable.func();
                    let next_func = &objs.functions[func_key];
                    // init return values
                    if next_func.ret_count > 0 {
                        let meta_type = objs.metas[*next_func.meta.as_meta()].typ();
                        let rs = &meta_type.sig_metadata().results;
                        let mut returns = rs
                            .iter()
                            .map(|x| objs.metas[*x.as_meta()].zero_val().clone())
                            .collect();
                        stack.append(&mut returns);
                    }
                    // push receiver on stack as the first parameter
                    let receiver = next_frame.callable.receiver().map(|x| x.clone());
                    if let Some(r) = receiver {
                        stack.push(duplicate!(r, objs));
                    }
                    self.next_frame = Some(next_frame);
                }
                Opcode::CALL | Opcode::CALL_ELLIPSIS => {
                    self.frames.push(self.next_frame.take().unwrap());
                    frame = self.frames.last_mut().unwrap();
                    func = &objs.functions[frame.callable.func()];
                    stack_base = frame.stack_base;
                    consts = &func.consts;
                    code = &func.code;
                    //dbg!(&consts);
                    dbg!(&code);
                    //dbg!(&stack);

                    if func.variadic() && inst_op != Opcode::CALL_ELLIPSIS {
                        let index = stack_base
                            + func.param_count
                            + func.ret_count
                            + if frame.callable.receiver().is_some() {
                                1
                            } else {
                                0
                            }
                            - 1;
                        pack_variadic!(stack, index, objs);
                    }

                    // todo clone parameters

                    // allocate placeholders for local variables
                    for _ in 0..func.local_count() {
                        stack.push(GosValue::Nil);
                    }
                }
                Opcode::RETURN | Opcode::RETURN_INIT_PKG => {
                    // first handle upvalues in 2 steps:
                    // 1. clean up any referred_by created by this frame
                    let callable = frame.callable.clone();
                    match &callable {
                        Callable::Closure(c) => {
                            let cls_val = c.as_closure().borrow();
                            for uv in cls_val.upvalues.iter() {
                                match uv {
                                    UpValue::Open(f, ind) => {
                                        drop(frame);
                                        let upframe =
                                            upframe!(self.frames.iter_mut().rev().skip(1), f);
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
                            let val = index![stack, stack_base + *ind as usize];
                            let func = frame.callable.func();
                            for r in referrers.iter() {
                                let mut cls_val = r.as_closure().borrow_mut();
                                cls_val.close_upvalue(func, *ind, &val);
                            }
                        }
                    }

                    //dbg!(stack.len());
                    //for s in stack.iter() {
                    //    dbg!(GosValueDebug::new(&s, &objs));
                    //}

                    match inst_op {
                        Opcode::RETURN => {
                            stack.truncate(stack_base + frame.ret_count(objs));
                        }
                        Opcode::RETURN_INIT_PKG => {
                            let index = inst.imm() as usize;
                            let pkey = pkgs[index];
                            let pkg = &mut objs.packages[pkey];
                            let count = pkg.var_count();
                            // remove garbage first
                            stack.truncate(stack_base + count);
                            // the var values left on the stack are for pkg members
                            for i in 0..count {
                                let val = stack.pop();
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
                        dbg!(total_inst);
                        /*let mut s = stats
                            .iter()
                            .map(|(&k, &v)| (k, v))
                            .collect::<Vec<(Opcode, usize)>>();
                        s.sort_by(|a, b| b.1.cmp(&a.1));
                        dbg!(s); */
                        break;
                    }
                    frame = self.frames.last_mut().unwrap();
                    stack_base = frame.stack_base;
                    // restore func, consts, code
                    func = &objs.functions[frame.callable.func()];
                    consts = &func.consts;
                    code = &func.code;
                }

                Opcode::JUMP => {
                    frame.pc = offset_uint!(frame.pc, inst.imm());
                }
                Opcode::JUMP_IF => {
                    let val = stack.pop();
                    if *val.as_bool() {
                        frame.pc = offset_uint!(frame.pc, inst.imm());
                    }
                }
                Opcode::JUMP_IF_NOT => {
                    let val = stack.pop();
                    if !*val.as_bool() {
                        frame.pc = offset_uint!(frame.pc, inst.imm());
                    }
                }
                // Opcode::RANGE assumes a container and an int(as the cursor) on the stack
                // and followed by a target jump address
                Opcode::RANGE => {
                    let offset = inst.imm();
                    let len = stack.len();
                    let t = index![stack, len - 2];
                    let mut mark = *index![stack, len - 1].as_int();
                    if mark < 0 {
                        mark = range_slot;
                        range_slot += 1;
                        assert!(range_slot < 16);
                        match mark {
                            0 => range_init!(objs, t, mr0, mp0, mi0, lr0, lp0, li0, sr0, sp0, si0),
                            1 => range_init!(objs, t, mr1, mp1, mi1, lr1, lp1, li1, sr1, sp1, si1),
                            2 => range_init!(objs, t, mr2, mp2, mi2, lr2, lp2, li2, sr2, sp2, si2),
                            3 => range_init!(objs, t, mr3, mp3, mi3, lr3, lp3, li3, sr3, sp3, si3),
                            4 => range_init!(objs, t, mr4, mp4, mi4, lr4, lp4, li4, sr4, sp4, si4),
                            5 => range_init!(objs, t, mr5, mp5, mi5, lr5, lp5, li5, sr5, sp5, si5),
                            6 => range_init!(objs, t, mr6, mp6, mi6, lr6, lp6, li6, sr6, sp6, si6),
                            7 => range_init!(objs, t, mr7, mp7, mi7, lr7, lp7, li7, sr7, sp7, si7),
                            8 => range_init!(objs, t, mr8, mp8, mi8, lr8, lp8, li8, sr8, sp8, si8),
                            9 => range_init!(objs, t, mr9, mp9, mi9, lr9, lp9, li9, sr9, sp9, si9),
                            10 => range_init!(
                                objs, t, mr10, mp10, mi10, lr10, lp10, li10, sr10, sp10, si10
                            ),
                            11 => range_init!(
                                objs, t, mr11, mp11, mi11, lr11, lp11, li11, sr11, sp11, si11
                            ),
                            12 => range_init!(
                                objs, t, mr12, mp12, mi12, lr12, lp12, li12, sr12, sp12, si12
                            ),
                            13 => range_init!(
                                objs, t, mr13, mp13, mi13, lr13, lp13, li13, sr13, sp13, si13
                            ),
                            14 => range_init!(
                                objs, t, mr14, mp14, mi14, lr14, lp14, li14, sr14, sp14, si14
                            ),
                            15 => range_init!(
                                objs, t, mr15, mp15, mi15, lr15, lp15, li15, sr15, sp15, si15
                            ),
                            _ => unreachable!(),
                        }
                        stack_set!(stack, len - 1, GosValue::Int(mark));
                    }
                    let end = match mark {
                        0 => range_body!(t, stack, mp0, mi0, lp0, li0, sp0, si0),
                        1 => range_body!(t, stack, mp1, mi1, lp1, li1, sp1, si1),
                        2 => range_body!(t, stack, mp2, mi2, lp2, li2, sp2, si2),
                        3 => range_body!(t, stack, mp3, mi3, lp3, li3, sp3, si3),
                        4 => range_body!(t, stack, mp4, mi4, lp4, li4, sp4, si4),
                        5 => range_body!(t, stack, mp5, mi5, lp5, li5, sp5, si5),
                        6 => range_body!(t, stack, mp6, mi6, lp6, li6, sp6, si6),
                        7 => range_body!(t, stack, mp7, mi7, lp7, li7, sp7, si7),
                        8 => range_body!(t, stack, mp8, mi8, lp8, li8, sp8, si8),
                        9 => range_body!(t, stack, mp9, mi9, lp9, li9, sp9, si9),
                        10 => range_body!(t, stack, mp10, mi10, lp10, li10, sp10, si10),
                        11 => range_body!(t, stack, mp11, mi11, lp11, li11, sp11, si11),
                        12 => range_body!(t, stack, mp12, mi12, lp12, li12, sp12, si12),
                        13 => range_body!(t, stack, mp13, mi13, lp13, li13, sp13, si13),
                        14 => range_body!(t, stack, mp14, mi14, lp14, li14, sp14, si14),
                        15 => range_body!(t, stack, mp15, mi15, lp15, li15, sp15, si15),
                        _ => unreachable!(),
                    };
                    if end {
                        frame.pc = offset_uint!(frame.pc, offset);
                        range_slot -= 1;
                    }
                }

                Opcode::IMPORT => {
                    let pkey = pkgs[inst.imm() as usize];
                    stack.push(GosValue::Package(pkey));
                    stack.push(GosValue::Bool(!objs.packages[pkey].inited()));
                }
                Opcode::SLICE | Opcode::SLICE_FULL => {
                    let max = if inst_op == Opcode::SLICE_FULL {
                        Some(*stack.pop().as_int() as usize)
                    } else {
                        None
                    };
                    let end_val = stack.pop();
                    let begin_val = stack.pop();
                    let end = if end_val.is_nil() {
                        None
                    } else {
                        Some(*end_val.as_int() as usize)
                    };
                    let begin = if begin_val.is_nil() {
                        None
                    } else {
                        Some(*begin_val.as_int() as usize)
                    };
                    let target = stack.pop();
                    let result = match target {
                        GosValue::Slice(sl) => GosValue::Slice(Rc::new(sl.slice(begin, end, max))),
                        GosValue::Str(s) => GosValue::Str(Rc::new(s.slice(begin, end))),
                        _ => unreachable!(),
                    };
                    stack.push(result);
                }

                Opcode::NEW => {
                    let new_val = match stack.pop() {
                        GosValue::Function(fkey) => {
                            // NEW a closure
                            let func = &objs.functions[fkey];
                            let closure = GosValue::Closure(Rc::new(RefCell::new(
                                ClosureVal::new(fkey, func.up_ptrs.clone()),
                            )));
                            // set referred_by for the frames down in the stack
                            for uv in func.up_ptrs.iter() {
                                match uv {
                                    UpValue::Open(func, ind) => {
                                        drop(frame);
                                        let upframe = upframe!(self.frames.iter_mut().rev(), func);
                                        upframe.add_referred_by(*ind, closure.clone());
                                        frame = self.frames.last_mut().unwrap();
                                    }
                                    UpValue::Closed(_) => unreachable!(),
                                }
                            }
                            closure
                        }
                        _ => unimplemented!(),
                    };
                    stack.push(new_val);
                }
                Opcode::MAKE => {
                    let index = inst.imm();
                    let meta = index![stack, offset_uint!(stack.len(), index - 1)];
                    let metadata = &objs.metas[*meta.as_meta()];
                    let val = match metadata.typ() {
                        MetadataType::Slice(vmeta) => {
                            let (cap, len) = match index {
                                -2 => (
                                    *stack.pop().as_int() as usize,
                                    *stack.pop().as_int() as usize,
                                ),
                                -1 => {
                                    let len = *stack.pop().as_int() as usize;
                                    (len, len)
                                }
                                _ => unreachable!(),
                            };
                            let vmetadata = &objs.metas[*vmeta.as_meta()];
                            GosValue::new_slice(len, cap, vmetadata.zero_val(), &mut objs.slices)
                        }
                        MetadataType::Map(_k, _v) => unimplemented!(),
                        MetadataType::Channel(_st) => unimplemented!(),
                        _ => unreachable!(),
                    };
                    stack.pop();
                    stack.push(val);
                }
                Opcode::LEN => match &stack.pop() {
                    GosValue::Slice(slice) => {
                        stack.push(GosValue::Int(slice.len() as isize));
                    }
                    GosValue::Map(map) => {
                        stack.push(GosValue::Int(map.len() as isize));
                    }
                    GosValue::Str(sval) => {
                        stack.push(GosValue::Int(sval.len() as isize));
                    }
                    _ => unreachable!(),
                },
                Opcode::CAP => match stack.pop() {
                    GosValue::Slice(slice) => {
                        stack.push(GosValue::Int(slice.cap() as isize));
                    }
                    _ => unreachable!(),
                },
                Opcode::APPEND => {
                    let index = offset_uint!(stack.len(), inst.imm());
                    pack_variadic!(stack, index, objs);
                    let b = stack.pop();
                    let a = index![stack, stack.len() - 1];
                    let vala = a.as_slice();
                    let valb = b.as_slice();
                    vala.borrow_data_mut()
                        .append(&mut valb.borrow_data().clone());
                }
                Opcode::ASSERT => match stack.pop() {
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
    objects: Pin<Box<VMObjects>>,
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

#[cfg(test)]
mod test {}
