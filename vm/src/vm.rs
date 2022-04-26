// Copyright 2022 The Goscript Authors. All rights reserved.
// Use of this source code is governed by a BSD-style
// license that can be found in the LICENSE file.

#![allow(dead_code)]
use super::channel;
use super::ffi::{FfiCallCtx, FfiFactory};
use super::gc::{gc, GcoVec};
use super::instruction::*;
use super::metadata::*;
use super::objects::{u64_to_key, ClosureObj};
use super::stack::{RangeStack, Stack};
use super::value::*;
use async_executor::LocalExecutor;
use futures_lite::future;
use goscript_parser::FileSet;
use std::cell::{Cell, RefCell};
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
        $frame.pc = $code.len() - 1;
    }};
}

macro_rules! panic_if_err {
    ($result:expr, $panic:ident, $frame:ident, $code:ident) => {{
        if let Err(e) = $result {
            go_panic_str!($panic, &e, $frame, $code);
        }
    }};
}

macro_rules! read_imm_key {
    ($code:ident, $frame:ident, $objs:ident) => {{
        let inst = $code[$frame.pc];
        $frame.pc += 1;
        u64_to_key(inst.get_u64())
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

#[derive(Debug)]
pub struct ByteCode {
    pub objects: Pin<Box<VMObjects>>,
    pub packages: Vec<PackageKey>,
    pub ifaces: Vec<(Meta, Vec<Binding4Runtime>)>,
    pub entry: FunctionKey,
}

impl ByteCode {
    pub fn new(
        objects: Pin<Box<VMObjects>>,
        packages: Vec<PackageKey>,
        ifaces: Vec<(Meta, Vec<IfaceBinding>)>,
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
    pc: usize,
    stack_base: usize,
    var_ptrs: Option<Vec<UpValue>>,
    // closures that have upvalues pointing to this frame
    referred_by: Option<HashMap<OpIndex, Referers>>,

    defer_stack: Option<Vec<DeferredCall>>,
}

impl CallFrame {
    fn with_closure(c: ClosureObj, sbase: usize) -> CallFrame {
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
                let val = stack.get(Stack::offset(self.stack_base, *ind));
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
    call_stack: Vec<(FunctionKey, usize)>,
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
        let frame = self.frames.last_mut().unwrap();
        let mut func = &objs.functions[frame.func()];

        let mut stack_mut_ref = self.stack.borrow_mut();
        let mut stack: &mut Stack = &mut stack_mut_ref;
        // allocate local variables
        stack.append_vec(func.local_zeros.clone());

        let mut consts = &func.consts;
        let mut code = func.code();
        let mut stack_base = frame.stack_base;
        let mut frame_height = self.frames.len();

        let mut total_inst = 0;
        //let mut stats: HashMap<Opcode, usize> = HashMap::new();
        loop {
            let mut frame = self.frames.last_mut().unwrap();
            let mut result: Result = Result::Continue;
            let mut panic: Option<PanicData> = None;
            let yield_unit = 1024;
            for _ in 0..yield_unit {
                let inst = code[frame.pc];
                let inst_op = inst.op();
                total_inst += 1;
                //stats.entry(*inst).and_modify(|e| *e += 1).or_insert(1);
                frame.pc += 1;
                //dbg!(inst_op);
                match inst_op {
                    Opcode::PUSH_CONST => {
                        let index = inst.imm();
                        stack.push(consts[index as usize].clone());
                    }
                    Opcode::PUSH_NIL => stack.push_nil(inst.t0()),
                    Opcode::PUSH_FALSE => stack.push_bool(false),
                    Opcode::PUSH_TRUE => stack.push_bool(true),
                    Opcode::PUSH_IMM => stack.push_int32_as(inst.imm(), inst.t0()),
                    Opcode::PUSH_ZERO_VALUE => {
                        let meta = consts[inst.imm() as usize].as_metadata();
                        stack.push(meta.zero(&objs.metas, gcv));
                    }
                    Opcode::POP => match inst.imm() {
                        // this looks weired because it used to require ValueType for every pop
                        // and we may change it back in the future.
                        1 => {
                            stack.pop_value();
                        }
                        2 => {
                            stack.pop_value();
                            stack.pop_value();
                        }
                        3 => {
                            stack.pop_value();
                            stack.pop_value();
                            stack.pop_value();
                        }
                        _ => unreachable!(),
                    },
                    Opcode::LOAD_LOCAL => {
                        let index = Stack::offset(stack_base, inst.imm());
                        stack.push_from_index(index); // (index![stack, index]);
                    }
                    Opcode::STORE_LOCAL => {
                        let (rhs_index, index) = inst.imm824();
                        let s_index = Stack::offset(stack_base, index);
                        stack.store_local(s_index, rhs_index, inst.t0(), gcv);
                    }
                    Opcode::LOAD_UPVALUE => {
                        let index = inst.imm();
                        let upvalue = frame.var_ptrs.as_ref().unwrap()[index as usize].clone();
                        stack.push(upvalue.value(stack));
                        frame = self.frames.last_mut().unwrap();
                    }
                    Opcode::STORE_UPVALUE => {
                        let (rhs_index, index) = inst.imm824();
                        let upvalue = frame.var_ptrs.as_ref().unwrap()[index as usize].clone();
                        stack.store_up_value(&upvalue, rhs_index, inst.t0(), gcv);
                        frame = self.frames.last_mut().unwrap();
                    }
                    Opcode::LOAD_INDEX => {
                        let ind = stack.pop_value();
                        let val = &stack.pop_value();
                        let result = if inst.t2_as_index() == 0 {
                            val.load_index(&ind, gcv).and_then(|v| Ok(stack.push(v)))
                        } else {
                            stack.push_index_comma_ok(val, &ind, gcv)
                        };
                        panic_if_err!(result, panic, frame, code);
                    }
                    Opcode::LOAD_INDEX_IMM => {
                        let val = &stack.pop_value();
                        let index = inst.imm() as usize;
                        let result = if inst.t2_as_index() == 0 {
                            val.load_index_int(index, gcv)
                                .and_then(|v| Ok(stack.push(v)))
                        } else {
                            stack.push_index_comma_ok(val, &GosValue::new_int(index as isize), gcv)
                        };
                        panic_if_err!(result, panic, frame, code);
                    }
                    Opcode::STORE_INDEX => {
                        let (rhs_index, index) = inst.imm824();
                        let s_index = Stack::offset(stack.len(), index);
                        let key = stack.get(s_index + 1);
                        let target = &stack.get(s_index);
                        let result = stack.store_index(target, &key, rhs_index, inst.t0(), gcv);
                        panic_if_err!(result, panic, frame, code);
                    }
                    Opcode::STORE_INDEX_IMM => {
                        // the only place we can store the immediate index is t2
                        let (rhs_index, imm) = inst.imm824();
                        let index = inst.t2_as_index();
                        let s_index = Stack::offset(stack.len(), index);
                        let target = &stack.get(s_index);
                        let result = stack.store_index_int(target, imm, rhs_index, inst.t0(), gcv);
                        panic_if_err!(result, panic, frame, code);
                    }
                    Opcode::LOAD_STRUCT_FIELD => {
                        let (struct_, index) = get_struct_and_index(
                            inst.imm(),
                            stack.pop_value(),
                            stack,
                            code,
                            frame,
                            objs,
                        );
                        match struct_ {
                            Ok(t) => {
                                stack.push(t.as_struct().0.borrow_fields()[index].clone());
                            }
                            Err(e) => go_panic_str!(panic, &e, frame, code),
                        }
                    }
                    Opcode::BIND_METHOD => {
                        let val = stack.pop_value();
                        let func = read_imm_key!(code, frame, objs);
                        stack.push(GosValue::new_closure(
                            ClosureObj::new_gos(
                                func,
                                &objs.functions,
                                Some(val.copy_semantic(gcv)),
                            ),
                            gcv,
                        ));
                    }
                    Opcode::BIND_INTERFACE_METHOD => {
                        let val = stack.pop_interface().unwrap();
                        let index = inst.imm() as usize;
                        match bind_method(&val, index, stack, objs, gcv) {
                            Ok(cls) => stack.push(cls),
                            Err(e) => {
                                go_panic_str!(panic, &e, frame, code);
                            }
                        }
                    }
                    Opcode::STORE_STRUCT_FIELD => {
                        let (rhs_index, imm) = inst.imm824();
                        let index = inst.t2_as_index();
                        let s_index = Stack::offset(stack.len(), index);
                        let (struct_, index) = get_struct_and_index(
                            imm,
                            stack.get(s_index).clone(),
                            stack,
                            code,
                            frame,
                            objs,
                        );
                        match struct_ {
                            Ok(target) => {
                                let field = &mut target.as_struct().0.borrow_fields_mut()[index];
                                stack.store_val(field, rhs_index, inst.t0(), gcv);
                            }
                            Err(e) => go_panic_str!(panic, &e, frame, code),
                        }
                    }
                    Opcode::LOAD_PKG_FIELD => {
                        let index = inst.imm();
                        let pkg_key = read_imm_key!(code, frame, objs);
                        let pkg = &objs.packages[pkg_key];
                        stack.push(pkg.member(index).clone());
                    }
                    Opcode::LOAD_PKG_INIT => {
                        let index = stack.pop_int32();
                        let pkg_key = read_imm_key!(code, frame, objs);
                        let pkg = &objs.packages[pkg_key];
                        match pkg.init_func(index) {
                            Some(f) => {
                                stack.push(GosValue::new_int32(index + 1));
                                stack.push(f.clone());
                                stack.push(GosValue::new_bool(true));
                            }
                            None => stack.push(GosValue::new_bool(false)),
                        }
                    }
                    Opcode::STORE_PKG_FIELD => {
                        let (rhs_index, imm) = inst.imm824();
                        let pkg = &objs.packages[read_imm_key!(code, frame, objs)];
                        stack.store_val(&mut pkg.member_mut(imm), rhs_index, inst.t0(), gcv);
                    }
                    Opcode::STORE_DEREF => {
                        let (rhs_index, index) = inst.imm824();
                        let s_index = Stack::offset(stack.len(), index);
                        let p = stack.get(s_index).clone();
                        let result = p.as_some_pointer().and_then(|p| {
                            stack.store_to_pointer(p, rhs_index, inst.t0(), &objs.packages, gcv)
                        });
                        panic_if_err!(result, panic, frame, code);
                    }
                    Opcode::CAST => {
                        let (target, mapping) = inst.imm824();
                        let index = Stack::offset(stack.len(), target);
                        let from_type = inst.t1();
                        let to_type = inst.t0();
                        match to_type {
                            ValueType::UintPtr => match from_type {
                                ValueType::UnsafePtr => {
                                    let up = stack.pop_unsafe_ptr();
                                    stack.push(GosValue::new_uint_ptr(
                                        up.map_or(0, |x| x.as_rust_ptr() as *const () as usize),
                                    ));
                                }
                                _ => stack.get_mut(index).cast_copyable(from_type, to_type),
                            },
                            _ if to_type.copyable() => {
                                stack.get_mut(index).cast_copyable(from_type, to_type);
                            }
                            ValueType::Interface => {
                                let binding = ifaces[mapping as usize].clone();
                                let under = stack.copy_semantic(index, gcv);
                                let val = GosValue::new_interface(InterfaceObj::with_value(
                                    under,
                                    Some(binding),
                                ));
                                stack.set(index, val);
                            }
                            ValueType::String => {
                                let result = match from_type {
                                    ValueType::Slice => match inst.t2() {
                                        ValueType::Int32 => {
                                            let s = match stack.get_slice::<Elem32>(index) {
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
                                        ValueType::Uint8 => match stack.get_slice::<Elem8>(index) {
                                            Some(slice) => GosValue::new_string(slice.0.clone()),
                                            None => GosValue::with_str(""),
                                        },
                                        _ => unreachable!(),
                                    },
                                    _ => {
                                        let val = stack.get_mut(index);
                                        val.cast_copyable(from_type, ValueType::Uint32);
                                        GosValue::with_str(
                                            &char_from_u32(*val.as_uint32()).to_string(),
                                        )
                                    }
                                };
                                stack.set(index, result);
                            }
                            ValueType::Slice => {
                                let from = stack.get_string(index);
                                let result = match inst.t2() {
                                    ValueType::Int32 => {
                                        let data = StrUtil::as_str(from)
                                            .chars()
                                            .map(|x| GosValue::new_int32(x as i32))
                                            .collect();
                                        GosValue::slice_with_data(data, inst.t2(), gcv)
                                    }
                                    ValueType::Uint8 => {
                                        GosValue::new_slice(from.clone(), ValueType::Uint8)
                                    }
                                    _ => unreachable!(),
                                };
                                stack.set(index, result);
                            }
                            ValueType::Pointer => match from_type {
                                ValueType::Pointer => {}
                                ValueType::UnsafePtr => {
                                    match stack.get(index).as_unsafe_ptr() {
                                        Some(p) => {
                                            match p.ptr().as_any().downcast_ref::<PointerHandle>() {
                                                Some(h) => {
                                                    match h.ptr().cast(
                                                        inst.t2(),
                                                        &stack,
                                                        &objs.packages,
                                                    ) {
                                                        Ok(p) => stack
                                                            .set(index, GosValue::new_pointer(p)),
                                                        Err(e) => {
                                                            go_panic_str!(panic, &e, frame, code)
                                                        }
                                                    };
                                                }
                                                None => {
                                                    go_panic_str!(panic, "only a unsafe-pointer cast from a pointer can be cast back to a pointer", frame, code);
                                                }
                                            }
                                        }
                                        None => {
                                            stack.set(index, GosValue::new_nil(ValueType::Pointer));
                                        }
                                    };
                                }
                                _ => unimplemented!(),
                            },
                            ValueType::UnsafePtr => match from_type {
                                ValueType::Pointer => {
                                    let h = PointerHandle::new(stack.get(index));
                                    stack.set(index, h);
                                }
                                _ => unimplemented!(),
                            },
                            _ => {
                                dbg!(to_type);
                                unimplemented!()
                            }
                        }
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
                    Opcode::SHL => stack.shl(inst.t0(), inst.t1()),
                    Opcode::SHR => stack.shr(inst.t0(), inst.t1()),
                    Opcode::UNARY_ADD => {}
                    Opcode::UNARY_SUB => stack.unary_negate(inst.t0()),
                    Opcode::UNARY_XOR => stack.unary_xor(inst.t0()),
                    Opcode::NOT => stack.logical_not(inst.t0()),
                    Opcode::EQL => stack.compare_eql(inst.t0(), inst.t1()),
                    Opcode::LSS => stack.compare_lss(inst.t0()),
                    Opcode::GTR => stack.compare_gtr(inst.t0()),
                    Opcode::NEQ => stack.compare_neq(inst.t0()),
                    Opcode::LEQ => stack.compare_leq(inst.t0()),
                    Opcode::GEQ => stack.compare_geq(inst.t0()),
                    Opcode::SEND => {
                        let val = stack.pop_value();
                        let chan = stack.pop_channel();
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
                        match stack.pop_channel() {
                            Some(chan) => {
                                drop(stack_mut_ref);
                                let val = chan.recv().await;
                                restore_stack_ref!(self, stack, stack_mut_ref);
                                let (unwrapped, ok) = unwrap_recv_val!(chan, val, gcv);
                                stack.push(unwrapped);
                                if inst.t1() == ValueType::FlagA {
                                    stack.push(GosValue::new_bool(ok));
                                }
                            }
                            None => loop {
                                future::yield_now().await;
                            },
                        };
                    }
                    Opcode::REF => {
                        let val = stack.pop_value();
                        let boxed = PointerObj::new_closed_up_value(&val);
                        stack.push(GosValue::new_pointer(boxed));
                    }
                    Opcode::REF_UPVALUE => {
                        let index = inst.imm();
                        let upvalue = frame.var_ptrs.as_ref().unwrap()[index as usize].clone();
                        stack.push(GosValue::new_pointer(PointerObj::UpVal(upvalue.clone())));
                    }
                    Opcode::REF_SLICE_MEMBER => {
                        let i = stack.pop_int() as OpIndex;
                        let typ = inst.t0();
                        let arr_or_slice = stack.pop_value();
                        match PointerObj::new_slice_member(arr_or_slice, i, typ, inst.t2()) {
                            Ok(p) => stack.push(GosValue::new_pointer(p)),
                            Err(e) => {
                                go_panic_str!(panic, &e, frame, code)
                            }
                        }
                    }
                    Opcode::REF_STRUCT_FIELD => {
                        let (struct_, index) = get_struct_and_index(
                            inst.imm(),
                            stack.pop_value(),
                            stack,
                            code,
                            frame,
                            objs,
                        );
                        match struct_ {
                            Ok(target) => {
                                stack.push(GosValue::new_pointer(PointerObj::StructField(
                                    target,
                                    index as OpIndex,
                                )));
                            }
                            Err(e) => go_panic_str!(panic, &e, frame, code),
                        }
                    }
                    Opcode::REF_PKG_MEMBER => {
                        let pkg = read_imm_key!(code, frame, objs);
                        stack.push(GosValue::new_pointer(PointerObj::PkgMember(
                            pkg,
                            inst.imm(),
                        )));
                    }
                    Opcode::DEREF => {
                        let boxed = stack.pop_value();
                        let re = deref_value(&boxed, stack, objs).and_then(|v| Ok(stack.push(v)));
                        panic_if_err!(re, panic, frame, code);
                    }
                    Opcode::PRE_CALL => {
                        let cls = &stack.pop_closure().unwrap().0;
                        let next_stack_base = stack.len();
                        match cls {
                            ClosureObj::Gos(gosc) => {
                                let next_func = &objs.functions[gosc.func];
                                stack.append_vec(next_func.ret_zeros.clone());
                                if let Some(r) = &gosc.recv {
                                    // push receiver on stack as the first parameter
                                    // don't call copy_semantic because BIND_METHOD did it already
                                    stack.push(r.clone());
                                }
                            }
                            _ => {}
                        }
                        let next_frame = CallFrame::with_closure(cls.clone(), next_stack_base);
                        self.next_frames.push(next_frame);
                    }
                    Opcode::CALL => {
                        let mut nframe = self.next_frames.pop().unwrap();
                        let cls = nframe.closure().clone();
                        let call_style = inst.t0();
                        let variadic_typ = inst.t1();
                        if variadic_typ != ValueType::Void {
                            let index = nframe.stack_base + cls.total_param_count(&objs.metas) - 1;
                            stack.pack_variadic(index, variadic_typ, gcv);
                        }
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
                                        stack_base = frame.stack_base;
                                        consts = &func.consts;
                                        code = func.code();
                                        //dbg!(&consts);
                                        //dbg!(&code);
                                        //dbg!(&stack);
                                        debug_assert!(func.local_count() == func.local_zeros.len());
                                        // allocate local variables
                                        stack.append_vec(func.local_zeros.clone());
                                    }
                                    ValueType::FlagA => {
                                        // goroutine
                                        nframe.stack_base = 0;
                                        let nstack =
                                            Stack::move_from(stack, nfunc.param_types().len());
                                        self.context.spawn_fiber(nstack, nframe);
                                    }
                                    ValueType::FlagB => {
                                        let v = stack.pop_value_n(nfunc.param_types().len());
                                        let deferred = DeferredCall {
                                            frame: nframe,
                                            vec: v,
                                        };
                                        frame.defer_stack.get_or_insert(vec![]).push(deferred);
                                    }
                                    _ => unreachable!(),
                                }
                            }
                            ClosureObj::Ffi(ffic) => {
                                let ptypes = &objs.metas[ffic.meta.key].as_signature().params_type;
                                let params = stack.pop_value_n(ptypes.len());
                                // release stack so that code in ffi can yield
                                drop(stack_mut_ref);
                                let returns = {
                                    let mut ctx = FfiCallCtx {
                                        func_name: &ffic.func_name,
                                        vm_objs: objs,
                                        stack: &mut self.stack.borrow_mut(),
                                        gcv: gcv,
                                    };
                                    let ffi = ffic.ffi.borrow();
                                    let fut = ffi.call(&mut ctx, params);
                                    fut.await
                                };
                                restore_stack_ref!(self, stack, stack_mut_ref);
                                match returns {
                                    Ok(result) => stack.append_vec(result),
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

                        let clear_stack = match inst.t0() {
                            // default case
                            ValueType::Void => true,
                            // init_package func
                            ValueType::FlagA => {
                                let index = inst.imm() as usize;
                                let pkey = pkgs[index];
                                let pkg = &objs.packages[pkey];
                                // the var values left on the stack are for pkg members
                                pkg.init_vars(stack);
                                false
                            }
                            // func with deferred calls
                            ValueType::FlagB => {
                                if let Some(call) =
                                    frame.defer_stack.as_mut().map(|x| x.pop()).flatten()
                                {
                                    // run Opcode::RETURN to check if deferred_stack is empty
                                    frame.pc -= 1;

                                    stack.append_vec(call.vec);
                                    let nframe = call.frame;

                                    self.frames.push(nframe);
                                    frame_height += 1;
                                    frame = self.frames.last_mut().unwrap();
                                    let fkey = frame.func();
                                    func = &objs.functions[fkey];
                                    stack_base = frame.stack_base;
                                    consts = &func.consts;
                                    code = func.code();
                                    debug_assert!(func.local_count() == func.local_zeros.len());
                                    stack.append_vec(func.local_zeros.clone());
                                    continue;
                                }
                                true
                            }
                            _ => unreachable!(),
                        };

                        let panicking = panic.is_some();
                        if clear_stack {
                            // println!(
                            //     "current line: {}",
                            //     self.context.fs.unwrap().position(
                            //         objs.functions[frame.func()].pos()[frame.pc - 1].unwrap()
                            //     )
                            // );

                            frame.on_drop(&stack);
                            if !panicking {
                                stack.pop_value_n(frame.func_val(objs).stack_temp_types.len());
                            } else {
                                stack.discard_n(frame.func_val(objs).stack_temp_types.len());
                            }
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
                        stack_base = frame.stack_base;
                        // restore func, consts, code
                        func = &objs.functions[frame.func()];
                        consts = &func.consts;
                        code = func.code();

                        if let Some(p) = &mut panic {
                            p.call_stack.push((frame.func(), frame.pc - 1));
                            frame.pc = code.len() - 1;
                        }
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
                    Opcode::SHORT_CIRCUIT_OR => {
                        if *stack.get_data(stack.len() - 1).as_bool() {
                            frame.pc = Stack::offset(frame.pc, inst.imm());
                        } else {
                            stack.pop_discard_copyable();
                        }
                    }
                    Opcode::SHORT_CIRCUIT_AND => {
                        if !*stack.get_data(stack.len() - 1).as_bool() {
                            frame.pc = Stack::offset(frame.pc, inst.imm());
                        } else {
                            stack.pop_discard_copyable();
                        }
                    }
                    Opcode::SWITCH => {
                        if stack.switch_cmp(inst.t0(), objs) {
                            stack.pop_value();
                            frame.pc = Stack::offset(frame.pc, inst.imm());
                        }
                    }
                    Opcode::SELECT => {
                        let blocks = inst.imm();
                        let begin = frame.pc - 1;
                        let mut end = begin + blocks as usize;
                        let end_code = &code[end - 1];
                        let default_offset = match end_code.t0() {
                            ValueType::FlagE => {
                                end -= 1;
                                Some(end_code.imm())
                            }
                            _ => None,
                        };
                        let comms = code[begin..end]
                            .iter()
                            .enumerate()
                            .rev()
                            .map(|(i, sel_code)| {
                                let offset = if i == 0 { 0 } else { sel_code.imm() };
                                let flag = sel_code.t0();
                                match &flag {
                                    ValueType::FlagA => {
                                        let val = stack.pop_value();
                                        let chan = stack.pop_value();
                                        channel::SelectComm::Send(chan, val, offset)
                                    }
                                    ValueType::FlagB | ValueType::FlagC | ValueType::FlagD => {
                                        let chan = stack.pop_value();
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
                                                    stack.push(unwrapped);
                                                }
                                                ValueType::FlagD => {
                                                    stack.push(unwrapped);
                                                    stack.push_bool(ok);
                                                }
                                                _ => {}
                                            }
                                            *offset
                                        }
                                    }
                                };
                                // jump to the block
                                frame.pc = Stack::offset(frame.pc, (blocks - 1) + block_offset);
                            }
                            Err(e) => {
                                go_panic_str!(panic, &e, frame, code);
                            }
                        }
                    }
                    Opcode::RANGE_INIT => {
                        let len = stack.len();
                        let target = stack.get(len - 1);
                        let re = self
                            .rstack
                            .range_init(target, inst.t0(), inst.t2())
                            .and_then(|_| Ok(stack.pop_value()));
                        panic_if_err!(re, panic, frame, code);
                    }
                    Opcode::RANGE => {
                        let offset = inst.imm();
                        if self.rstack.range_body(inst.t0(), inst.t2(), stack) {
                            frame.pc = Stack::offset(frame.pc, offset);
                        }
                    }

                    Opcode::TYPE_ASSERT => {
                        let val = stack.pop_value();
                        let do_try = inst.t2_as_index() > 0;
                        let result = match val.as_some_interface() {
                            Ok(iface) => match &iface as &InterfaceObj {
                                InterfaceObj::Gos(v, b) => {
                                    let meta = b.as_ref().unwrap().0;
                                    let want_meta = consts[inst.imm() as usize].as_metadata();
                                    if *want_meta == meta {
                                        Ok((v.copy_semantic(gcv), true))
                                    } else {
                                        if do_try {
                                            Ok((want_meta.zero(&objs.metas, gcv), false))
                                        } else {
                                            Err("interface conversion: wrong type".to_owned())
                                        }
                                    }
                                }
                                InterfaceObj::Ffi(_) => {
                                    Err("FFI interface do not support type assertion".to_owned())
                                }
                            },
                            Err(e) => Err(e),
                        };
                        match result {
                            Ok((val, ok)) => {
                                stack.push(val);
                                if do_try {
                                    stack.push_bool(ok);
                                }
                            }
                            Err(e) => go_panic_str!(panic, &e, frame, code),
                        }
                    }
                    Opcode::TYPE => {
                        let iface_value = stack.pop_value();
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
                        stack.push(GosValue::new_metadata(meta));
                        let option_count = inst.imm() as usize;
                        if option_count > 0 {
                            let mut index = None;
                            for i in 0..option_count {
                                let inst_data = code[frame.pc + i];
                                if inst_data.t0() == typ {
                                    index = Some(inst_data.imm());
                                }
                            }
                            let s_index = Stack::offset(stack_base, index.unwrap());
                            stack.set(s_index, val);

                            frame.pc += option_count;
                        }
                    }
                    Opcode::IMPORT => {
                        let pkey = pkgs[inst.imm() as usize];
                        stack.push(GosValue::new_bool(!objs.packages[pkey].inited()));
                    }
                    Opcode::SLICE | Opcode::SLICE_FULL => {
                        let max = if inst_op == Opcode::SLICE_FULL {
                            stack.pop_int()
                        } else {
                            -1
                        };
                        let end = stack.pop_int();
                        let begin = stack.pop_int();
                        let result = match inst.t0() {
                            ValueType::Slice => {
                                let s = stack.pop_value();
                                s.dispatcher_a_s().slice_slice(&s, begin, end, max)
                            }
                            ValueType::String => {
                                GosValue::slice_string(&stack.pop_value(), begin, end, max)
                            }
                            ValueType::Array => {
                                GosValue::slice_array(stack.pop_value(), begin, end, inst.t1())
                            }
                            _ => unreachable!(),
                        };

                        match result {
                            Ok(v) => stack.push(v),
                            Err(e) => go_panic_str!(panic, &e, frame, code),
                        }
                    }
                    Opcode::LITERAL => {
                        let index = inst.imm();
                        let arg = &consts[index as usize];
                        let new_val = match arg.typ() {
                            ValueType::Function => {
                                // NEW a closure
                                let mut val =
                                    ClosureObj::new_gos(*arg.as_function(), &objs.functions, None);
                                match &mut val {
                                    ClosureObj::Gos(gos) => {
                                        if let Some(uvs) = &mut gos.uvs {
                                            drop(frame);
                                            for (_, uv) in uvs.iter_mut() {
                                                let r: &mut UpValueState =
                                                    &mut uv.inner.borrow_mut();
                                                if let UpValueState::Open(d) = r {
                                                    // get frame index, and add_referred_by
                                                    for i in 1..frame_height {
                                                        let index = frame_height - i;
                                                        if self.frames[index].func() == d.func {
                                                            let upframe = &mut self.frames[index];
                                                            d.stack = Rc::downgrade(&self.stack);
                                                            d.stack_base =
                                                                upframe.stack_base as OpIndex;
                                                            upframe.add_referred_by(
                                                                d.index, d.typ, uv,
                                                            );
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
                                GosValue::new_closure(val, gcv)
                            }
                            ValueType::Metadata => {
                                let count = stack.pop_int32();
                                let mut build_val = |m: &Meta| {
                                    let zero_val = m.zero(&objs.metas, gcv);
                                    let mut val = vec![];
                                    let mut cur_index = -1;
                                    for _ in 0..count {
                                        let i = stack.pop_int();
                                        let elem = stack.pop_value();
                                        if i < 0 {
                                            cur_index += 1;
                                        } else {
                                            cur_index = i;
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
                                let md = arg.as_metadata();
                                match &objs.metas[md.key] {
                                    MetadataType::Slice(m) => {
                                        let (val, typ) = build_val(m);
                                        GosValue::slice_with_data(val, typ, gcv)
                                    }
                                    MetadataType::Array(m, _) => {
                                        let (val, typ) = build_val(m);
                                        GosValue::array_with_data(val, typ, gcv)
                                    }
                                    MetadataType::Map(_, vm) => {
                                        let map_val = GosValue::map_with_default_val(
                                            vm.zero(&objs.metas, gcv),
                                            gcv,
                                        );
                                        let map = map_val.as_map().unwrap();
                                        for _ in 0..count {
                                            let k = stack.pop_value();
                                            let v = stack.pop_value();
                                            map.0.insert(k, v);
                                        }
                                        map_val
                                    }
                                    MetadataType::Struct(_, _) => {
                                        let struct_val = md.zero(&objs.metas, gcv);
                                        {
                                            let fields =
                                                &mut struct_val.as_struct().0.borrow_fields_mut();
                                            for _ in 0..count {
                                                let index = stack.pop_uint();
                                                fields[index] = stack.pop_value();
                                            }
                                        }
                                        struct_val
                                    }
                                    _ => unreachable!(),
                                }
                            }
                            _ => unimplemented!(),
                        };
                        stack.push(new_val);
                    }

                    Opcode::NEW => {
                        let md = stack.pop_metadata();
                        let v = md.into_value_category().zero(&objs.metas, gcv);
                        let p = GosValue::new_pointer(PointerObj::UpVal(UpValue::new_closed(v)));
                        stack.push(p);
                    }
                    Opcode::MAKE => {
                        let index = inst.imm() - 1;
                        let i = Stack::offset(stack.len(), index - 1);
                        let meta_val = stack.get(i);
                        let md = meta_val.as_metadata();
                        let val = match md.mtype_unwraped(&objs.metas) {
                            MetadataType::Slice(vmeta) => {
                                let (cap, len) = match index {
                                    -2 => (stack.pop_int() as usize, stack.pop_int() as usize),
                                    -1 => {
                                        let len = stack.pop_int() as usize;
                                        (len, len)
                                    }
                                    _ => unreachable!(),
                                };
                                let zero = vmeta.zero(&objs.metas, gcv);
                                GosValue::slice_with_size(len, cap, &zero, zero.typ(), gcv)
                            }
                            MetadataType::Map(_, v) => {
                                let default = v.zero(&objs.metas, gcv);
                                GosValue::map_with_default_val(default, gcv)
                            }
                            MetadataType::Channel(_, val_meta) => {
                                let cap = match index {
                                    -1 => stack.pop_int() as usize,
                                    0 => 0,
                                    _ => unreachable!(),
                                };
                                let zero = val_meta.zero(&objs.metas, gcv);
                                GosValue::new_channel(ChannelObj::new(cap, zero))
                            }
                            _ => unreachable!(),
                        };
                        stack.pop_value();
                        stack.push(val);
                    }
                    Opcode::COMPLEX => {
                        // for the specs: For complex, the two arguments must be of the same
                        // floating-point type and the return type is the complex type with
                        // the corresponding floating-point constituents
                        let t = inst.t0();
                        let val = match t {
                            ValueType::Float32 => {
                                let i = stack.pop_float32();
                                let r = stack.pop_float32();
                                GosValue::new_complex64(r, i)
                            }
                            ValueType::Float64 => {
                                let i = stack.pop_float64();
                                let r = stack.pop_float64();
                                GosValue::new_complex128(r, i)
                            }
                            _ => unreachable!(),
                        };
                        stack.push(val);
                    }
                    Opcode::REAL => {
                        let val = match inst.t0() {
                            ValueType::Complex64 => GosValue::new_float32(stack.pop_complex64().r),
                            ValueType::Complex128 => {
                                GosValue::new_float64(stack.pop_value().as_complex128().r)
                            }
                            _ => unreachable!(),
                        };
                        stack.push(val);
                    }
                    Opcode::IMAG => {
                        let val = match inst.t0() {
                            ValueType::Complex64 => GosValue::new_float32(stack.pop_complex64().i),
                            ValueType::Complex128 => {
                                GosValue::new_float64(stack.pop_value().as_complex128().i)
                            }
                            _ => unreachable!(),
                        };
                        stack.push(val);
                    }
                    Opcode::LEN => {
                        let l = stack.pop_value().len();
                        stack.push(GosValue::new_int(l as isize));
                    }
                    Opcode::CAP => {
                        let l = stack.pop_value().cap();
                        stack.push(GosValue::new_int(l as isize));
                    }
                    Opcode::APPEND => {
                        let index = Stack::offset(stack.len(), inst.imm() - 2);
                        match inst.t2() {
                            ValueType::FlagA => unreachable!(),
                            ValueType::FlagB => {} // default case, nothing to do
                            ValueType::FlagC => {
                                // special case, appending string as bytes
                                let s = stack.pop_string();
                                let arr = GosValue::new_non_gc_array(
                                    ArrayObj::with_raw_data(s.as_rust_slice().to_vec()),
                                    ValueType::Uint8,
                                );
                                let b_slice =
                                    GosValue::slice_array(arr, 0, -1, ValueType::Uint8).unwrap();
                                stack.push(b_slice);
                            }
                            _ => {
                                // pack args into a slice
                                stack.pack_variadic(index + 1, inst.t2(), gcv);
                            }
                        };
                        let b = stack.pop_value();
                        let a = stack.pop_value();
                        match dispatcher_a_s_for(inst.t0()).slice_append(a, b, gcv) {
                            Ok(slice) => stack.push(slice),
                            Err(e) => go_panic_str!(panic, &e, frame, code),
                        };
                    }
                    Opcode::COPY => {
                        let t2 = match inst.t2() {
                            ValueType::FlagC => ValueType::String,
                            _ => ValueType::Slice,
                        };
                        let b = stack.pop_value();
                        let a = stack.pop_value();
                        let count = match t2 {
                            ValueType::String => {
                                let string = b.as_string();
                                match a.as_slice::<Elem8>() {
                                    Some(s) => s.0.copy_from(&string),
                                    None => 0,
                                }
                            }
                            ValueType::Slice => dispatcher_a_s_for(inst.t0()).slice_copy_from(a, b),
                            _ => unreachable!(),
                        };
                        stack.push_int(count as isize);
                    }
                    Opcode::DELETE => {
                        let key = &stack.pop_value();
                        match stack.pop_map() {
                            Some(m) => m.0.delete(key),
                            None => {}
                        }
                    }
                    Opcode::CLOSE => match stack.pop_channel() {
                        Some(c) => c.close(),
                        None => {}
                    },
                    Opcode::PANIC => {
                        let val = stack.pop_value();
                        go_panic!(panic, val, frame, code);
                    }
                    Opcode::RECOVER => {
                        let p = panic.take();
                        let val = p.map_or(GosValue::new_nil(ValueType::Void), |x| x.msg);
                        stack.push(val);
                    }
                    Opcode::ASSERT => {
                        let ok = stack.pop_bool();
                        if !ok {
                            go_panic_str!(panic, "Opcode::ASSERT: not true!", frame, code);
                        }
                    }
                    Opcode::FFI => {
                        let meta = stack.pop_value();
                        let total_params = inst.imm();
                        let index = Stack::offset(stack.len(), -total_params);
                        let itype = stack.get(index).clone();
                        let name = stack.get(index + 1).clone();
                        let ptypes = &objs.metas[meta.as_metadata().key]
                            .as_signature()
                            .params_type[2..];
                        let params = stack.pop_value_n(ptypes.len());
                        let name_str = StrUtil::as_str(name.as_string());
                        let v = match self.context.ffi_factory.create_by_name(&name_str, params) {
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
                        };
                        stack.pop_string();
                        stack.pop_metadata();
                        stack.push(v);
                    }
                    Opcode::VOID => unreachable!(),
                };
                //dbg!(inst_op, stack.len());
            } //yield unit
            match result {
                Result::End => {
                    if let Some(p) = panic {
                        println!("panic: {}", p.msg);
                        if let Some(files) = self.context.fs {
                            for (fkey, pc) in p.call_stack.iter() {
                                let func = &objs.functions[*fkey];
                                if let Some(p) = func.pos()[*pc] {
                                    println!("{}", files.position(p));
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
fn get_struct_and_index(
    op_index: OpIndex,
    val: GosValue,
    stack: &mut Stack,
    code: &Vec<Instruction>,
    frame: &mut CallFrame,
    objs: &VMObjects,
) -> (RuntimeResult<GosValue>, usize) {
    let (target, index) = if op_index >= 0 {
        (Ok(val), op_index as usize)
    } else {
        let count = -op_index as usize;
        let mut indices = Vec::with_capacity(count);
        for c in &code[frame.pc..frame.pc + count - 1] {
            indices.push(c.get_u64() as usize);
        }
        let index = code[frame.pc + count - 1].get_u64() as usize;
        frame.pc += count;
        let val = get_embeded(val, &indices, stack, &objs.packages);
        (val, index)
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
    indices: &Vec<usize>,
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

#[cfg(test)]
mod test {}
