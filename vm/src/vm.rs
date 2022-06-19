// Copyright 2022 The Goscript Authors. All rights reserved.
// Use of this source code is governed by a BSD-style
// license that can be found in the LICENSE file.

#![allow(dead_code)]
use crate::channel;
use crate::ffi::{FfiCallCtx, FfiFactory};
use crate::gc::{gc, GcoVec};
use crate::metadata::*;
use crate::objects::ClosureObj;
use crate::stack::{RangeStack, Stack};
use crate::value::*;
use async_executor::LocalExecutor;
use futures_lite::future;
use goscript_parser::{FilePos, FileSet};
use std::cell::{Cell, RefCell};
use std::cmp::Ordering;
use std::collections::HashMap;
use std::rc::Rc;

// restore stack_ref after drop to allow code in block call yield
macro_rules! restore_stack_ref {
    ($self_:ident, $stack:ident, $stack_ref:ident) => {{
        $stack_ref = $self_.stack.borrow_mut();
        $stack = &mut $stack_ref;
    }};
}

macro_rules! go_panic {
    ($panic:ident, $msg:expr, $frame:ident, $code:ident) => {{
        let mut data = PanicData::new($msg);
        data.call_stack.push(($frame.func(), $frame.pc - 1));
        $panic = Some(data);
        $frame.pc = $code.len() as OpIndex - 1;
    }};
}

macro_rules! go_panic_str {
    ($panic:ident, $msg:expr, $frame:ident, $code:ident) => {{
        let str_val = GosValue::with_str($msg);
        let iface = GosValue::empty_iface_with_val(str_val);
        let mut data = PanicData::new(iface);
        data.call_stack.push(($frame.func(), $frame.pc - 1));
        $panic = Some(data);
        $frame.pc = $code.len() as OpIndex - 1;
    }};
}

macro_rules! panic_if_err {
    ($result:expr, $panic:ident, $frame:ident, $code:ident) => {{
        if let Err(e) = $result {
            go_panic_str!($panic, &e, $frame, $code);
        }
    }};
}

macro_rules! unwrap_recv_val {
    ($chan:expr, $val:expr, $gcv:expr) => {
        match $val {
            Some(v) => (v, true),
            None => ($chan.recv_zero.copy_semantic($gcv), false),
        }
    };
}

macro_rules! binary_op {
    ($stack:expr, $op:tt, $inst:expr, $sb:expr, $consts:expr) => {{
        let vdata = $stack
            .read($inst.s0, $sb, $consts)
            .data()
            .$op($stack.read($inst.s1, $sb, $consts).data(), $inst.t0);
        let val = GosValue::new($inst.t0, vdata);
        $stack.set($inst.d + $sb, val);
    }};
}

macro_rules! binary_op_assign {
    ($stack:ident, $op:tt, $inst:expr, $sb:expr, $consts:expr) => {{
        let right = unsafe { $stack.read($inst.s0, $sb, $consts).data().copy_non_ptr() };
        let d = $stack.get_data_mut($inst.d + $sb);
        *d = d.$op(&right, $inst.t0);
    }};
}

macro_rules! shift_op {
    ($stack:expr, $op:tt, $inst:expr, $sb:expr, $consts:expr) => {{
        let right = $stack
            .read($inst.s1, $sb, $consts)
            .data()
            .cast_copyable($inst.t1, ValueType::Uint32);
        let vdata = $stack
            .read($inst.s0, $sb, $consts)
            .data()
            .$op(right.as_uint32(), $inst.t0);
        let val = GosValue::new($inst.t0, vdata);
        $stack.set($inst.d + $sb, val);
    }};
}

macro_rules! shift_op_assign {
    ($stack:ident, $op:tt, $inst:expr, $sb:expr, $consts:expr) => {{
        let right = $stack
            .read($inst.s1, $sb, $consts)
            .data()
            .cast_copyable($inst.t1, ValueType::Uint32);
        let d = $stack.get_data_mut($inst.s0 + $sb);
        *d = d.$op(right.as_uint32(), $inst.t0);
    }};
}

macro_rules! unary_op {
    ($stack:expr, $op:tt, $inst:expr, $sb:expr, $consts:expr) => {{
        let vdata = $stack.read($inst.s0, $sb, $consts).data().$op($inst.t0);
        let val = GosValue::new($inst.t0, vdata);
        $stack.set($inst.d + $sb, val);
    }};
}

#[derive(Debug)]
pub struct ByteCode {
    pub objects: VMObjects,
    pub consts: Vec<GosValue>,
    /// For calling method via interfaces
    pub ifaces: Vec<(Meta, Vec<Binding4Runtime>)>,
    /// For embedded fields of structs
    pub indices: Vec<Vec<OpIndex>>,
    pub entry: FunctionKey,
}

impl ByteCode {
    pub fn new(
        objects: VMObjects,
        consts: Vec<GosValue>,
        ifaces: Vec<(Meta, Vec<IfaceBinding>)>,
        indices: Vec<Vec<OpIndex>>,
        entry: FunctionKey,
    ) -> ByteCode {
        let ifaces = ifaces
            .into_iter()
            .map(|(ms, binding)| {
                let binding = binding.into_iter().map(|x| x.into()).collect();
                (ms, binding)
            })
            .collect();
        ByteCode {
            objects,
            consts,
            ifaces,
            indices,
            entry,
        }
    }
}

#[derive(Clone, Debug)]
struct Referers {
    typ: ValueType,
    weaks: Vec<WeakUpValue>,
}

#[derive(Clone, Debug)]
struct CallFrame {
    closure: ClosureObj,
    pc: OpIndex,
    stack_base: OpIndex,
    var_ptrs: Option<Vec<UpValue>>,
    // closures that have upvalues pointing to this frame
    referred_by: Option<HashMap<OpIndex, Referers>>,

    defer_stack: Option<Vec<DeferredCall>>,
}

impl CallFrame {
    fn with_closure(c: ClosureObj, sbase: OpIndex) -> CallFrame {
        CallFrame {
            closure: c,
            pc: 0,
            stack_base: sbase,
            var_ptrs: None,
            referred_by: None,
            defer_stack: None,
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
        self.closure.as_gos().func
    }

    #[inline]
    fn closure(&self) -> &ClosureObj {
        &self.closure
    }

    #[inline]
    fn func_val<'a>(&self, objs: &'a VMObjects) -> &'a FunctionVal {
        let fkey = self.func();
        &objs.functions[fkey]
    }

    #[inline]
    fn on_drop(&mut self, stack: &Stack) {
        if let Some(referred) = &self.referred_by {
            for (ind, referrers) in referred {
                if referrers.weaks.len() == 0 {
                    continue;
                }
                let val = stack.get(self.stack_base + *ind);
                for weak in referrers.weaks.iter() {
                    if let Some(uv) = weak.upgrade() {
                        uv.close(val.clone());
                    }
                }
            }
        }
    }
}

#[derive(Clone, Debug)]
struct DeferredCall {
    frame: CallFrame,
    vec: Vec<GosValue>,
}

#[derive(Debug)]
enum Result {
    Continue,
    End,
}

#[derive(Debug)]
struct PanicData {
    msg: GosValue,
    call_stack: Vec<(FunctionKey, OpIndex)>,
}

impl PanicData {
    fn new(m: GosValue) -> PanicData {
        PanicData {
            msg: m,
            call_stack: vec![],
        }
    }
}

#[derive(Clone)]
struct Context<'a> {
    exec: Rc<LocalExecutor<'a>>,
    code: &'a ByteCode,
    gcv: &'a GcoVec,
    ffi_factory: &'a FfiFactory,
    fs: Option<&'a FileSet>,
    next_id: Cell<usize>,
}

impl<'a> Context<'a> {
    fn new(
        exec: Rc<LocalExecutor<'a>>,
        code: &'a ByteCode,
        gcv: &'a GcoVec,
        ffi_factory: &'a FfiFactory,
        fs: Option<&'a FileSet>,
    ) -> Context<'a> {
        Context {
            exec,
            code,
            gcv,
            ffi_factory,
            fs,
            next_id: Cell::new(0),
        }
    }

    fn new_entry_frame(&self, entry: FunctionKey) -> CallFrame {
        let cls = GosValue::new_closure_static(entry, &self.code.objects.functions);
        CallFrame::with_closure(cls.as_closure().unwrap().0.clone(), 0)
    }

    fn spawn_fiber(&self, stack: Stack, first_frame: CallFrame) {
        let mut f = Fiber::new(self.clone(), stack, first_frame);
        self.exec
            .spawn(async move {
                // let parent fiber go first
                future::yield_now().await;
                f.main_loop().await;
            })
            .detach();
    }
}

pub struct Fiber<'a> {
    stack: Rc<RefCell<Stack>>,
    rstack: RangeStack,
    frames: Vec<CallFrame>,
    next_frames: Vec<CallFrame>,
    context: Context<'a>,
    id: usize,
}

impl<'a> Fiber<'a> {
    fn new(c: Context<'a>, stack: Stack, first_frame: CallFrame) -> Fiber<'a> {
        let id = c.next_id.get();
        c.next_id.set(id + 1);
        Fiber {
            stack: Rc::new(RefCell::new(stack)),
            rstack: RangeStack::new(),
            frames: vec![first_frame],
            next_frames: Vec::new(),
            context: c,
            id: id,
        }
    }

    async fn main_loop(&mut self) {
        let ctx = &self.context;
        let gcv = ctx.gcv;
        let objs: &VMObjects = &ctx.code.objects;
        let consts = &ctx.code.consts;
        let s_meta: &StaticMeta = &objs.s_meta;
        let ifaces = &ctx.code.ifaces;
        let indices = &ctx.code.indices;
        let mut frame_height = self.frames.len();
        let fr = self.frames.last().unwrap();
        let mut func = &objs.functions[fr.func()];
        let mut sb = fr.stack_base;

        let mut stack_mut_ref = self.stack.borrow_mut();
        let mut stack: &mut Stack = &mut stack_mut_ref;
        // allocate local variables
        stack.set_vec(0, func.local_zeros.clone());

        let mut code = &func.code;

        let mut total_inst = 0;
        //let mut stats: HashMap<Opcode, usize> = HashMap::new();
        loop {
            let mut frame = self.frames.last_mut().unwrap();
            let mut result: Result = Result::Continue;
            let mut panic: Option<PanicData> = None;
            let yield_unit = 1024;
            for _ in 0..yield_unit {
                let inst = &code[frame.pc as usize];
                let inst_op = inst.op0;
                total_inst += 1;
                //stats.entry(*inst).and_modify(|e| *e += 1).or_insert(1);
                frame.pc += 1;
                //dbg!(inst);
                //dbg!(inst_op);
                match inst_op {
                    // desc: local
                    // s0: local/const
                    Opcode::ASSIGN => {
                        //dbg!(sb + inst.d, stack.read(inst.s0, sb, consts), &stack);
                        stack.set(
                            sb + inst.d,
                            stack.read(inst.s0, sb, consts).copy_semantic(gcv),
                        )
                    }
                    // desc: local
                    // s0: slice
                    // s1: index
                    Opcode::LOAD_SLICE => {
                        let slice = stack.read(inst.s0, sb, consts);
                        let index = stack.read(inst.s1, sb, consts).as_index();
                        match slice.slice_array_equivalent(index) {
                            Ok((array, i)) => match array.dispatcher_a_s().array_get(&array, i) {
                                Ok(val) => stack.set(sb + inst.d, val),
                                Err(e) => go_panic_str!(panic, &e, frame, code),
                            },
                            Err(e) => go_panic_str!(panic, &e, frame, code),
                        }
                    }
                    // desc: slice
                    // s0: index
                    // s1: value
                    Opcode::STORE_SLICE => {
                        let dest = stack.read(inst.d, sb, consts);
                        let index = stack.read(inst.s0, sb, consts).as_index();
                        match dest.slice_array_equivalent(index) {
                            Ok((array, i)) => match inst.op1 {
                                Opcode::VOID => {
                                    let val = stack.read(inst.s1, sb, consts).copy_semantic(gcv);
                                    let result = array.dispatcher_a_s().array_set(&array, &val, i);
                                    panic_if_err!(result, panic, frame, code);
                                }
                                _ => match array.dispatcher_a_s().array_get(&array, i) {
                                    Ok(old) => {
                                        let val = stack.read_and_op(
                                            old.data(),
                                            inst.t0,
                                            inst.op1,
                                            inst.s1,
                                            sb,
                                            &consts,
                                        );
                                        let result =
                                            array.dispatcher_a_s().array_set(&array, &val, i);
                                        panic_if_err!(result, panic, frame, code);
                                    }
                                    Err(e) => go_panic_str!(panic, &e, frame, code),
                                },
                            },
                            Err(e) => go_panic_str!(panic, &e, frame, code),
                        }
                    }
                    // desc: local
                    // s0: array
                    // s1: index
                    Opcode::LOAD_ARRAY => {
                        let array = stack.read(inst.s0, sb, consts);
                        let index = stack.read(inst.s1, sb, consts).as_index();
                        match array.dispatcher_a_s().array_get(&array, index) {
                            Ok(val) => stack.set(inst.d + sb, val),
                            Err(e) => go_panic_str!(panic, &e, frame, code),
                        }
                    }
                    // desc: array
                    // s0: index
                    // s1: value
                    Opcode::STORE_ARRAY => {
                        let array = stack.read(inst.d, sb, consts);
                        let index = stack.read(inst.s0, sb, consts).as_index();
                        match inst.op1 {
                            Opcode::VOID => {
                                let val = stack.read(inst.s0, sb, consts).copy_semantic(gcv);
                                let result = array.dispatcher_a_s().array_set(&array, &val, index);
                                panic_if_err!(result, panic, frame, code);
                            }
                            _ => match array.dispatcher_a_s().array_get(&array, index) {
                                Ok(old) => {
                                    let val = stack.read_and_op(
                                        old.data(),
                                        inst.t0,
                                        inst.op1,
                                        inst.s1,
                                        sb,
                                        &consts,
                                    );
                                    let result =
                                        array.dispatcher_a_s().array_set(&array, &val, index);
                                    panic_if_err!(result, panic, frame, code);
                                }
                                Err(e) => go_panic_str!(panic, &e, frame, code),
                            },
                        }
                    }
                    // inst.d: local
                    // inst_ex.d: local
                    // inst.s0: map
                    // inst.s1: key
                    // inst_ex.s0: zero_val
                    Opcode::LOAD_MAP => {
                        let inst_ex = &code[frame.pc as usize];
                        frame.pc += 1;
                        let map = stack.read(inst.s0, sb, consts);
                        let key = stack.read(inst.s1, sb, consts);
                        let val = match map.as_map() {
                            Some(map) => map.0.get(&key),
                            None => None,
                        };
                        let (v, ok) = match val {
                            Some(v) => (v, true),
                            None => (stack.read(inst_ex.s0, sb, consts).copy_semantic(gcv), false),
                        };
                        stack.set(inst.d + sb, v);
                        if inst.t1 == ValueType::FlagB {
                            stack.set(inst_ex.d + sb, GosValue::new_bool(ok));
                        }
                    }
                    // desc: map
                    // s0: index
                    // s1: value
                    // inst_ex.s0: zero_val
                    Opcode::STORE_MAP => {
                        let inst_ex = &code[frame.pc as usize];
                        frame.pc += 1;
                        let dest = stack.read(inst.d, sb, consts);
                        match dest.as_some_map() {
                            Ok(map) => {
                                let key = stack.read(inst.s0, sb, consts);
                                match inst.op1 {
                                    Opcode::VOID => {
                                        let val =
                                            stack.read(inst.s1, sb, consts).copy_semantic(gcv);
                                        map.0.insert(key.clone(), val);
                                    }
                                    _ => {
                                        let old = match map.0.get(&key) {
                                            Some(v) => v,
                                            None => stack.read(inst_ex.s0, sb, consts).clone(),
                                        };
                                        frame.pc += 1;
                                        let val = stack.read_and_op(
                                            old.data(),
                                            inst.t0,
                                            inst.op1,
                                            inst.s1,
                                            sb,
                                            &consts,
                                        );
                                        map.0.insert(key.clone(), val);
                                    }
                                }
                            }
                            Err(e) => go_panic_str!(panic, &e, frame, code),
                        }
                    }
                    // desc: local
                    // s0: struct
                    // s1: index
                    Opcode::LOAD_STRUCT => {
                        let struct_ = stack.read(inst.s0, sb, consts);
                        let val = struct_.as_struct().0.borrow_fields()[inst.s1 as usize].clone();
                        stack.set(inst.d + sb, val);
                    }
                    // desc: struct
                    // s0: index
                    // s1: value
                    Opcode::STORE_STRUCT => {
                        let dest = stack.read(inst.d, sb, consts);
                        match inst.op1 {
                            Opcode::VOID => {
                                let val = stack.read(inst.s1, sb, consts).copy_semantic(gcv);
                                dest.as_struct().0.borrow_fields_mut()[inst.s0 as usize] = val;
                            }
                            _ => {
                                let old = &dest.as_struct().0.borrow_fields()[inst.s0 as usize];
                                let val = stack.read_and_op(
                                    old.data(),
                                    inst.t0,
                                    inst.op1,
                                    inst.s1,
                                    sb,
                                    &consts,
                                );
                                dest.as_struct().0.borrow_fields_mut()[inst.s0 as usize] = val;
                            }
                        }
                    }
                    // desc: local
                    // s0: struct
                    // s1: index of indices
                    Opcode::LOAD_STRUCT_EMBEDDED => {
                        let src = stack.read(inst.s0, sb, consts);
                        let (struct_, index) = get_struct_and_index(
                            src.clone(),
                            &indices[inst.s1 as usize],
                            stack,
                            objs,
                        );
                        match struct_ {
                            Ok(s) => {
                                let val = s.as_struct().0.borrow_fields()[index].clone();
                                stack.set(inst.d + sb, val);
                            }
                            Err(e) => go_panic_str!(panic, &e, frame, code),
                        }
                    }
                    // desc: struct
                    // s0: index of indices
                    // s1: value
                    Opcode::STORE_STRUCT_EMBEDDED => {
                        let dest = stack.read(inst.d, sb, consts);
                        let (struct_, index) = get_struct_and_index(
                            dest.clone(),
                            &indices[inst.s0 as usize],
                            stack,
                            objs,
                        );
                        match struct_ {
                            Ok(s) => match inst.op1 {
                                Opcode::VOID => {
                                    let val = stack.read(inst.s1, sb, consts).copy_semantic(gcv);
                                    s.as_struct().0.borrow_fields_mut()[index] = val;
                                }
                                _ => {
                                    let old = &s.as_struct().0.borrow_fields()[index as usize];
                                    let val = stack.read_and_op(
                                        old.data(),
                                        inst.t0,
                                        inst.op1,
                                        inst.s1,
                                        sb,
                                        &consts,
                                    );
                                    s.as_struct().0.borrow_fields_mut()[index as usize] = val;
                                }
                            },
                            Err(e) => go_panic_str!(panic, &e, frame, code),
                        }
                    }
                    // desc: local
                    // s0: package
                    // s1: index
                    Opcode::LOAD_PKG => {
                        let src = stack.read(inst.s0, sb, consts);
                        let index = inst.s1;
                        let pkg = &objs.packages[*src.as_package()];
                        let val = pkg.member(index).clone();
                        stack.set(inst.d + sb, val);
                    }
                    // desc: package
                    // s0: index
                    // s1: value
                    Opcode::STORE_PKG => {
                        let dest = stack.read(inst.d, sb, consts);
                        let index = inst.s0;

                        let pkg = &objs.packages[*dest.as_package()];
                        match inst.op1 {
                            Opcode::VOID => {
                                let val = stack.read(inst.s1, sb, consts).copy_semantic(gcv);
                                *pkg.member_mut(index) = val;
                            }
                            _ => {
                                let mut old = pkg.member_mut(index);
                                let val = stack.read_and_op(
                                    old.data(),
                                    inst.t0,
                                    inst.op1,
                                    inst.s1,
                                    sb,
                                    &consts,
                                );
                                *old = val;
                            }
                        }
                    }
                    // desc: local
                    // s0: pointer
                    Opcode::LOAD_POINTER => {
                        let src = stack.read(inst.s0, sb, consts);
                        match src.as_some_pointer() {
                            Ok(p) => match p.deref(stack, &objs.packages) {
                                Ok(val) => stack.set(inst.d + sb, val),
                                Err(e) => go_panic_str!(panic, &e, frame, code),
                            },
                            Err(e) => go_panic_str!(panic, &e, frame, code),
                        }
                    }
                    // desc: pointer
                    // s0: value
                    Opcode::STORE_POINTER => {
                        let dest = stack.read(inst.d, sb, consts).clone();
                        let result = dest.as_some_pointer().and_then(|p| {
                            let val = match inst.op1 {
                                Opcode::VOID => stack.read(inst.s0, sb, consts).copy_semantic(gcv),
                                _ => {
                                    let old = p.deref(stack, &objs.packages)?;
                                    stack.read_and_op(
                                        old.data(),
                                        inst.t0,
                                        inst.op1,
                                        inst.s1,
                                        sb,
                                        &consts,
                                    )
                                }
                            };
                            match p {
                                PointerObj::UpVal(uv) => {
                                    uv.set_value(val, stack);
                                    Ok(())
                                }
                                PointerObj::SliceMember(s, index) => {
                                    let index = *index as usize;
                                    let (array, index) = s.slice_array_equivalent(index)?;
                                    array.dispatcher_a_s().array_set(&array, &val, index)
                                }
                                PointerObj::StructField(s, index) => {
                                    s.as_struct().0.borrow_fields_mut()[*index as usize] = val;
                                    Ok(())
                                }
                                PointerObj::PkgMember(p, index) => {
                                    let pkg = &objs.packages[*p];
                                    *pkg.member_mut(*index) = val;
                                    Ok(())
                                }
                            }
                        });
                        panic_if_err!(result, panic, frame, code);
                    }
                    // desc: local
                    // s0: upvalue
                    Opcode::LOAD_UP_VALUE => {
                        let uvs = frame.var_ptrs.as_ref().unwrap();
                        let val = uvs[inst.s0 as usize].value(stack).into_owned();
                        stack.set(inst.d + sb, val);
                    }
                    Opcode::STORE_UP_VALUE => {
                        let uvs = frame.var_ptrs.as_ref().unwrap();
                        let uv = &uvs[inst.d as usize];
                        match inst.op1 {
                            Opcode::VOID => {
                                let val = stack.read(inst.s0, sb, consts).copy_semantic(gcv);
                                uv.set_value(val, stack);
                            }
                            _ => {
                                let old = uv.value(stack);
                                let val = stack.read_and_op(
                                    old.data(),
                                    inst.t0,
                                    inst.op1,
                                    inst.s1,
                                    sb,
                                    &consts,
                                );
                                uv.set_value(val, stack);
                            }
                        }
                    }
                    Opcode::ADD => binary_op!(stack, binary_op_add, inst, sb, consts),
                    Opcode::SUB => binary_op!(stack, binary_op_sub, inst, sb, consts),
                    Opcode::MUL => binary_op!(stack, binary_op_mul, inst, sb, consts),
                    Opcode::QUO => binary_op!(stack, binary_op_quo, inst, sb, consts),
                    Opcode::REM => binary_op!(stack, binary_op_rem, inst, sb, consts),
                    Opcode::AND => binary_op!(stack, binary_op_and, inst, sb, consts),
                    Opcode::OR => binary_op!(stack, binary_op_or, inst, sb, consts),
                    Opcode::XOR => binary_op!(stack, binary_op_xor, inst, sb, consts),
                    Opcode::AND_NOT => binary_op!(stack, binary_op_and_not, inst, sb, consts),
                    Opcode::SHL => shift_op!(stack, binary_op_shl, inst, sb, consts),
                    Opcode::SHR => shift_op!(stack, binary_op_shr, inst, sb, consts),
                    Opcode::ADD_ASSIGN => binary_op_assign!(stack, binary_op_add, inst, sb, consts),
                    Opcode::SUB_ASSIGN => binary_op_assign!(stack, binary_op_sub, inst, sb, consts),
                    Opcode::MUL_ASSIGN => binary_op_assign!(stack, binary_op_mul, inst, sb, consts),
                    Opcode::QUO_ASSIGN => binary_op_assign!(stack, binary_op_quo, inst, sb, consts),
                    Opcode::REM_ASSIGN => binary_op_assign!(stack, binary_op_rem, inst, sb, consts),
                    Opcode::AND_ASSIGN => binary_op_assign!(stack, binary_op_and, inst, sb, consts),
                    Opcode::OR_ASSIGN => binary_op_assign!(stack, binary_op_or, inst, sb, consts),
                    Opcode::XOR_ASSIGN => binary_op_assign!(stack, binary_op_xor, inst, sb, consts),
                    Opcode::AND_NOT_ASSIGN => {
                        binary_op_assign!(stack, binary_op_and_not, inst, sb, consts)
                    }
                    Opcode::SHL_ASSIGN => shift_op_assign!(stack, binary_op_shl, inst, sb, consts),
                    Opcode::SHR_ASSIGN => shift_op_assign!(stack, binary_op_shr, inst, sb, consts),
                    Opcode::INC => unsafe {
                        stack.get_mut(inst.d + sb).data_mut().inc(inst.t0);
                    },
                    Opcode::DEC => unsafe {
                        stack.get_mut(inst.d + sb).data_mut().dec(inst.t0);
                    },
                    Opcode::UNARY_SUB => unary_op!(stack, unary_negate, inst, sb, consts),
                    Opcode::UNARY_XOR => unary_op!(stack, unary_xor, inst, sb, consts),
                    Opcode::NOT => unary_op!(stack, logical_not, inst, sb, consts),
                    Opcode::EQL => {
                        let a = stack.read(inst.s0, sb, consts);
                        let b = stack.read(inst.s1, sb, consts);
                        let eq = if inst.t0.copyable() && inst.t0 == inst.t1 {
                            a.data().compare_eql(b.data(), inst.t0)
                        } else {
                            a.eq(b)
                        };
                        stack.set(inst.d + sb, GosValue::new_bool(eq));
                    }
                    Opcode::NEQ => {
                        let a = stack.read(inst.s0, sb, consts);
                        let b = stack.read(inst.s1, sb, consts);
                        let neq = if inst.t0.copyable() {
                            a.data().compare_neq(b.data(), inst.t0)
                        } else {
                            !a.eq(b)
                        };
                        stack.set(inst.d + sb, GosValue::new_bool(neq));
                    }
                    Opcode::LSS => {
                        let a = stack.read(inst.s0, sb, consts);
                        let b = stack.read(inst.s1, sb, consts);
                        let lss = if inst.t0.copyable() {
                            a.data().compare_lss(b.data(), inst.t0)
                        } else {
                            a.cmp(b) == Ordering::Less
                        };
                        stack.set(inst.d + sb, GosValue::new_bool(lss));
                    }
                    Opcode::GTR => {
                        let a = stack.read(inst.s0, sb, consts);
                        let b = stack.read(inst.s1, sb, consts);
                        let gtr = if inst.t0.copyable() {
                            a.data().compare_gtr(b.data(), inst.t0)
                        } else {
                            a.cmp(b) == Ordering::Greater
                        };
                        stack.set(inst.d + sb, GosValue::new_bool(gtr));
                    }
                    Opcode::LEQ => {
                        let a = stack.read(inst.s0, sb, consts);
                        let b = stack.read(inst.s1, sb, consts);
                        let leq = if inst.t0.copyable() {
                            a.data().compare_leq(b.data(), inst.t0)
                        } else {
                            a.cmp(b) != Ordering::Greater
                        };
                        stack.set(inst.d + sb, GosValue::new_bool(leq));
                    }
                    Opcode::GEQ => {
                        let a = stack.read(inst.s0, sb, consts);
                        let b = stack.read(inst.s1, sb, consts);
                        let geq = if inst.t0.copyable() {
                            a.data().compare_geq(b.data(), inst.t0)
                        } else {
                            a.cmp(b) != Ordering::Less
                        };
                        stack.set(inst.d + sb, GosValue::new_bool(geq));
                    }
                    Opcode::REF => {
                        let val = stack.read(inst.s0, sb, consts);
                        let boxed = PointerObj::new_closed_up_value(&val);
                        stack.set(inst.d + sb, GosValue::new_pointer(boxed));
                    }
                    Opcode::REF_UPVALUE => {
                        let uvs = frame.var_ptrs.as_ref().unwrap();
                        let upvalue = uvs[inst.s0 as usize].clone();
                        stack.set(
                            inst.d + sb,
                            GosValue::new_pointer(PointerObj::UpVal(upvalue.clone())),
                        );
                    }
                    Opcode::REF_SLICE_MEMBER => {
                        let arr_or_slice = stack.read(inst.s0, sb, consts).clone();
                        match PointerObj::new_slice_member(arr_or_slice, inst.s1, inst.t0, inst.t1)
                        {
                            Ok(p) => stack.set(inst.d + sb, GosValue::new_pointer(p)),
                            Err(e) => {
                                go_panic_str!(panic, &e, frame, code)
                            }
                        }
                    }
                    Opcode::REF_STRUCT_FIELD => {
                        let struct_ = stack.read(inst.s0, sb, consts).clone();
                        stack.set(
                            inst.d + sb,
                            GosValue::new_pointer(PointerObj::StructField(struct_, inst.s1)),
                        );
                    }
                    Opcode::REF_STRUCT_EMBEDDED_FIELD => {
                        let src = stack.read(inst.s0, sb, consts);
                        let (struct_, index) = get_struct_and_index(
                            src.clone(),
                            &indices[inst.s1 as usize],
                            stack,
                            objs,
                        );
                        match struct_ {
                            Ok(target) => {
                                stack.set(
                                    inst.d + sb,
                                    GosValue::new_pointer(PointerObj::StructField(
                                        target,
                                        index as OpIndex,
                                    )),
                                );
                            }
                            Err(e) => go_panic_str!(panic, &e, frame, code),
                        }
                    }
                    Opcode::REF_PKG_MEMBER => {
                        let pkg = *stack.read(inst.s0, sb, consts).as_package();
                        stack.set(
                            inst.d + sb,
                            GosValue::new_pointer(PointerObj::PkgMember(pkg, inst.s1)),
                        );
                    }
                    Opcode::SEND => {
                        let chan = stack.read(inst.s0, sb, consts).as_channel().cloned();
                        let val = stack.read(inst.s1, sb, consts).clone();
                        drop(stack_mut_ref);
                        let re = match chan {
                            Some(c) => c.send(&val).await,
                            None => loop {
                                future::yield_now().await;
                            },
                        };
                        restore_stack_ref!(self, stack, stack_mut_ref);
                        panic_if_err!(re, panic, frame, code);
                    }
                    Opcode::RECV => {
                        match stack.read(inst.s0, sb, consts).as_channel().cloned() {
                            Some(chan) => {
                                drop(stack_mut_ref);
                                let val = chan.recv().await;
                                restore_stack_ref!(self, stack, stack_mut_ref);
                                let (unwrapped, ok) = unwrap_recv_val!(chan, val, gcv);
                                stack.set(inst.d + sb, unwrapped);
                                if inst.t1 == ValueType::FlagB {
                                    stack.set(inst.s1 + sb, GosValue::new_bool(ok));
                                }
                            }
                            None => loop {
                                future::yield_now().await;
                            },
                        };
                    }
                    // d: closure
                    // s0: next stack base
                    // s1: param count
                    Opcode::PRE_CALL => {
                        let cls = stack
                            .read(inst.d, sb, consts)
                            .as_closure()
                            .unwrap()
                            .0
                            .clone();
                        let next_sb = sb + inst.s0;
                        match &cls {
                            ClosureObj::Gos(gosc) => {
                                let next_func = &objs.functions[gosc.func];
                                let mut returns_recv = next_func.ret_zeros.clone();
                                if let Some(r) = &gosc.recv {
                                    // push receiver on stack as the first parameter
                                    // don't call copy_semantic because BIND_METHOD did it already
                                    returns_recv.push(r.clone());
                                }
                                let param_count = inst.s1;
                                stack.set_min_size(
                                    next_sb as usize + returns_recv.len() + param_count as usize,
                                );
                                stack.set_vec(next_sb, returns_recv);
                            }
                            _ => {}
                        }
                        let next_frame = CallFrame::with_closure(cls, next_sb);
                        self.next_frames.push(next_frame);
                    }
                    Opcode::PACK_VARIADIC => {
                        let v = stack.move_vec(inst.s0 + sb, inst.s1 + sb);
                        let val = GosValue::slice_with_data(v, inst.t0, gcv);
                        stack.set(inst.d + sb, val);
                    }
                    // t0: call style
                    Opcode::CALL => {
                        let mut nframe = self.next_frames.pop().unwrap();
                        let cls = nframe.closure().clone();
                        let call_style = inst.t0;
                        match cls {
                            ClosureObj::Gos(gosc) => {
                                let nfunc = &objs.functions[gosc.func];
                                if let Some(uvs) = &gosc.uvs {
                                    let mut ptrs: Vec<UpValue> =
                                        Vec::with_capacity(nfunc.up_ptrs.len());
                                    for (i, p) in nfunc.up_ptrs.iter().enumerate() {
                                        ptrs.push(if p.is_up_value {
                                            uvs[&i].clone()
                                        } else {
                                            // local pointers
                                            let uv = UpValue::new(p.clone_with_stack(
                                                Rc::downgrade(&self.stack),
                                                nframe.stack_base as OpIndex,
                                            ));
                                            nframe.add_referred_by(p.index, p.typ, &uv);
                                            uv
                                        });
                                    }
                                    nframe.var_ptrs = Some(ptrs);
                                }
                                match call_style {
                                    ValueType::FlagA => {
                                        // default call
                                        self.frames.push(nframe);
                                        frame_height += 1;
                                        frame = self.frames.last_mut().unwrap();
                                        func = nfunc;
                                        sb = frame.stack_base;
                                        code = &func.code;
                                        //dbg!(&consts);
                                        //dbg!(&code);
                                        //dbg!(&stack);
                                    }
                                    ValueType::FlagB => {
                                        // goroutine
                                        let begin = nframe.stack_base;
                                        let end = begin + nfunc.param_types().len() as OpIndex;
                                        let vec = stack.move_vec(begin, end);
                                        let nstack = Stack::with_vec(vec);
                                        nframe.stack_base = 0;
                                        self.context.spawn_fiber(nstack, nframe);
                                    }
                                    ValueType::FlagC => {
                                        let begin = nframe.stack_base;
                                        let end = begin + nfunc.param_types().len() as OpIndex;
                                        let vec = stack.move_vec(begin, end);
                                        let deferred = DeferredCall {
                                            frame: nframe,
                                            vec: vec,
                                        };
                                        frame.defer_stack.get_or_insert(vec![]).push(deferred);
                                    }
                                    _ => unreachable!(),
                                }
                            }
                            ClosureObj::Ffi(ffic) => {
                                let ptypes = &objs.metas[ffic.meta.key].as_signature().params_type;
                                let begin = nframe.stack_base;
                                let end = begin + ptypes.len() as OpIndex;
                                let params = stack.move_vec(begin, end);
                                // release stack so that code in ffi can yield
                                drop(stack_mut_ref);
                                let returns = {
                                    let mut ctx = FfiCallCtx {
                                        func_name: &ffic.func_name,
                                        vm_objs: objs,
                                        stack: &mut self.stack.borrow_mut(),
                                        gcv: gcv,
                                        statics: self.context.ffi_factory.statics(),
                                    };
                                    let fut = ffic.ffi.call(&mut ctx, params);
                                    fut.await
                                };
                                restore_stack_ref!(self, stack, stack_mut_ref);
                                match returns {
                                    Ok(result) => stack.set_vec(begin, result),
                                    Err(e) => {
                                        go_panic_str!(panic, &e, frame, code);
                                    }
                                }
                            }
                        }
                    }
                    Opcode::RETURN => {
                        //dbg!(stack.len());
                        //for s in stack.iter() {
                        //    dbg!(GosValueDebug::new(&s, &objs));
                        //}

                        let clear_stack = match inst.t0 {
                            // default case
                            ValueType::FlagA => true,
                            // init_package func
                            ValueType::FlagB => {
                                let pkey = stack.read(inst.d, sb, consts).as_package();
                                let pkg = &objs.packages[*pkey];
                                // the var values left on the stack are for pkg members
                                let func = frame.func_val(objs);
                                let begin = sb;
                                let end = begin + func.stack_temp_types.len() as OpIndex;
                                pkg.init_vars(stack.move_vec(begin, end));
                                false
                            }
                            // func with deferred calls
                            ValueType::FlagC => {
                                if let Some(call) =
                                    frame.defer_stack.as_mut().map(|x| x.pop()).flatten()
                                {
                                    // run Opcode::RETURN to check if deferred_stack is empty
                                    frame.pc -= 1;

                                    let call_vec_len = call.vec.len() as OpIndex;
                                    let new_sb =
                                        sb + frame.func_val(objs).stack_temp_types.len() as OpIndex;
                                    stack.set_vec(new_sb, call.vec);
                                    let nframe = call.frame;

                                    self.frames.push(nframe);
                                    frame_height += 1;
                                    frame = self.frames.last_mut().unwrap();
                                    let fkey = frame.func();
                                    func = &objs.functions[fkey];
                                    sb = frame.stack_base;
                                    code = &func.code;
                                    let index = new_sb + call_vec_len;
                                    stack.set_vec(index, func.local_zeros.clone());
                                    continue;
                                }
                                true
                            }
                            _ => unreachable!(),
                        };

                        if clear_stack {
                            // println!(
                            //     "current line: {}",
                            //     self.context.fs.unwrap().position(
                            //         objs.functions[frame.func()].pos()[frame.pc - 1].unwrap()
                            //     )
                            // );

                            frame.on_drop(&stack);
                            let func = frame.func_val(objs);
                            let begin = sb + func.ret_count() as OpIndex;
                            let end = begin + func.stack_temp_types.len() as OpIndex;
                            stack.move_vec(begin, end);
                        }

                        drop(frame);
                        self.frames.pop();
                        frame_height -= 1;
                        if self.frames.is_empty() {
                            dbg!(total_inst);

                            result = Result::End;
                            break;
                        }
                        frame = self.frames.last_mut().unwrap();
                        sb = frame.stack_base;
                        // restore func, consts, code
                        func = &objs.functions[frame.func()];
                        code = &func.code;

                        if let Some(p) = &mut panic {
                            p.call_stack.push((frame.func(), frame.pc - 1));
                            frame.pc = code.len() as OpIndex - 1;
                        }
                    }
                    Opcode::JUMP => frame.pc += inst.d,
                    Opcode::JUMP_IF => {
                        if *stack.read(inst.s0, sb, consts).as_bool() {
                            frame.pc += inst.d;
                        }
                    }
                    Opcode::JUMP_IF_NOT => {
                        if !*stack.read(inst.s0, sb, consts).as_bool() {
                            frame.pc += inst.d;
                        }
                    }
                    Opcode::SWITCH => {
                        let t = inst.t0;
                        let a = stack.read(inst.s0, sb, consts);
                        let b = stack.read(inst.s1, sb, consts);
                        let ok = if t.copyable() {
                            a.data().compare_eql(b.data(), t)
                        } else if t != ValueType::Metadata {
                            a.eq(&b)
                        } else {
                            a.as_metadata().identical(b.as_metadata(), &objs.metas)
                        };
                        if ok {
                            frame.pc += inst.d;
                        }
                    }
                    Opcode::SELECT => {
                        let comm_count = inst.s0;
                        let has_default = inst.t0 == ValueType::FlagE;
                        let default_offset = has_default.then(|| inst.d);
                        let mut comms = Vec::with_capacity(comm_count as usize);
                        for _ in 0..comm_count {
                            let entry = &code[frame.pc as usize];
                            frame.pc += 1;
                            let chan = stack.read(entry.s0, sb, consts).clone();
                            let offset = entry.d;
                            let flag = entry.t0;
                            let typ = match &flag {
                                ValueType::FlagA => {
                                    let val = stack.read(entry.s1, sb, consts).copy_semantic(gcv);
                                    channel::SelectCommType::Send(val)
                                }
                                ValueType::FlagB | ValueType::FlagC | ValueType::FlagD => {
                                    channel::SelectCommType::Recv(flag, entry.s1)
                                }
                                _ => unreachable!(),
                            };
                            comms.push(channel::SelectComm { typ, chan, offset });
                        }
                        let selector = channel::Selector::new(comms, default_offset);

                        drop(stack_mut_ref);
                        let re = selector.select().await;
                        restore_stack_ref!(self, stack, stack_mut_ref);

                        match re {
                            Ok((i, val)) => {
                                let block_offset = if i >= selector.comms.len() {
                                    selector.default_offset.unwrap()
                                } else {
                                    let comm = &selector.comms[i];
                                    match comm.typ {
                                        channel::SelectCommType::Send(_) => {}
                                        channel::SelectCommType::Recv(flag, dst) => {
                                            let (unwrapped, ok) = unwrap_recv_val!(
                                                comm.chan.as_channel().as_ref().unwrap(),
                                                val,
                                                gcv
                                            );
                                            match flag {
                                                ValueType::FlagC => {
                                                    stack.set(dst + sb, unwrapped);
                                                }
                                                ValueType::FlagD => {
                                                    stack.set(dst + sb, unwrapped);
                                                    stack.set(dst + 1 + sb, GosValue::new_bool(ok));
                                                }
                                                _ => {}
                                            }
                                        }
                                    }
                                    comm.offset
                                };
                                // jump to the block
                                frame.pc += block_offset;
                            }
                            Err(e) => {
                                go_panic_str!(panic, &e, frame, code);
                            }
                        }
                    }
                    Opcode::RANGE_INIT => {
                        let target = stack.read(inst.s0, sb, consts);
                        let re = self.rstack.range_init(target, inst.t0, inst.t1);
                        panic_if_err!(re, panic, frame, code);
                    }
                    Opcode::RANGE => {
                        if self.rstack.range_body(
                            inst.t0,
                            inst.t1,
                            stack,
                            inst.s1 + sb,
                            inst.d + sb,
                        ) {
                            frame.pc += inst.s0;
                        }
                    }
                    // load user defined init function or jump 3 if failed
                    Opcode::LOAD_PKG_INIT_FUNC => {
                        let src = stack.read(inst.s0, sb, consts);
                        let index = *stack.read(inst.s1, sb, consts).as_int32();
                        let pkg = &objs.packages[*src.as_package()];
                        match pkg.get_init_func(index) {
                            Some(f) => {
                                stack.set(inst.d + sb, f.clone());
                                stack.set(inst.s1 + sb, GosValue::new_int32(index + 1));
                            }
                            None => {
                                frame.pc += 3;
                            }
                        }
                    }
                    Opcode::BIND_METHOD => {
                        let recv = stack.read(inst.s0, sb, consts).copy_semantic(gcv);
                        let func = *stack.read(inst.s1, sb, consts).as_function();
                        stack.set(
                            inst.d + sb,
                            GosValue::new_closure(
                                ClosureObj::new_gos(func, &objs.functions, Some(recv)),
                                gcv,
                            ),
                        );
                    }
                    Opcode::BIND_INTERFACE_METHOD => {
                        match stack.read(inst.s0, sb, consts).as_some_interface() {
                            Ok(iface) => {
                                match bind_iface_method(iface, inst.s1 as usize, stack, objs, gcv) {
                                    Ok(cls) => stack.set(inst.d + sb, cls),
                                    Err(e) => go_panic_str!(panic, &e, frame, code),
                                }
                            }
                            Err(e) => go_panic_str!(panic, &e, frame, code),
                        }
                    }
                    Opcode::CAST => {
                        let from_type = inst.t1;
                        let to_type = inst.t0;
                        let val = match to_type {
                            ValueType::UintPtr => match from_type {
                                ValueType::UnsafePtr => {
                                    let up =
                                        stack.read(inst.s0, sb, consts).as_unsafe_ptr().cloned();
                                    GosValue::new_uint_ptr(
                                        up.map_or(0, |x| x.as_rust_ptr() as *const () as usize),
                                    )
                                }
                                _ => stack
                                    .read(inst.s0, sb, consts)
                                    .cast_copyable(from_type, to_type),
                            },
                            _ if to_type.copyable() => stack
                                .read(inst.s0, sb, consts)
                                .cast_copyable(from_type, to_type),
                            ValueType::Interface => {
                                let binding = ifaces[inst.s1 as usize].clone();
                                let under = stack.read(inst.s0, sb, consts).copy_semantic(gcv);
                                GosValue::new_interface(InterfaceObj::with_value(
                                    under,
                                    Some(binding),
                                ))
                            }
                            ValueType::String => match from_type {
                                ValueType::Slice => match inst.op1_as_t() {
                                    ValueType::Int32 => {
                                        let s = match stack
                                            .read(inst.s0, sb, consts)
                                            .as_slice::<Elem32>()
                                        {
                                            Some(slice) => slice
                                                .0
                                                .as_rust_slice()
                                                .iter()
                                                .map(|x| char_from_i32(x.cell.get() as i32))
                                                .collect(),
                                            None => "".to_owned(),
                                        };
                                        GosValue::with_str(&s)
                                    }
                                    ValueType::Uint8 => {
                                        match stack.read(inst.s0, sb, consts).as_slice::<Elem8>() {
                                            Some(slice) => GosValue::new_string(slice.0.clone()),
                                            None => GosValue::with_str(""),
                                        }
                                    }
                                    _ => unreachable!(),
                                },
                                _ => {
                                    let val = stack
                                        .read(inst.s0, sb, consts)
                                        .cast_copyable(from_type, ValueType::Uint32);
                                    GosValue::with_str(&char_from_u32(*val.as_uint32()).to_string())
                                }
                            },
                            ValueType::Slice => {
                                let from = stack.read(inst.s0, sb, consts).as_string();
                                match inst.op1_as_t() {
                                    ValueType::Int32 => {
                                        let data = StrUtil::as_str(from)
                                            .chars()
                                            .map(|x| GosValue::new_int32(x as i32))
                                            .collect();
                                        GosValue::slice_with_data(data, inst.op1_as_t(), gcv)
                                    }
                                    ValueType::Uint8 => {
                                        GosValue::new_slice(from.clone(), ValueType::Uint8)
                                    }
                                    _ => unreachable!(),
                                }
                            }
                            ValueType::Pointer => match from_type {
                                ValueType::Pointer => stack.read(inst.s0, sb, consts).clone(),
                                ValueType::UnsafePtr => {
                                    match stack.read(inst.s0, sb, consts).as_unsafe_ptr() {
                                        Some(p) => {
                                            match p.ptr().as_any().downcast_ref::<PointerHandle>() {
                                                Some(h) => {
                                                    match h.ptr().cast(
                                                        inst.op1_as_t(),
                                                        &stack,
                                                        &objs.packages,
                                                    ) {
                                                        Ok(p) => GosValue::new_pointer(p),
                                                        Err(e) => {
                                                            go_panic_str!(panic, &e, frame, code);
                                                            continue;
                                                        }
                                                    }
                                                }
                                                None => {
                                                    go_panic_str!(panic, "only a unsafe-pointer cast from a pointer can be cast back to a pointer", frame, code);
                                                    continue;
                                                }
                                            }
                                        }
                                        None => GosValue::new_nil(ValueType::Pointer),
                                    }
                                }
                                _ => unimplemented!(),
                            },
                            ValueType::UnsafePtr => match from_type {
                                ValueType::Pointer => {
                                    PointerHandle::new(stack.read(inst.s0, sb, consts))
                                }
                                _ => unimplemented!(),
                            },
                            _ => {
                                dbg!(to_type);
                                unimplemented!()
                            }
                        };
                        stack.set(inst.d + sb, val);
                    }
                    Opcode::TYPE_ASSERT => {
                        let val = stack.read(inst.s0, sb, consts);
                        match type_assert(val, cst(consts, inst.s1), gcv, Some(&objs.metas)) {
                            Ok((val, ok)) => {
                                stack.set(inst.d + sb, val);
                                if inst.t1 == ValueType::FlagB {
                                    let inst_ex = &code[frame.pc as usize];
                                    frame.pc += 1;
                                    stack.set(inst_ex.d + sb, GosValue::new_bool(ok));
                                }
                            }
                            Err(e) => go_panic_str!(panic, &e, frame, code),
                        }
                    }
                    Opcode::TYPE => {
                        let iface_value = stack.read(inst.s0, sb, consts).clone();
                        let (val, meta) = match iface_value.as_interface() {
                            Some(iface) => match &iface as &InterfaceObj {
                                InterfaceObj::Gos(v, b) => {
                                    (v.copy_semantic(gcv), b.as_ref().unwrap().0)
                                }
                                _ => (iface_value.clone(), s_meta.none),
                            },
                            _ => (iface_value, s_meta.none),
                        };
                        let typ = meta.value_type(&objs.metas);
                        stack.set(inst.d + sb, GosValue::new_metadata(meta));
                        let option_count = inst.s1;
                        if option_count > 0 {
                            let mut index = None;
                            for i in 0..option_count {
                                let inst_data = &code[(frame.pc + i) as usize];
                                if inst_data.t0 == typ {
                                    index = Some(inst_data.d);
                                }
                            }
                            stack.set(index.unwrap() + sb, val);
                            frame.pc += option_count;
                        }
                    }
                    Opcode::IMPORT => {
                        let pkey = *stack.read(inst.s0, sb, consts).as_package();
                        if objs.packages[pkey].inited() {
                            frame.pc += inst.d
                        }
                    }
                    Opcode::SLICE => {
                        let inst_ex = &code[frame.pc as usize];
                        frame.pc += 1;
                        let s = stack.read(inst.s0, sb, consts);
                        let begin = *stack.read(inst.s1, sb, consts).as_int();
                        let end = *stack.read(inst_ex.s0, sb, consts).as_int();
                        let max = *stack.read(inst_ex.s1, sb, consts).as_int();
                        let result = match inst.t0 {
                            ValueType::Slice => s.dispatcher_a_s().slice_slice(s, begin, end, max),
                            ValueType::String => GosValue::slice_string(s, begin, end, max),
                            ValueType::Array => {
                                GosValue::slice_array(s.clone(), begin, end, inst.t1)
                            }
                            _ => unreachable!(),
                        };

                        match result {
                            Ok(v) => stack.set(inst.d + sb, v),
                            Err(e) => go_panic_str!(panic, &e, frame, code),
                        }
                    }
                    Opcode::CLOSURE => {
                        let func = cst(consts, inst.s0);
                        let mut val =
                            ClosureObj::new_gos(*func.as_function(), &objs.functions, None);
                        match &mut val {
                            ClosureObj::Gos(gos) => {
                                if let Some(uvs) = &mut gos.uvs {
                                    drop(frame);
                                    for (_, uv) in uvs.iter_mut() {
                                        let r: &mut UpValueState = &mut uv.inner.borrow_mut();
                                        if let UpValueState::Open(d) = r {
                                            // get frame index, and add_referred_by
                                            for i in 1..frame_height {
                                                let index = frame_height - i;
                                                if self.frames[index].func() == d.func {
                                                    let upframe = &mut self.frames[index];
                                                    d.stack = Rc::downgrade(&self.stack);
                                                    d.stack_base = upframe.stack_base as OpIndex;
                                                    upframe.add_referred_by(d.index, d.typ, uv);
                                                    // if not found, the upvalue is already closed, nothing to be done
                                                    break;
                                                }
                                            }
                                        }
                                    }
                                    frame = self.frames.last_mut().unwrap();
                                }
                            }
                            _ => {}
                        };
                        stack.set(inst.d + sb, GosValue::new_closure(val, gcv));
                    }
                    Opcode::LITERAL => {
                        let inst_ex = &code[frame.pc as usize];
                        frame.pc += 1;
                        let md = cst(consts, inst_ex.s0).as_metadata();

                        let begin = inst.s0 + sb;
                        let count = inst.s1;
                        let build_val = |m: &Meta| {
                            let zero_val = m.zero(&objs.metas, gcv);
                            let mut val = vec![];
                            let mut cur_index = -1;
                            for i in 0..count {
                                let index = *stack.get(begin + i * 2).as_int();
                                let elem = stack.get(begin + 1 + i * 2).clone();
                                if index < 0 {
                                    cur_index += 1;
                                } else {
                                    cur_index = index;
                                }
                                let gap = cur_index - (val.len() as isize);
                                if gap == 0 {
                                    val.push(elem);
                                } else if gap > 0 {
                                    for _ in 0..gap {
                                        val.push(zero_val.clone());
                                    }
                                    val.push(elem);
                                } else {
                                    val[cur_index as usize] = elem;
                                }
                            }
                            (val, zero_val.typ())
                        };
                        let new_val = match &objs.metas[md.key] {
                            MetadataType::Slice(m) => {
                                let (val, typ) = build_val(m);
                                GosValue::slice_with_data(val, typ, gcv)
                            }
                            MetadataType::Array(m, _) => {
                                let (val, typ) = build_val(m);
                                GosValue::array_with_data(val, typ, gcv)
                            }
                            MetadataType::Map(_, _) => {
                                let map_val = GosValue::new_map(gcv);
                                let map = map_val.as_map().unwrap();
                                for i in 0..count {
                                    let k = stack.get(begin + i * 2).clone();
                                    let v = stack.get(begin + 1 + i * 2).clone();
                                    map.0.insert(k, v);
                                }
                                map_val
                            }
                            MetadataType::Struct(_, _) => {
                                let struct_val = md.zero(&objs.metas, gcv);
                                {
                                    let fields = &mut struct_val.as_struct().0.borrow_fields_mut();
                                    for i in 0..count {
                                        let index = *stack.get(begin + i * 2).as_uint();
                                        fields[index] = stack.get(begin + 1 + i * 2).clone();
                                    }
                                }
                                struct_val
                            }
                            _ => unreachable!(),
                        };
                        stack.set(inst.d + sb, new_val);
                    }
                    Opcode::NEW => {
                        let md = stack.read(inst.s0, sb, consts).as_metadata();
                        let v = md.into_value_category().zero(&objs.metas, gcv);
                        let p = GosValue::new_pointer(PointerObj::UpVal(UpValue::new_closed(v)));
                        stack.set(inst.d + sb, p);
                    }
                    Opcode::MAKE => {
                        let md = stack.read(inst.s0, sb, consts).as_metadata();
                        let val = match md.mtype_unwraped(&objs.metas) {
                            MetadataType::Slice(vmeta) => {
                                let (cap, len) = match inst.t0 {
                                    // 3 args
                                    ValueType::FlagC => {
                                        let inst_ex = &code[frame.pc as usize];
                                        frame.pc += 1;
                                        (
                                            *stack.read(inst.s1, sb, consts).as_int() as usize,
                                            *stack.read(inst_ex.s0, sb, consts).as_int() as usize,
                                        )
                                    }
                                    // 2 args
                                    ValueType::FlagB => {
                                        let len =
                                            *stack.read(inst.s1, sb, consts).as_int() as usize;
                                        (len, len)
                                    }
                                    _ => unreachable!(),
                                };
                                let zero = vmeta.zero(&objs.metas, gcv);
                                GosValue::slice_with_size(len, cap, &zero, zero.typ(), gcv)
                            }
                            MetadataType::Map(_, _) => GosValue::new_map(gcv),
                            MetadataType::Channel(_, val_meta) => {
                                let cap = match inst.t0 {
                                    // 2 args
                                    ValueType::FlagB => {
                                        *stack.read(inst.s1, sb, consts).as_int() as usize
                                    }
                                    // 1 arg
                                    ValueType::FlagA => 0,
                                    _ => unreachable!(),
                                };
                                let zero = val_meta.zero(&objs.metas, gcv);
                                GosValue::new_channel(ChannelObj::new(cap, zero))
                            }
                            _ => unreachable!(),
                        };
                        stack.set(inst.d + sb, val);
                    }
                    Opcode::COMPLEX => {
                        // for the specs: For complex, the two arguments must be of the same
                        // floating-point type and the return type is the complex type with
                        // the corresponding floating-point constituents
                        let val = match inst.t0 {
                            ValueType::Float32 => {
                                let i = *stack.read(inst.s0, sb, consts).as_float32();
                                let r = *stack.read(inst.s1, sb, consts).as_float32();
                                GosValue::new_complex64(r, i)
                            }
                            ValueType::Float64 => {
                                let i = *stack.read(inst.s0, sb, consts).as_float64();
                                let r = *stack.read(inst.s1, sb, consts).as_float64();
                                GosValue::new_complex128(r, i)
                            }
                            _ => unreachable!(),
                        };
                        stack.set(inst.d + sb, val);
                    }
                    Opcode::REAL => {
                        let val = match inst.t0 {
                            ValueType::Complex64 => GosValue::new_float32(
                                stack.read(inst.s0, sb, consts).as_complex64().r,
                            ),
                            ValueType::Complex128 => GosValue::new_float64(
                                stack.read(inst.s1, sb, consts).as_complex128().r,
                            ),
                            _ => unreachable!(),
                        };
                        stack.set(inst.d + sb, val);
                    }
                    Opcode::IMAG => {
                        let val = match inst.t0 {
                            ValueType::Complex64 => GosValue::new_float32(
                                stack.read(inst.s0, sb, consts).as_complex64().i,
                            ),
                            ValueType::Complex128 => GosValue::new_float64(
                                stack.read(inst.s1, sb, consts).as_complex128().i,
                            ),
                            _ => unreachable!(),
                        };
                        stack.set(inst.d + sb, val);
                    }
                    Opcode::LEN => {
                        let l = stack.read(inst.s0, sb, consts).len();
                        stack.set(inst.d + sb, GosValue::new_int(l as isize));
                    }
                    Opcode::CAP => {
                        let l = stack.read(inst.s0, sb, consts).cap();
                        stack.set(inst.d + sb, GosValue::new_int(l as isize));
                    }
                    Opcode::APPEND => {
                        let a = stack.read(inst.s0, sb, consts).clone();
                        let b = if inst.t0 != ValueType::String {
                            stack.read(inst.s1, sb, consts).clone()
                        } else {
                            // special case, appending string as bytes
                            let s = stack.read(inst.s0, sb, consts).as_string();
                            let arr = GosValue::new_non_gc_array(
                                ArrayObj::with_raw_data(s.as_rust_slice().to_vec()),
                                ValueType::Uint8,
                            );
                            GosValue::slice_array(arr, 0, -1, ValueType::Uint8).unwrap()
                        };

                        match dispatcher_a_s_for(inst.t1).slice_append(a, b, gcv) {
                            Ok(slice) => stack.set(inst.d + sb, slice),
                            Err(e) => go_panic_str!(panic, &e, frame, code),
                        };
                    }
                    Opcode::COPY => {
                        let a = stack.read(inst.s0, sb, consts).clone();
                        let b = stack.read(inst.s1, sb, consts).clone();
                        let count = match inst.t0 {
                            ValueType::String => {
                                let string = b.as_string();
                                match a.as_slice::<Elem8>() {
                                    Some(s) => s.0.copy_from(&string),
                                    None => 0,
                                }
                            }
                            _ => dispatcher_a_s_for(inst.t1).slice_copy_from(a, b),
                        };
                        stack.set(inst.d + sb, GosValue::new_int(count as isize));
                    }
                    Opcode::DELETE => {
                        let map = stack.read(inst.s0, sb, consts);
                        let key = stack.read(inst.s1, sb, consts);
                        match map.as_map() {
                            Some(m) => m.0.delete(key),
                            None => {}
                        }
                    }
                    Opcode::CLOSE => match stack.read(inst.s0, sb, consts).as_channel() {
                        Some(c) => c.close(),
                        None => {}
                    },
                    Opcode::PANIC => {
                        let val = stack.read(inst.s0, sb, consts).clone();
                        go_panic!(panic, val, frame, code);
                    }
                    Opcode::RECOVER => {
                        let p = panic.take();
                        let val = p.map_or(GosValue::new_nil(ValueType::Void), |x| x.msg);
                        stack.set(inst.d + sb, val);
                    }
                    Opcode::ASSERT => {
                        let ok = *stack.read(inst.s0, sb, consts).as_bool();
                        if !ok {
                            go_panic_str!(panic, "Opcode::ASSERT: not true!", frame, code);
                        }
                    }
                    Opcode::FFI => {
                        let val = {
                            let itype = stack.read(inst.s0, sb, consts);
                            let name = stack.read(inst.s1, sb, consts);
                            let name_str = StrUtil::as_str(name.as_string());
                            match self.context.ffi_factory.create_by_name(&name_str) {
                                Ok(v) => {
                                    let meta = itype.as_metadata().underlying(&objs.metas).clone();
                                    GosValue::new_interface(InterfaceObj::Ffi(UnderlyingFfi::new(
                                        v, meta,
                                    )))
                                }
                                Err(e) => {
                                    go_panic_str!(panic, &e, frame, code);
                                    continue;
                                }
                            }
                        };
                        stack.set(inst.d + sb, val);
                    }
                    Opcode::VOID => unreachable!(),
                }
            } //yield unit
            match result {
                Result::End => {
                    if let Some(p) = panic {
                        println!("panic: {}", p.msg);
                        if let Some(files) = self.context.fs {
                            for (fkey, pc) in p.call_stack.iter() {
                                let func = &objs.functions[*fkey];
                                if let Some(p) = func.pos[*pc as usize] {
                                    println!("{}", files.position(p).unwrap_or(FilePos::null()));
                                } else {
                                    println!("<no debug info available>");
                                }
                            }
                        }

                        // a hack to make the test case fail
                        let iface = p.msg.as_interface().unwrap();
                        let val = iface.underlying_value().unwrap();
                        if val.typ() == ValueType::String
                            && StrUtil::as_str(val.as_string()).starts_with("Opcode::ASSERT")
                        {
                            panic!("ASSERT");
                        }
                    }
                    break;
                }
                Result::Continue => {
                    drop(stack_mut_ref);
                    future::yield_now().await;
                    restore_stack_ref!(self, stack, stack_mut_ref);
                }
            };
        } //loop

        gc(gcv);
    }
}

pub struct GosVM<'a> {
    code: ByteCode,
    gcv: GcoVec,
    ffi: &'a FfiFactory,
    fs: Option<&'a FileSet>,
}

impl<'a> GosVM<'a> {
    pub fn new(bc: ByteCode, ffi: &'a FfiFactory, fs: Option<&'a FileSet>) -> GosVM<'a> {
        GosVM {
            code: bc,
            gcv: GcoVec::new(),
            ffi: ffi,
            fs: fs,
        }
    }

    pub fn run(&self) {
        // Init array/slice dispatcher
        dispatcher_a_s_for(ValueType::Uint);

        let exec = Rc::new(LocalExecutor::new());
        let ctx = Context::new(exec.clone(), &self.code, &self.gcv, self.ffi, self.fs);
        let entry = ctx.new_entry_frame(self.code.entry);
        ctx.spawn_fiber(Stack::new(), entry);

        future::block_on(async {
            loop {
                if !exec.try_tick() {
                    break;
                }
            }
        });
    }
}

#[inline]
fn char_from_u32(u: u32) -> char {
    unsafe { char::from_u32_unchecked(u) }
}

#[inline]
fn char_from_i32(i: i32) -> char {
    unsafe { char::from_u32_unchecked(i as u32) }
}

#[inline]
fn deref_value(v: &GosValue, stack: &Stack, objs: &VMObjects) -> RuntimeResult<GosValue> {
    v.as_some_pointer()?.deref(stack, &objs.packages)
}

#[inline(always)]
fn cst(consts: &Vec<GosValue>, i: OpIndex) -> &GosValue {
    &consts[(-i - 1) as usize]
}

#[inline(always)]
fn type_assert(
    val: &GosValue,
    want_meta: &GosValue,
    gcv: &GcoVec,
    metas: Option<&MetadataObjs>,
) -> RuntimeResult<(GosValue, bool)> {
    match val.as_some_interface() {
        Ok(iface) => match &iface as &InterfaceObj {
            InterfaceObj::Gos(v, b) => {
                let meta = b.as_ref().unwrap().0;
                let want_meta = want_meta.as_metadata();
                if *want_meta == meta {
                    Ok((v.copy_semantic(gcv), true))
                } else {
                    if let Some(mobjs) = metas {
                        Ok((want_meta.zero(mobjs, gcv), false))
                    } else {
                        Err("interface conversion: wrong type".to_owned())
                    }
                }
            }
            InterfaceObj::Ffi(_) => Err("FFI interface do not support type assertion".to_owned()),
        },
        Err(e) => Err(e),
    }
}

#[inline(always)]
fn get_struct_and_index(
    val: GosValue,
    indices: &Vec<OpIndex>,
    stack: &mut Stack,
    objs: &VMObjects,
) -> (RuntimeResult<GosValue>, usize) {
    let (target, index) = {
        let val = get_embeded(val, &indices[..indices.len() - 1], stack, &objs.packages);
        (val, *indices.last().unwrap())
    };
    (
        match target {
            Ok(v) => match v.typ() {
                ValueType::Pointer => deref_value(&v, stack, objs),
                _ => Ok(v.clone()),
            },
            Err(e) => Err(e),
        },
        index as usize,
    )
}

#[inline]
pub fn get_embeded(
    val: GosValue,
    indices: &[OpIndex],
    stack: &Stack,
    pkgs: &PackageObjs,
) -> RuntimeResult<GosValue> {
    let typ = val.typ();
    let mut cur_val: GosValue = val;
    if typ == ValueType::Pointer {
        cur_val = cur_val.as_some_pointer()?.deref(stack, pkgs)?;
    }
    for &i in indices.iter() {
        let s = &cur_val.as_struct().0;
        let v = s.borrow_fields()[i as usize].clone();
        cur_val = v;
    }
    Ok(cur_val)
}

#[inline]
fn cast_receiver(
    receiver: GosValue,
    b1: bool,
    stack: &Stack,
    objs: &VMObjects,
) -> RuntimeResult<GosValue> {
    let b0 = receiver.typ() == ValueType::Pointer;
    if b0 == b1 {
        Ok(receiver)
    } else if b1 {
        Ok(GosValue::new_pointer(PointerObj::UpVal(
            UpValue::new_closed(receiver.clone()),
        )))
    } else {
        deref_value(&receiver, stack, objs)
    }
}

#[inline]
pub fn bind_iface_method(
    iface: &InterfaceObj,
    index: usize,
    stack: &Stack,
    objs: &VMObjects,
    gcv: &GcoVec,
) -> RuntimeResult<GosValue> {
    match iface {
        InterfaceObj::Gos(obj, b) => {
            let binding = &b.as_ref().unwrap().1[index];
            match binding {
                Binding4Runtime::Struct(func, ptr_recv, indices) => {
                    let obj = match indices {
                        None => obj.copy_semantic(gcv),
                        Some(inds) => get_embeded(obj.clone(), inds, stack, &objs.packages)?
                            .copy_semantic(gcv),
                    };
                    let obj = cast_receiver(obj, *ptr_recv, stack, objs)?;
                    let cls = ClosureObj::new_gos(*func, &objs.functions, Some(obj));
                    Ok(GosValue::new_closure(cls, gcv))
                }
                Binding4Runtime::Iface(i, indices) => {
                    let bind = |obj: &GosValue| {
                        bind_iface_method(&obj.as_interface().unwrap(), *i, stack, objs, gcv)
                    };
                    match indices {
                        None => bind(&obj),
                        Some(inds) => bind(&get_embeded(obj.clone(), inds, stack, &objs.packages)?),
                    }
                }
            }
        }
        InterfaceObj::Ffi(ffi) => {
            let methods = objs.metas[ffi.meta.key].as_interface().iface_methods_info();
            let (name, meta) = methods[index].clone();
            let cls = FfiClosureObj {
                ffi: ffi.ffi_obj.clone(),
                func_name: name,
                meta: meta,
            };
            Ok(GosValue::new_closure(ClosureObj::new_ffi(cls), gcv))
        }
    }
}
