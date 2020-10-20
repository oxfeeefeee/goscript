#![allow(dead_code)]
use super::instruction::*;
use super::objects::{
    u64_to_key, ClosureObj, GosHashMap, MetadataType, SliceEnumIter, SliceRef, StringEnumIter,
};
use super::stack::Stack;
use super::value::*;
use super::vm_util;
use std::cell::{Ref, RefCell};
use std::collections::HashMap;
use std::convert::TryInto;
use std::pin::Pin;
use std::rc::Rc;

#[derive(Debug)]
pub struct ByteCode {
    pub objects: Pin<Box<VMObjects>>,
    pub packages: Vec<PackageKey>,
    pub ifaces: Vec<(GosValue, Rc<Vec<FunctionKey>>)>,
    pub entry: FunctionKey,
}

#[derive(Clone, Debug)]
struct Referers {
    typ: ValueType,
    weaks: Vec<WeakUpValue>,
}

#[derive(Clone, Debug)]
struct CallFrame {
    closure: Rc<ClosureObj>,
    pc: usize,
    stack_base: usize,
    // closures that have upvalues pointing to this frame
    referred_by: Option<HashMap<OpIndex, Referers>>,
}

impl CallFrame {
    fn with_closure(c: GosValue, sbase: usize) -> CallFrame {
        CallFrame {
            closure: c.as_closure().clone(),
            pc: 0,
            stack_base: sbase,
            referred_by: None,
        }
    }

    fn add_referred_by(&mut self, index: OpIndex, typ: ValueType, uv: &UpValue) {
        if self.referred_by.is_none() {
            self.referred_by = Some(HashMap::new());
        }
        let map = self.referred_by.as_mut().unwrap();
        let weak = uv.downgrade();
        match map.get_mut(&index) {
            Some(v) => {
                debug_assert!(v.typ == typ);
                v.weaks.push(weak);
            }
            None => {
                map.insert(
                    index,
                    Referers {
                        typ: typ,
                        weaks: vec![weak],
                    },
                );
            }
        }
    }

    #[inline]
    fn func(&self) -> FunctionKey {
        self.closure.func
    }

    #[inline]
    fn closure(&self) -> &Rc<ClosureObj> {
        &self.closure
    }

    #[inline]
    fn ret_count(&self, objs: &VMObjects) -> usize {
        let fkey = self.func();
        objs.functions[fkey].ret_count
    }

    #[inline]
    fn receiver(&self) -> &Option<GosValue> {
        &self.closure.receiver
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

    fn run(&mut self, code: &mut ByteCode) {
        let cls = GosValue::new_closure(code.entry);
        let frame = CallFrame::with_closure(cls, 0);
        self.frames.push(frame);
        self.main_loop(code);
    }

    fn main_loop(&mut self, code: &mut ByteCode) {
        let objs: &mut VMObjects = &mut code.objects;
        let pkgs = &code.packages;
        let ifaces = &code.ifaces;
        let mut frame = self.frames.last_mut().unwrap();
        let fkey = frame.func();
        let mut func = &objs.functions[fkey];
        let stack = &mut self.stack;
        // allocate local variables
        for _ in 0..func.local_count() {
            stack.push_nil();
        }
        let mut consts = &func.consts;
        let mut code = &func.code;
        let mut stack_base = frame.stack_base;

        let zval = &objs.zero_val;

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
                        _ => gos_val.copy_semantic(None),
                    };
                    stack.push(val);
                }
                Opcode::PUSH_NIL => stack.push_nil(),
                Opcode::PUSH_FALSE => stack.push_bool(false),
                Opcode::PUSH_TRUE => stack.push_bool(true),
                Opcode::PUSH_IMM => stack.push_int(inst.imm() as isize),
                Opcode::POP => {
                    stack.pop_discard_n(inst.imm() as usize);
                }
                Opcode::LOAD_LOCAL => {
                    let index = Stack::offset(stack_base, inst.imm());
                    stack.push_from_index(index, inst.t0()); // (index![stack, index]);
                }
                Opcode::STORE_LOCAL => {
                    let (rhs_index, index) = inst.imm824();
                    let s_index = Stack::offset(stack_base, index);
                    store_local!(stack, s_index, rhs_index, inst.t0(), zval);
                }
                Opcode::LOAD_UPVALUE => {
                    let index = inst.imm();
                    let upvalue = frame.closure().upvalues()[index as usize].clone();
                    stack.push(load_up_value!(upvalue, self, stack, frame));
                }
                Opcode::STORE_UPVALUE => {
                    let (rhs_index, index) = inst.imm824();
                    let upvalue = frame.closure().upvalues()[index as usize].clone();
                    store_up_value!(upvalue, self, stack, frame, rhs_index, inst.t0(), zval);
                }
                Opcode::LOAD_INDEX => {
                    let ind = stack.pop_with_type(inst.t1());
                    let val = &stack.pop_with_type(inst.t0());
                    stack.push(vm_util::load_index(val, &ind));
                }
                Opcode::LOAD_INDEX_IMM => {
                    let val = &stack.pop_with_type(inst.t0());
                    stack.push(vm_util::load_index_int(val, inst.imm() as usize));
                }
                Opcode::STORE_INDEX => {
                    let (rhs_index, index) = inst.imm824();
                    let s_index = Stack::offset(stack.len(), index);
                    let key = stack.get_with_type(s_index + 1, inst.t2());
                    let target = &stack.get_with_type(s_index, inst.t1());
                    vm_util::store_index(stack, target, &key, rhs_index, inst.t0(), objs);
                }
                Opcode::STORE_INDEX_IMM => {
                    // the only place we can store the immediate index is t2
                    let (rhs_index, imm) = inst.imm824();
                    let index = inst.t2_as_index();
                    let s_index = Stack::offset(stack.len(), index);
                    let target = &stack.get_with_type(s_index, inst.t1());
                    vm_util::store_index_int(
                        stack,
                        target,
                        imm as usize,
                        rhs_index,
                        inst.t0(),
                        objs,
                    );
                }
                Opcode::LOAD_FIELD => {
                    let ind = stack.pop_with_type(inst.t1());
                    let val = stack.pop_with_type(inst.t0());
                    stack.push(match val {
                        GosValue::Boxed(_) => {
                            let unboxed = deref_value!(val, self, stack, frame, objs);
                            vm_util::load_field(&unboxed, &ind, objs)
                        }
                        _ => vm_util::load_field(&val, &ind, objs),
                    });
                }
                Opcode::LOAD_STRUCT_FIELD => {
                    let ind = inst.imm();
                    let mut target = stack.pop_with_type(inst.t0());
                    if let GosValue::Boxed(_) = &target {
                        target = deref_value!(target, self, stack, frame, objs);
                    }
                    let val = if let GosValue::Struct(sval) = &target {
                        sval.borrow().fields[ind as usize].clone()
                    } else {
                        unreachable!()
                    };
                    stack.push(val);
                }
                Opcode::BIND_METHOD => {
                    let val = stack.pop_with_type(inst.t0());
                    let func = *consts[inst.imm() as usize].as_function();
                    stack.push(GosValue::Closure(Rc::new(ClosureObj::new(
                        func,
                        Some(val.copy_semantic(None)),
                        None,
                    ))));
                }
                Opcode::BIND_INTERFACE_METHOD => {
                    let val = stack.pop_with_type(ValueType::Interface);
                    let borrowed = val.as_interface().borrow();
                    let (val, funcs) = borrowed.underlying().as_ref().unwrap();
                    let func = funcs[inst.imm() as usize];
                    stack.push(GosValue::Closure(Rc::new(ClosureObj::new(
                        func,
                        Some(val.copy_semantic(None)),
                        None,
                    ))));
                }
                Opcode::STORE_FIELD => {
                    let (rhs_index, _) = inst.imm824();
                    let index = inst.t2_as_index();
                    let s_index = Stack::offset(stack.len(), index);
                    let key = stack.get_with_type(s_index + 1, inst.t2());
                    let target = stack.get_with_type(s_index, inst.t1());
                    match target {
                        GosValue::Boxed(_) => {
                            let unboxed = deref_value!(target, self, stack, frame, objs);
                            vm_util::store_field(stack, &unboxed, &key, rhs_index, inst.t0(), objs);
                        }
                        _ => vm_util::store_field(stack, &target, &key, rhs_index, inst.t0(), objs),
                    };
                }
                Opcode::STORE_STRUCT_FIELD => {
                    let (rhs_index, imm) = inst.imm824();
                    let index = inst.t2_as_index();
                    let s_index = Stack::offset(stack.len(), index);
                    let mut target = stack.get_with_type(s_index, inst.t1());
                    if let GosValue::Boxed(_) = &target {
                        target = deref_value!(target, self, stack, frame, objs);
                    }
                    if let GosValue::Struct(s) = &target {
                        let field = &mut s.borrow_mut().fields[imm as usize];
                        stack.store_val(field, rhs_index, inst.t0(), &objs.zero_val);
                    } else {
                        unreachable!();
                    }
                }
                Opcode::LOAD_PKG_FIELD => {
                    let index = inst.imm();
                    let pkg_key = read_imm_pkg!(code, frame, objs);
                    let pkg = &mut objs.packages[pkg_key];
                    stack.push(pkg.member(index).clone());
                }
                Opcode::STORE_PKG_FIELD => {
                    let (rhs_index, imm) = inst.imm824();
                    let pkg = &mut objs.packages[read_imm_pkg!(code, frame, objs)];
                    stack.store_val(pkg.member_mut(imm), rhs_index, inst.t0(), zval);
                }
                Opcode::STORE_DEREF => {
                    let (rhs_index, index) = inst.imm824();
                    let s_index = Stack::offset(stack.len(), index);
                    match stack.get_with_type(s_index, ValueType::Boxed) {
                        GosValue::Boxed(b) => {
                            match *b {
                                BoxedObj::Nil => unimplemented!(), //panic?
                                BoxedObj::UpVal(uv) => {
                                    store_up_value!(
                                        uv,
                                        self,
                                        stack,
                                        frame,
                                        rhs_index,
                                        inst.t0(),
                                        zval
                                    );
                                }
                                BoxedObj::Struct(s) => {
                                    let rhs_s_index = Stack::offset(stack.len(), rhs_index);
                                    let val = stack.get_with_type(rhs_s_index, ValueType::Struct);
                                    s.replace(RefCell::clone(&*val.as_struct()).into_inner());
                                }
                                BoxedObj::SliceMember(s, index) => {
                                    let rhs_s_index = Stack::offset(stack.len(), rhs_index);
                                    let val = stack.get_with_type(rhs_s_index, inst.t0());
                                    s.set(index as usize, val);
                                }
                                BoxedObj::StructField(s, index) => {
                                    let rhs_s_index = Stack::offset(stack.len(), rhs_index);
                                    let val = stack.get_with_type(rhs_s_index, inst.t0());
                                    s.borrow_mut().fields[index as usize] = val;
                                }
                                BoxedObj::PkgMember(p, index) => {
                                    let rhs_s_index = Stack::offset(stack.len(), rhs_index);
                                    let val = stack.get_with_type(rhs_s_index, inst.t0());
                                    *objs.packages[p].member_mut(index) = val;
                                }
                            };
                        }
                        _ => unreachable!(),
                    }
                }
                Opcode::CAST_TO_INTERFACE => {
                    let (target, mapping) = inst.imm824();
                    let rhs_s_index = Stack::offset(stack.len(), target);
                    let iface = ifaces[mapping as usize].clone();
                    let named = stack.get_with_type(rhs_s_index, inst.t0());
                    let val =
                        GosValue::new_iface(iface.0, Some((named, iface.1)), &mut objs.interfaces);
                    stack.set(rhs_s_index, val);
                }
                Opcode::ADD => stack.add(inst.t0()),
                Opcode::SUB => stack.sub(inst.t0()),
                Opcode::MUL => stack.mul(inst.t0()),
                Opcode::QUO => stack.quo(inst.t0()),
                Opcode::REM => stack.rem(inst.t0()),
                Opcode::AND => stack.and(inst.t0()),
                Opcode::OR => stack.or(inst.t0()),
                Opcode::XOR => stack.xor(inst.t0()),
                Opcode::AND_NOT => stack.and_not(inst.t0()),
                Opcode::SHL => stack.shl(inst.t0()),
                Opcode::SHR => stack.shr(inst.t0()),
                Opcode::UNARY_ADD => {}
                Opcode::UNARY_SUB => stack.unary_negate(inst.t0()),
                Opcode::UNARY_XOR => stack.unary_xor(inst.t0()),
                Opcode::NOT => stack.logical_not(inst.t0()),
                Opcode::EQL => stack.compare_eql(inst.t0()),
                Opcode::LSS => stack.compare_lss(inst.t0()),
                Opcode::GTR => stack.compare_gtr(inst.t0()),
                Opcode::NEQ => stack.compare_neq(inst.t0()),
                Opcode::LEQ => stack.compare_leq(inst.t0()),
                Opcode::GEQ => stack.compare_geq(inst.t0()),
                Opcode::ARROW => unimplemented!(),
                Opcode::REF_UPVALUE => {
                    let index = inst.imm();
                    let upvalue = frame.closure().upvalues()[index as usize].clone();
                    stack.push(GosValue::new_boxed(BoxedObj::UpVal(upvalue.clone())));
                }
                Opcode::REF_LOCAL => {
                    let t = inst.t0();
                    let s_index = Stack::offset(stack_base, inst.imm());
                    let boxed = if t == ValueType::Struct {
                        BoxedObj::new_var_pointer(stack.get_with_type(s_index, t))
                    } else {
                        BoxedObj::new_var_up_val(ValueDesc {
                            func: frame.func(),
                            index: s_index as OpIndex,
                            typ: t,
                        })
                    };
                    stack.push(GosValue::new_boxed(boxed));
                }
                Opcode::REF_SLICE_MEMBER => {
                    let index = stack.pop_int();
                    let slice = stack.pop_with_type(inst.t0());
                    stack.push(GosValue::new_boxed(BoxedObj::new_slice_member(
                        &slice,
                        index.try_into().unwrap(),
                    )));
                }
                Opcode::REF_STRUCT_FIELD => {
                    let struct_ = stack.pop_with_type(ValueType::Struct);
                    stack.push(GosValue::new_boxed(BoxedObj::new_struct_field(
                        &struct_,
                        inst.imm(),
                    )));
                }
                Opcode::REF_PKG_MEMBER => {
                    let pkg = read_imm_pkg!(code, frame, objs);
                    stack.push(GosValue::new_boxed(BoxedObj::PkgMember(pkg, inst.imm())));
                }
                Opcode::DEREF => {
                    let boxed = stack.pop_with_type(inst.t0());
                    let val = deref_value!(boxed, self, stack, frame, objs);
                    stack.push(val);
                }
                Opcode::PRE_CALL => {
                    let val = stack.pop_with_type(ValueType::Closure);
                    let sbase = stack.len();
                    let next_frame = CallFrame::with_closure(val, sbase);
                    let func_key = next_frame.func();
                    let next_func = &objs.functions[func_key];
                    // init return values
                    if next_func.ret_count > 0 {
                        let meta = &objs.metas[*next_func.meta.as_meta()];
                        let rs = &meta.sig_metadata().results;
                        let mut returns = rs
                            .iter()
                            .map(|x| objs.metas[*x.as_meta()].zero_val().clone())
                            .collect();
                        stack.append(&mut returns);
                    }
                    // push receiver on stack as the first parameter
                    if let Some(r) = next_frame.receiver() {
                        // don't call copy_semantic because BIND_METHOD did it already
                        stack.push(r.clone());
                    }
                    self.next_frame = Some(next_frame);
                }
                Opcode::CALL | Opcode::CALL_ELLIPSIS => {
                    self.frames.push(self.next_frame.take().unwrap());
                    frame = self.frames.last_mut().unwrap();
                    func = &objs.functions[frame.func()];
                    stack_base = frame.stack_base;
                    consts = &func.consts;
                    code = &func.code;
                    //dbg!(&consts);
                    dbg!(&code);
                    //dbg!(&stack);

                    if let Some(vt) = func.variadic() {
                        if inst_op != Opcode::CALL_ELLIPSIS {
                            let index = stack_base
                                + func.param_count
                                + func.ret_count
                                + if frame.receiver().is_some() { 1 } else { 0 }
                                - 1;
                            stack.pack_variadic(index, vt, &mut objs.slices);
                        }
                    }

                    // todo: clone parameters, initialize nil values

                    debug_assert!(func.local_count() == func.local_zeros.len());
                    // allocate local variables
                    for v in func.local_zeros.iter() {
                        stack.push(v.clone());
                    }
                }
                Opcode::RETURN | Opcode::RETURN_INIT_PKG => {
                    // close any active upvalue this frame contains
                    if let Some(referred) = &frame.referred_by {
                        for (ind, referrers) in referred {
                            if referrers.weaks.len() == 0 {
                                continue;
                            }
                            let val =
                                stack.get_with_type(Stack::offset(stack_base, *ind), referrers.typ);
                            for weak in referrers.weaks.iter() {
                                if let Some(uv) = weak.upgrade() {
                                    uv.close(val.clone());
                                }
                            }
                        }
                    }

                    //dbg!(stack.len());
                    //for s in stack.iter() {
                    //    dbg!(GosValueDebug::new(&s, &objs));
                    //}

                    match inst_op {
                        Opcode::RETURN => {
                            //for v in func.local_zeros.iter().skip(frame.ret_count(objs)).rev() {
                            //    stack.pop_with_type(v.get_type());
                            //}
                            stack.truncate(stack_base + frame.ret_count(objs));
                        }
                        Opcode::RETURN_INIT_PKG => {
                            let index = inst.imm() as usize;
                            let pkey = pkgs[index];
                            let pkg = &mut objs.packages[pkey];
                            let count = pkg.var_count();
                            // remove garbage first
                            debug_assert!(stack.len() == stack_base + count);
                            // the var values left on the stack are for pkg members
                            stack.init_pkg_vars(pkg, count);
                            /*for i in 0..count {
                                let val = stack.pop();
                                let index = (count - 1 - i) as OpIndex;
                                pkg.init_var(&index, val);
                            }*/
                            // the one pushed by IMPORT was poped by LOAD_FIELD
                            //stack.push(GosValue::Package(pkey));
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
                    func = &objs.functions[frame.func()];
                    consts = &func.consts;
                    code = &func.code;
                }

                Opcode::JUMP => {
                    frame.pc = Stack::offset(frame.pc, inst.imm());
                }
                Opcode::JUMP_IF => {
                    if stack.pop_bool() {
                        frame.pc = Stack::offset(frame.pc, inst.imm());
                    }
                }
                Opcode::JUMP_IF_NOT => {
                    if !stack.pop_bool() {
                        frame.pc = Stack::offset(frame.pc, inst.imm());
                    }
                }
                Opcode::SWITCH => {
                    if stack.switch_cmp(inst.t0()) {
                        frame.pc = Stack::offset(frame.pc, inst.imm());
                    }
                }
                // Opcode::RANGE assumes a container and an int(as the cursor) on the stack
                // and followed by a target jump address
                Opcode::RANGE => {
                    let offset = inst.imm();
                    let len = stack.len();
                    let t = stack.get_with_type(len - 2, inst.t0());
                    let mut mark = *stack.get_with_type(len - 1, ValueType::Int).as_int();
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
                        stack.set(len - 1, GosValue::Int(mark));
                    }
                    let end = match mark {
                        0 => range_body!(t, stack, inst, mp0, mi0, lp0, li0, sp0, si0),
                        1 => range_body!(t, stack, inst, mp1, mi1, lp1, li1, sp1, si1),
                        2 => range_body!(t, stack, inst, mp2, mi2, lp2, li2, sp2, si2),
                        3 => range_body!(t, stack, inst, mp3, mi3, lp3, li3, sp3, si3),
                        4 => range_body!(t, stack, inst, mp4, mi4, lp4, li4, sp4, si4),
                        5 => range_body!(t, stack, inst, mp5, mi5, lp5, li5, sp5, si5),
                        6 => range_body!(t, stack, inst, mp6, mi6, lp6, li6, sp6, si6),
                        7 => range_body!(t, stack, inst, mp7, mi7, lp7, li7, sp7, si7),
                        8 => range_body!(t, stack, inst, mp8, mi8, lp8, li8, sp8, si8),
                        9 => range_body!(t, stack, inst, mp9, mi9, lp9, li9, sp9, si9),
                        10 => range_body!(t, stack, inst, mp10, mi10, lp10, li10, sp10, si10),
                        11 => range_body!(t, stack, inst, mp11, mi11, lp11, li11, sp11, si11),
                        12 => range_body!(t, stack, inst, mp12, mi12, lp12, li12, sp12, si12),
                        13 => range_body!(t, stack, inst, mp13, mi13, lp13, li13, sp13, si13),
                        14 => range_body!(t, stack, inst, mp14, mi14, lp14, li14, sp14, si14),
                        15 => range_body!(t, stack, inst, mp15, mi15, lp15, li15, sp15, si15),
                        _ => unreachable!(),
                    };
                    if end {
                        frame.pc = Stack::offset(frame.pc, offset);
                        range_slot -= 1;
                    }
                }

                Opcode::TYPE_ASSERT => {}
                Opcode::TYPE_TRY_ASSERT => {}
                Opcode::TYPE => {}
                Opcode::TYPE_ASSIGN => {}

                Opcode::IMPORT => {
                    let pkey = pkgs[inst.imm() as usize];
                    stack.push(GosValue::Bool(!objs.packages[pkey].inited()));
                }
                Opcode::SLICE | Opcode::SLICE_FULL => {
                    let max = if inst_op == Opcode::SLICE_FULL {
                        stack.pop_int()
                    } else {
                        -1
                    };
                    let end = stack.pop_int();
                    let begin = stack.pop_int();
                    let target = stack.pop_with_type(inst.t0());
                    let result = match &target {
                        GosValue::Slice(sl) => GosValue::Slice(Rc::new(sl.slice(begin, end, max))),
                        GosValue::Str(s) => GosValue::Str(Rc::new(s.slice(begin, end))),
                        _ => unreachable!(),
                    };
                    stack.push(result);
                }

                Opcode::NEW => {
                    let new_val = match stack.pop_with_type(inst.t0()) {
                        GosValue::Function(fkey) => {
                            // NEW a closure
                            let func = &objs.functions[fkey];
                            let val = ClosureObj::new(fkey, None, Some(func.up_ptrs.clone()));
                            for uv in val.upvalues().iter() {
                                drop(frame);
                                let desc = uv.desc();
                                let upframe = upframe!(self.frames.iter_mut().rev(), desc.func);
                                upframe.add_referred_by(desc.index, desc.typ, uv);
                                frame = self.frames.last_mut().unwrap();
                            }
                            GosValue::Closure(Rc::new(val))
                        }
                        _ => unimplemented!(),
                    };
                    stack.push(new_val);
                }
                Opcode::MAKE => {
                    let index = inst.imm();
                    let i = Stack::offset(stack.len(), index - 1);
                    let meta = stack.get_with_type(i, ValueType::Metadata);
                    let metadata = &objs.metas[*meta.as_meta()];
                    let val = match metadata.typ() {
                        MetadataType::Slice(vmeta) => {
                            let (cap, len) = match index {
                                -2 => (stack.pop_int() as usize, stack.pop_int() as usize),
                                -1 => {
                                    let len = stack.pop_int() as usize;
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
                    stack.pop_discard();
                    stack.push(val);
                }
                Opcode::LEN => match &stack.pop_with_type(inst.t0()) {
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
                Opcode::CAP => match &stack.pop_with_type(inst.t0()) {
                    GosValue::Slice(slice) => {
                        stack.push(GosValue::Int(slice.cap() as isize));
                    }
                    _ => unreachable!(),
                },
                Opcode::APPEND => {
                    let index = Stack::offset(stack.len(), inst.imm());
                    stack.pack_variadic(index, inst.t1(), &mut objs.slices);
                    let b = stack.pop_with_type(ValueType::Slice);
                    let a = stack.get_with_type(stack.len() - 1, ValueType::Slice);
                    let vala = a.as_slice();
                    let valb = b.as_slice();
                    vala.borrow_data_mut()
                        .append(&mut valb.borrow_data().clone());
                }
                Opcode::ASSERT => {
                    if !stack.pop_bool() {
                        assert!(false, "Opcode::ASSERT: not true!");
                    }
                }
                _ => {
                    dbg!(inst_op);
                    unimplemented!();
                }
            };
            //dbg!(inst_op, stack.len());
        }
    }
}

pub struct GosVM {
    fibers: Vec<Rc<RefCell<Fiber>>>,
    current_fiber: Option<Rc<RefCell<Fiber>>>,
    code: ByteCode,
}

impl GosVM {
    pub fn new(bc: ByteCode) -> GosVM {
        let mut vm = GosVM {
            fibers: Vec::new(),
            current_fiber: None,
            code: bc,
        };
        let fb = Rc::new(RefCell::new(Fiber::new(None)));
        vm.fibers.push(fb.clone());
        vm.current_fiber = Some(fb);
        vm
    }

    pub fn run(&mut self) {
        let mut fb = self.current_fiber.as_ref().unwrap().borrow_mut();
        fb.run(&mut self.code);
    }
}

#[cfg(test)]
mod test {}
