// Copyright 2022 The Goscript Authors. All rights reserved.
// Use of this source code is governed by a BSD-style
// license that can be found in the LICENSE file.

#![allow(dead_code)]
use crate::channel;
use crate::ffi::{FfiCallCtx, FfiFactory};
use crate::gc::{gc, GcoVec};
use crate::metadata::*;
use crate::objects::{u64_to_key, ClosureObj};
use crate::stack2::{RangeStack, Stack};
use crate::value::*;
use async_executor::LocalExecutor;
use futures_lite::future;
use goscript_parser::{FilePos, FileSet};
use std::borrow::Cow;
use std::cell::{Cell, RefCell};
use std::cmp::Ordering;
use std::collections::HashMap;
use std::pin::Pin;
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
        $frame.pc = $code.len() - 1;
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
        let right = unsafe { $stack.read($inst.s1, $sb, $consts).data().copy_non_ptr() };
        let d = $stack.get_data_mut($inst.s0 + $sb);
        *d = d.$op(&right, $inst.t0);
    }};
}

macro_rules! shift_op {
    ($stack:expr, $op:tt, $inst:expr, $sb:expr, $consts:expr) => {{
        let mut right = unsafe { $stack.read($inst.s1, $sb, $consts).data().copy_non_ptr() };
        right.cast_copyable($inst.t1, ValueType::Uint32);
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
        let mut right = unsafe { $stack.read($inst.s1, $sb, $consts).data().copy_non_ptr() };
        right.cast_copyable($inst.t1, ValueType::Uint32);
        let d = $stack.get_data_mut($inst.s0 + $sb);
        *d = d.$op(right.as_uint32(), $inst.t0);
    }};
}

macro_rules! unary_op {
    ($stack:expr, $op:tt, $inst:expr, $sb:expr, $consts:expr) => {{
        let mut val = $stack.read($inst.s0, $sb, $consts).clone();
        unsafe { val.data_mut() }.unary_negate($inst.t0);
        $stack.set($inst.d + $sb, val);
    }};
}

#[derive(Debug)]
pub struct ByteCode {
    pub objects: Pin<Box<VMObjects>>,
    pub packages: Vec<PackageKey>,
    /// For calling method via interfaces
    pub ifaces: Vec<(Meta, Vec<Binding4Runtime>)>,
    /// For embedded fields of structs
    pub indices: Vec<Vec<usize>>,
    pub entry: FunctionKey,
}

impl ByteCode {
    pub fn new(
        objects: Pin<Box<VMObjects>>,
        packages: Vec<PackageKey>,
        ifaces: Vec<(Meta, Vec<IfaceBinding>)>,
        indices: Vec<Vec<usize>>,
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
            objects: objects,
            packages: packages,
            ifaces: ifaces,
            indices: indices,
            entry: entry,
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
            exec: exec,
            code: code,
            gcv: gcv,
            ffi_factory: ffi_factory,
            fs: fs,
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
        let s_meta: &StaticMeta = &objs.s_meta;
        let pkgs = &ctx.code.packages;
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

        let mut consts = &func.consts;
        let mut code = func.code();

        let mut total_inst = 0;
        //let mut stats: HashMap<Opcode, usize> = HashMap::new();
        loop {
            let mut frame = self.frames.last_mut().unwrap();
            let mut result: Result = Result::Continue;
            let mut panic: Option<PanicData> = None;
            let yield_unit = 1024;
            for _ in 0..yield_unit {
                let inst = &code[frame.pc as usize];
                let inst_op = inst.op;
                total_inst += 1;
                //stats.entry(*inst).and_modify(|e| *e += 1).or_insert(1);
                frame.pc += 1;
                //dbg!(inst_op);
                match inst_op {
                    // desc: local
                    // s0: slice
                    // s1: index
                    Opcode::LOAD_SLICE => {
                        let slice = stack.read(inst.s0, sb, consts);
                        let index = *stack.read(inst.s1, sb, consts).as_uint();
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
                        let index = *stack.read(inst.s0, sb, consts).as_uint();
                        match dest.slice_array_equivalent(index) {
                            Ok((array, i)) => match inst.extra_op {
                                None => {
                                    let val = stack.read(inst.s1, sb, consts).copy_semantic(gcv);
                                    let result = array.dispatcher_a_s().array_set(&array, &val, i);
                                    panic_if_err!(result, panic, frame, code);
                                }
                                Some(op) => match array.dispatcher_a_s().array_get(&array, i) {
                                    Ok(old) => {
                                        let val = stack.read_and_op(
                                            old.data(),
                                            inst.t0,
                                            op,
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
                        let index = *stack.read(inst.s1, sb, consts).as_uint();
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
                        let index = *stack.read(inst.s0, sb, consts).as_uint();
                        match inst.extra_op {
                            None => {
                                let val = stack.read(inst.s0, sb, consts).copy_semantic(gcv);
                                let result = array.dispatcher_a_s().array_set(&array, &val, index);
                                panic_if_err!(result, panic, frame, code);
                            }
                            Some(op) => match array.dispatcher_a_s().array_get(&array, index) {
                                Ok(old) => {
                                    let val = stack.read_and_op(
                                        old.data(),
                                        inst.t0,
                                        op,
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
                    // desc: local
                    // s0: map
                    // s1: key
                    // type0: FlagA indicating it's a comma-ok
                    Opcode::LOAD_MAP => {
                        let map = stack.read(inst.s0, sb, consts);
                        let key = stack.read(inst.s1, sb, consts);
                        let val = match map.as_map() {
                            Some(map) => map.0.get(&key),
                            None => None,
                        };
                        if inst.t1 != ValueType::FlagA {
                            match val {
                                Some(v) => stack.set(inst.d + sb, v),
                                None => go_panic_str!(panic, "read from nil map", frame, code),
                            }
                        } else {
                            let (v, ok) = match val {
                                Some(v) => (v, true),
                                None => (stack.read(inst.s2, sb, consts).clone(), false),
                            };
                            stack.set(inst.d + sb, v);
                            stack.set(inst.d + 1 + sb, GosValue::new_bool(ok));
                        }
                    }
                    // desc: map
                    // s0: index
                    // s1: value
                    Opcode::STORE_MAP => {
                        let dest = stack.read(inst.d, sb, consts);
                        match dest.as_some_map() {
                            Ok(map) => {
                                let key = stack.read(inst.s0, sb, consts);
                                match inst.extra_op {
                                    None => {
                                        let val =
                                            stack.read(inst.s1, sb, consts).copy_semantic(gcv);
                                        map.0.insert(key.clone(), val);
                                    }
                                    Some(op) => {
                                        let old = match map.0.get(&key) {
                                            Some(v) => v,
                                            None => stack.read(inst.s2, sb, consts).clone(),
                                        };
                                        let val = stack.read_and_op(
                                            old.data(),
                                            inst.t0,
                                            op,
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
                        match inst.extra_op {
                            None => {
                                let val = stack.read(inst.s1, sb, consts).copy_semantic(gcv);
                                dest.as_struct().0.borrow_fields_mut()[inst.s0 as usize] = val;
                            }
                            Some(op) => {
                                let old = &dest.as_struct().0.borrow_fields()[inst.s0 as usize];
                                let val = stack.read_and_op(
                                    old.data(),
                                    inst.t0,
                                    op,
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
                            Ok(s) => match inst.extra_op {
                                None => {
                                    let val = stack.read(inst.s1, sb, consts).copy_semantic(gcv);
                                    s.as_struct().0.borrow_fields_mut()[index] = val;
                                }
                                Some(op) => {
                                    let old = &s.as_struct().0.borrow_fields()[index as usize];
                                    let val = stack.read_and_op(
                                        old.data(),
                                        inst.t0,
                                        op,
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
                        let index = inst.s0;
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
                        match inst.extra_op {
                            None => {
                                let val = stack.read(inst.s1, sb, consts).copy_semantic(gcv);
                                *pkg.member_mut(index) = val;
                            }
                            Some(op) => {
                                let old = pkg.member(index);
                                let val = stack.read_and_op(
                                    old.data(),
                                    inst.t0,
                                    op,
                                    inst.s1,
                                    sb,
                                    &consts,
                                );
                                *pkg.member_mut(index) = val;
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
                            let val = match inst.extra_op {
                                None => stack.read(inst.s0, sb, consts).copy_semantic(gcv),
                                Some(op) => {
                                    let old = p.deref(stack, &objs.packages)?;
                                    stack.read_and_op(old.data(), inst.t0, op, inst.s1, sb, &consts)
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
                    Opcode::LOAD_UPVALUE => {
                        let uvs = frame.var_ptrs.as_ref().unwrap();
                        let val = uvs[inst.s0 as usize].value(stack).into_owned();
                        stack.set(inst.d + sb, val);
                    }
                    Opcode::STORE_UPVALUE => {
                        let uvs = frame.var_ptrs.as_ref().unwrap();
                        let uv = &uvs[inst.d as usize];
                        match inst.extra_op {
                            None => {
                                let val = stack.read(inst.s0, sb, consts).copy_semantic(gcv);
                                uv.set_value(val, stack);
                            }
                            Some(op) => {
                                let old = uv.value(stack);
                                let val = stack.read_and_op(
                                    old.data(),
                                    inst.t0,
                                    op,
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
                                if inst.t0 == ValueType::FlagA {
                                    stack.set(inst.d + sb + 1, GosValue::new_bool(ok));
                                }
                            }
                            None => loop {
                                future::yield_now().await;
                            },
                        };
                    }
                    Opcode::PRE_CALL => {
                        let cls = stack
                            .read(inst.s0, sb, consts)
                            .as_closure()
                            .unwrap()
                            .0
                            .clone();
                        let next_sb = inst.s0;
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
                                    ValueType::Void => {
                                        // default call
                                        self.frames.push(nframe);
                                        frame_height += 1;
                                        frame = self.frames.last_mut().unwrap();
                                        func = nfunc;
                                        sb = frame.stack_base;
                                        consts = &func.consts;
                                        code = func.code();
                                        //dbg!(&consts);
                                        //dbg!(&code);
                                        //dbg!(&stack);
                                        debug_assert!(func.local_count() == func.local_zeros.len());
                                    }
                                    ValueType::FlagA => {
                                        // goroutine
                                        nframe.stack_base = 0;
                                        let begin = inst.s0 + sb;
                                        let end = begin + nfunc.param_types().len() as OpIndex;
                                        let vec = stack.move_vec(begin, end);
                                        let nstack = Stack::with_vec(vec);
                                        self.context.spawn_fiber(nstack, nframe);
                                    }
                                    ValueType::FlagB => {
                                        let begin = inst.s0 + sb;
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
                                let begin = inst.s0 + sb;
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
                            ValueType::Void => true,
                            // init_package func
                            ValueType::FlagA => {
                                let pkey = stack.read(inst.d, sb, consts).as_package();
                                let pkg = &objs.packages[*pkey];
                                // the var values left on the stack are for pkg members
                                pkg.init_vars(stack.move_vec(inst.s0 + sb, inst.s1 + sb));
                                false
                            }
                            // func with deferred calls
                            ValueType::FlagB => {
                                if let Some(call) =
                                    frame.defer_stack.as_mut().map(|x| x.pop()).flatten()
                                {
                                    // run Opcode::RETURN to check if deferred_stack is empty
                                    frame.pc -= 1;

                                    let call_vec_len = call.vec.len() as OpIndex;
                                    stack.set_vec(inst.s0 + sb, call.vec);
                                    let nframe = call.frame;

                                    self.frames.push(nframe);
                                    frame_height += 1;
                                    frame = self.frames.last_mut().unwrap();
                                    let fkey = frame.func();
                                    func = &objs.functions[fkey];
                                    sb = frame.stack_base;
                                    consts = &func.consts;
                                    code = func.code();
                                    debug_assert!(func.local_count() == func.local_zeros.len());
                                    let index = inst.s0 + sb + call_vec_len;
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
                            let begin = inst.s0 + sb;
                            let end =
                                begin + frame.func_val(objs).stack_temp_types.len() as OpIndex;
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
                        consts = &func.consts;
                        code = func.code();

                        if let Some(p) = &mut panic {
                            p.call_stack.push((frame.func(), frame.pc - 1));
                            frame.pc = code.len() as OpIndex - 1;
                        }
                    }
                    Opcode::JUMP => frame.pc += inst.s0,
                    Opcode::JUMP_IF => {
                        if *stack.read(inst.s0, sb, consts).as_bool() {
                            frame.pc += inst.s1;
                        }
                    }
                    Opcode::JUMP_IF_NOT => {
                        if !*stack.read(inst.s0, sb, consts).as_bool() {
                            frame.pc += inst.s1;
                        }
                    }
                    Opcode::SWITCH => {
                        let t = inst.t0;
                        let a = stack.read(inst.s0, sb, consts);
                        let b = stack.read(inst.s0 + 1, sb, consts);
                        let ok = if t.copyable() {
                            a.data().compare_eql(b.data(), t)
                        } else if t != ValueType::Metadata {
                            a.eq(&b)
                        } else {
                            a.as_metadata().identical(b.as_metadata(), &objs.metas)
                        };
                        if ok {
                            frame.pc += inst.s1;
                        }
                    }
                    Opcode::SELECT => {
                        let blocks = inst.s1;
                        let begin = frame.pc as usize - 1;
                        let mut end = begin + blocks as usize;
                        let end_code = &code[end - 1];
                        let default_offset = match end_code.t0 {
                            ValueType::FlagE => {
                                end -= 1;
                                Some(end_code.s1)
                            }
                            _ => None,
                        };
                        let comms = code[begin..end]
                            .iter()
                            .enumerate()
                            .rev()
                            .map(|(i, sel_code)| {
                                let offset = if i == 0 { 0 } else { sel_code.s1 };
                                let flag = sel_code.t0;
                                match &flag {
                                    ValueType::FlagA => {
                                        let chan = stack.read(inst.s0, sb, consts).clone();
                                        let val =
                                            stack.read(inst.s0 + 1, sb, consts).copy_semantic(gcv);
                                        channel::SelectComm::Send(chan, val, offset)
                                    }
                                    ValueType::FlagB | ValueType::FlagC | ValueType::FlagD => {
                                        let chan = stack.read(inst.s0, sb, consts).clone();
                                        channel::SelectComm::Recv(chan, flag, offset)
                                    }
                                    _ => unreachable!(),
                                }
                            })
                            .collect();
                        let selector = channel::Selector::new(comms, default_offset);

                        drop(stack_mut_ref);
                        let re = selector.select().await;
                        restore_stack_ref!(self, stack, stack_mut_ref);

                        match re {
                            Ok((i, val)) => {
                                let block_offset = if i >= selector.comms.len() {
                                    selector.default_offset.unwrap()
                                } else {
                                    match &selector.comms[i] {
                                        channel::SelectComm::Send(_, _, offset) => *offset,
                                        channel::SelectComm::Recv(c, flag, offset) => {
                                            let (unwrapped, ok) = unwrap_recv_val!(
                                                c.as_channel().as_ref().unwrap(),
                                                val,
                                                gcv
                                            );
                                            match flag {
                                                ValueType::FlagC => {
                                                    stack.set(inst.d + sb, unwrapped);
                                                }
                                                ValueType::FlagD => {
                                                    stack.set(inst.d + sb, unwrapped);
                                                    stack.set(
                                                        inst.d + sb + 1,
                                                        GosValue::new_bool(ok),
                                                    );
                                                }
                                                _ => {}
                                            }
                                            *offset
                                        }
                                    }
                                };
                                // jump to the block
                                frame.pc += (blocks - 1) + block_offset;
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
                        if self.rstack.range_body(inst.t0, inst.t1, stack, inst.s0) {
                            frame.pc += inst.s1;
                        }
                    }
                    _ => unimplemented!(),
                }
            } //yield unit
            match result {
                Result::End => {
                    if let Some(p) = panic {
                        println!("panic: {}", p.msg);
                        if let Some(files) = self.context.fs {
                            for (fkey, pc) in p.call_stack.iter() {
                                let func = &objs.functions[*fkey];
                                if let Some(p) = func.pos()[*pc as usize] {
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
fn get_struct_and_index(
    val: GosValue,
    indices: &Vec<usize>,
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
        index,
    )
}

#[inline]
pub fn get_embeded(
    val: GosValue,
    indices: &[usize],
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
        let v = s.borrow_fields()[i].clone();
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

pub fn bind_method(
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
                        bind_method(&obj.as_interface().unwrap(), *i, stack, objs, gcv)
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
