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
use std::str;

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
        let str_val = GosValue::new_str($msg);
        let iface = GosValue::empty_iface_with_val(str_val);
        let mut data = PanicData::new(iface);
        data.call_stack.push(($frame.func(), $frame.pc - 1));
        $panic = Some(data);
        $frame.pc = $code.len() - 1;
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
    closure: Rc<(RefCell<ClosureObj>, RCount)>,
    pc: usize,
    stack_base: usize,
    // var pointers are used in two cases
    // - a real "upvalue" of a real closure
    // - a local var that has pointer(s) point to it
    var_ptrs: Option<Vec<UpValue>>,
    // closures that have upvalues pointing to this frame
    referred_by: Option<HashMap<OpIndex, Referers>>,

    defer_stack: Option<Vec<DeferredCall>>,
}

impl CallFrame {
    fn with_closure(c: Rc<(RefCell<ClosureObj>, RCount)>, sbase: usize) -> CallFrame {
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
        self.closure.0.borrow().func.unwrap()
    }

    #[inline]
    fn closure(&self) -> &Rc<(RefCell<ClosureObj>, RCount)> {
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
                let val = stack.get_value(Stack::offset(self.stack_base, *ind));
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
        CallFrame::with_closure(cls.clone().into_closure().unwrap(), 0)
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
                        stack.push_value(consts[index as usize].clone());
                    }
                    Opcode::PUSH_NIL => stack.push_nil(inst.t0()),
                    Opcode::PUSH_FALSE => stack.push_bool(false),
                    Opcode::PUSH_TRUE => stack.push_bool(true),
                    Opcode::PUSH_IMM => stack.push_int32_as(inst.imm(), inst.t0()),
                    Opcode::PUSH_ZERO_VALUE => {
                        let meta = consts[inst.imm() as usize].as_metadata();
                        stack.push_value(meta.zero(&objs.metas, gcv));
                    }
                    Opcode::POP => match inst.imm() {
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
                        stack.push_value(upvalue.value(stack));
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
                        if inst.t2_as_index() == 0 {
                            match val.load_index(&ind, gcv) {
                                Ok(v) => stack.push_value(v),
                                Err(e) => {
                                    go_panic_str!(panic, e, frame, code);
                                }
                            }
                        } else {
                            stack.push_index_comma_ok(val, &ind, gcv);
                        }
                    }
                    Opcode::LOAD_INDEX_IMM => {
                        let val = &stack.pop_value();
                        let index = inst.imm() as usize;
                        if inst.t2_as_index() == 0 {
                            match val.load_index_int(index, gcv) {
                                Ok(v) => stack.push_value(v),
                                Err(e) => {
                                    go_panic_str!(panic, e, frame, code);
                                }
                            }
                        } else {
                            stack.push_index_comma_ok(val, &GosValue::new_int(index as isize), gcv);
                        }
                    }
                    Opcode::STORE_INDEX => {
                        let (rhs_index, index) = inst.imm824();
                        let s_index = Stack::offset(stack.len(), index);
                        let key = stack.get_value(s_index + 1);
                        let target = &stack.get_value(s_index);
                        if let Err(e) = stack.store_index(target, &key, rhs_index, inst.t0(), gcv) {
                            go_panic_str!(panic, e, frame, code);
                        }
                    }
                    Opcode::STORE_INDEX_IMM => {
                        // the only place we can store the immediate index is t2
                        let (rhs_index, imm) = inst.imm824();
                        let index = inst.t2_as_index();
                        let s_index = Stack::offset(stack.len(), index);
                        let target = &stack.get_value(s_index);
                        if let Err(e) =
                            stack.store_index_int(target, imm, rhs_index, inst.t0(), gcv)
                        {
                            go_panic_str!(panic, e, frame, code);
                        }
                    }
                    Opcode::LOAD_FIELD => {
                        let ind = stack.pop_value();
                        let val = stack.pop_value();
                        stack.push_value(val.load_field(&ind, objs));
                    }
                    Opcode::LOAD_STRUCT_FIELD => {
                        let ind = inst.imm();
                        match pop_try_deref_value(stack, inst.t0(), objs) {
                            Ok(target) => {
                                let val =
                                    target.as_struct().0.borrow().fields[ind as usize].clone();
                                stack.push_value(val);
                            }
                            Err(e) => go_panic_str!(panic, e, frame, code),
                        }
                    }
                    Opcode::BIND_METHOD => {
                        let val = stack.pop_value();
                        match cast_receiver(val, inst.t1() == ValueType::Pointer, stack, objs) {
                            Ok(val) => {
                                let func = read_imm_key!(code, frame, objs);
                                stack.push_value(GosValue::new_closure(
                                    ClosureObj::new_gos(
                                        func,
                                        &objs.functions,
                                        Some(val.copy_semantic(gcv)),
                                    ),
                                    gcv,
                                ));
                            }
                            Err(e) => go_panic_str!(panic, e, frame, code),
                        }
                    }
                    Opcode::BIND_INTERFACE_METHOD => {
                        let val = stack.pop_interface().unwrap();
                        let index = inst.imm() as usize;
                        let borrowed = val.borrow();
                        match bind_method(&borrowed, index, stack, objs, gcv) {
                            Ok(cls) => stack.push_value(cls),
                            Err(e) => {
                                go_panic_str!(panic, e, frame, code);
                            }
                        }
                    }
                    Opcode::STORE_FIELD => {
                        let (rhs_index, _) = inst.imm824();
                        let index = inst.t2_as_index();
                        let s_index = Stack::offset(stack.len(), index);
                        let key = stack.get_value(s_index + 1).clone();

                        match get_try_deref_value(stack, s_index, inst.t1(), objs) {
                            Ok(target) => {
                                stack.store_field(&target, &key, rhs_index, inst.t0(), gcv);
                            }
                            Err(e) => go_panic_str!(panic, e, frame, code),
                        }
                    }
                    Opcode::STORE_STRUCT_FIELD => {
                        let (rhs_index, imm) = inst.imm824();
                        let index = inst.t2_as_index();
                        let s_index = Stack::offset(stack.len(), index);
                        match get_try_deref_value(stack, s_index, inst.t1(), objs) {
                            Ok(target) => {
                                let field =
                                    &mut target.as_struct().0.borrow_mut().fields[imm as usize];
                                stack.store_val(field, rhs_index, inst.t0(), gcv);
                            }
                            Err(e) => go_panic_str!(panic, e, frame, code),
                        }
                    }
                    Opcode::LOAD_PKG_FIELD => {
                        let index = inst.imm();
                        let pkg_key = read_imm_key!(code, frame, objs);
                        let pkg = &objs.packages[pkg_key];
                        stack.push_value(pkg.member(index).clone());
                    }
                    Opcode::LOAD_PKG_INIT => {
                        let index = stack.pop_int32();
                        let pkg_key = read_imm_key!(code, frame, objs);
                        let pkg = &objs.packages[pkg_key];
                        match pkg.init_func(index) {
                            Some(f) => {
                                stack.push_value(GosValue::new_int32(index + 1));
                                stack.push_value(f.clone());
                                stack.push_value(GosValue::new_bool(true));
                            }
                            None => stack.push_value(GosValue::new_bool(false)),
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
                        let p = stack.get_value(s_index).clone();
                        match p.as_some_pointer() {
                            Ok(ptr_obj) => {
                                stack.store_to_pointer(
                                    ptr_obj,
                                    rhs_index,
                                    inst.t0(),
                                    &objs.packages,
                                    gcv,
                                );
                            }
                            Err(e) => {
                                go_panic_str!(panic, e, frame, code);
                            }
                        }
                    }
                    Opcode::CAST => {
                        let (target, mapping) = inst.imm824();
                        let target_index = Stack::offset(stack.len(), target);
                        match inst.t0() {
                            ValueType::Interface => {
                                let binding = ifaces[mapping as usize].clone();
                                let under = stack.copy_semantic(target_index, inst.t1(), gcv);
                                let val = GosValue::new_interface(InterfaceObj::Gos(
                                    under,
                                    Some(binding),
                                ));
                                stack.set_value(target_index, val);
                            }
                            ValueType::Str => {
                                let result = match inst.t1() {
                                    ValueType::Slice => match stack.get_slice(target_index) {
                                        Some(slice) => match inst.t2() {
                                            ValueType::Int32 => slice
                                                .0
                                                .borrow()
                                                .iter()
                                                .map(|x| char_from_i32(*(x.borrow().as_int32())))
                                                .collect(),
                                            ValueType::Uint8 => {
                                                let buf: Vec<u8> = slice
                                                    .0
                                                    .borrow()
                                                    .iter()
                                                    .map(|x| *(x.borrow().as_uint8()))
                                                    .collect();
                                                // todo: error handling
                                                str::from_utf8(&buf).unwrap().to_string()
                                            }
                                            _ => unreachable!(),
                                        },
                                        None => "".to_owned(),
                                    },
                                    _ => {
                                        let mut target =
                                            unsafe { stack.get_data(target_index).copy_non_ptr() };
                                        target.to_uint32(inst.t1());
                                        char_from_u32(*target.as_uint32()).to_string()
                                    }
                                };
                                stack.set_value(target_index, GosValue::new_str(result));
                            }
                            ValueType::Slice => {
                                let from = stack.get_str(target_index);
                                let result = match inst.t2() {
                                    ValueType::Int32 => from
                                        .as_str()
                                        .chars()
                                        .map(|x| GosValue::new_int32(x as i32))
                                        .collect(),

                                    ValueType::Uint8 => from
                                        .as_str()
                                        .bytes()
                                        .map(|x| GosValue::new_uint8(x))
                                        .collect(),

                                    _ => unreachable!(),
                                };
                                let v = GosValue::slice_with_data(result, gcv);
                                stack.set_value(target_index, v);
                            }
                            ValueType::UnsafePtr => {
                                unimplemented!()
                            }
                            ValueType::UintPtr => match inst.t1() {
                                ValueType::UnsafePtr => {
                                    let up = stack.pop_unsafe_ptr();
                                    stack.push_value(GosValue::new_uint_ptr(
                                        up.map_or(0, |x| Rc::as_ptr(&x) as *const () as usize),
                                    ));
                                }
                                _ => stack.get_data_mut(target_index).to_uint_ptr(inst.t1()),
                            },
                            ValueType::Uint => stack.get_data_mut(target_index).to_uint(inst.t1()),
                            ValueType::Uint8 => {
                                stack.get_data_mut(target_index).to_uint8(inst.t1())
                            }
                            ValueType::Uint16 => {
                                stack.get_data_mut(target_index).to_uint16(inst.t1())
                            }
                            ValueType::Uint32 => {
                                stack.get_data_mut(target_index).to_uint32(inst.t1())
                            }
                            ValueType::Uint64 => {
                                stack.get_data_mut(target_index).to_uint64(inst.t1())
                            }
                            ValueType::Int => stack.get_data_mut(target_index).to_int(inst.t1()),
                            ValueType::Int8 => stack.get_data_mut(target_index).to_int8(inst.t1()),
                            ValueType::Int16 => {
                                stack.get_data_mut(target_index).to_int16(inst.t1())
                            }
                            ValueType::Int32 => {
                                stack.get_data_mut(target_index).to_int32(inst.t1())
                            }
                            ValueType::Int64 => {
                                stack.get_data_mut(target_index).to_int64(inst.t1())
                            }
                            ValueType::Float32 => {
                                stack.get_data_mut(target_index).to_float32(inst.t1())
                            }
                            ValueType::Float64 => {
                                stack.get_data_mut(target_index).to_float64(inst.t1())
                            }
                            _ => {
                                dbg!(inst.t0());
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
                        if let Err(e) = re {
                            go_panic_str!(panic, e, frame, code);
                        }
                    }
                    Opcode::RECV => {
                        match stack.pop_channel() {
                            Some(chan) => {
                                drop(stack_mut_ref);
                                let val = chan.recv().await;
                                restore_stack_ref!(self, stack, stack_mut_ref);
                                let (unwrapped, ok) = unwrap_recv_val!(chan, val, gcv);
                                stack.push_value(unwrapped);
                                if inst.t1() == ValueType::FlagA {
                                    stack.push_value(GosValue::new_bool(ok));
                                }
                            }
                            None => loop {
                                future::yield_now().await;
                            },
                        };
                    }
                    Opcode::REF_UPVALUE => {
                        let index = inst.imm();
                        let upvalue = frame.var_ptrs.as_ref().unwrap()[index as usize].clone();
                        stack.push_value(GosValue::new_pointer(PointerObj::UpVal(upvalue.clone())));
                    }
                    Opcode::REF_LOCAL => {
                        let t = inst.t0();
                        let val = if inst.imm() >= 0 {
                            let s_index = Stack::offset(stack_base, inst.imm());
                            stack.get_value(s_index).clone()
                        } else {
                            stack.pop_value()
                        };
                        let boxed = PointerObj::try_new_local(&val).unwrap();
                        stack.push_value(GosValue::new_pointer(boxed));
                    }
                    Opcode::REF_SLICE_MEMBER => {
                        let i = stack.pop_int() as OpIndex;
                        let typ = inst.t0();
                        let arr_or_slice = stack.pop_value();
                        match typ {
                            ValueType::Array => stack.push_value(GosValue::new_pointer(
                                PointerObj::new_array_member(&arr_or_slice, i, gcv),
                            )),
                            ValueType::Slice => match arr_or_slice.clone().into_some_slice() {
                                Ok(s) => stack.push_value(GosValue::new_pointer(
                                    PointerObj::SliceMember(s, i),
                                )),
                                Err(e) => {
                                    go_panic_str!(panic, e, frame, code);
                                }
                            },
                            _ => unreachable!(),
                        };
                    }
                    Opcode::REF_STRUCT_FIELD => match pop_try_deref_value(stack, inst.t0(), objs) {
                        Ok(target) => {
                            stack.push_value(GosValue::new_pointer(PointerObj::StructField(
                                target.clone().into_struct(),
                                inst.imm(),
                            )));
                        }
                        Err(e) => go_panic_str!(panic, e, frame, code),
                    },
                    Opcode::REF_PKG_MEMBER => {
                        let pkg = read_imm_key!(code, frame, objs);
                        stack.push_value(GosValue::new_pointer(PointerObj::PkgMember(
                            pkg,
                            inst.imm(),
                        )));
                    }
                    Opcode::REF_LITERAL => {
                        let v = stack.pop_value();
                        stack.push_value(GosValue::new_pointer(PointerObj::UpVal(
                            UpValue::new_closed(v),
                        )))
                    }
                    Opcode::DEREF => {
                        let boxed = stack.pop_value();
                        match deref_value(&boxed, stack, objs) {
                            Ok(val) => stack.push_value(val),
                            Err(e) => go_panic_str!(panic, e, frame, code),
                        }
                    }
                    Opcode::PRE_CALL => {
                        let cls_rc = stack.pop_closure().unwrap();
                        let cls: &ClosureObj = &*cls_rc.0.borrow();
                        let next_frame = CallFrame::with_closure(cls_rc.clone(), stack.len());
                        match cls.func {
                            Some(key) => {
                                let next_func = &objs.functions[key];
                                stack.append_vec(next_func.ret_zeros.clone());
                                if let Some(r) = &cls.recv {
                                    // push receiver on stack as the first parameter
                                    // don't call copy_semantic because BIND_METHOD did it already
                                    stack.push_value(r.clone());
                                }
                            }
                            None => {} //ffi
                        }
                        self.next_frames.push(next_frame);
                    }
                    Opcode::CALL => {
                        let mut nframe = self.next_frames.pop().unwrap();
                        let ref_cls = nframe.closure().clone();
                        let cls: &ClosureObj = &ref_cls.0.borrow();
                        let call_style = inst.t0();
                        let pack = inst.t1() == ValueType::FlagA;
                        if pack {
                            let sig = &objs.metas[cls.meta.key].as_signature();
                            let (_, v_meta) = sig.variadic.unwrap();
                            let vt = v_meta.value_type(&objs.metas);
                            let is_ffi = cls.func.is_none();
                            let index = nframe.stack_base
                                + sig.params.len()
                                + if is_ffi { 0 } else { sig.results.len() }
                                - 1;
                            stack.pack_variadic(index, vt, gcv);
                        }
                        match cls.func {
                            Some(key) => {
                                let nfunc = &objs.functions[key];
                                if let Some(uvs) = &cls.uvs {
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
                            None => {
                                let call = cls.ffi.as_ref().unwrap();
                                let ptypes = &objs.metas[call.meta.key].as_signature().params_type;
                                let params = stack.pop_value_n(ptypes.len());
                                // release stack so that code in ffi can yield
                                drop(stack_mut_ref);
                                let returns = {
                                    let ffi_ref = call.ffi.borrow();
                                    let mut ctx = FfiCallCtx {
                                        func_name: &call.func_name,
                                        vm_objs: objs,
                                        stack: &mut self.stack.borrow_mut(),
                                        gcv: gcv,
                                    };
                                    let fut = ffi_ref.call(&mut ctx, params);
                                    fut.await
                                };
                                restore_stack_ref!(self, stack, stack_mut_ref);
                                match returns {
                                    Ok(result) => stack.append_vec(result),
                                    Err(e) => {
                                        go_panic_str!(panic, e, frame, code);
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
                                    let fkey = frame.closure.0.borrow().func.unwrap();
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
                                                    stack.push_value(unwrapped);
                                                }
                                                ValueType::FlagD => {
                                                    stack.push_value(unwrapped);
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
                                go_panic_str!(panic, e, frame, code);
                            }
                        }
                    }
                    Opcode::RANGE_INIT => {
                        let len = stack.len();
                        let t = stack.get_value(len - 1);
                        self.rstack.range_init(&t);
                        stack.pop_value();
                    }
                    Opcode::RANGE => {
                        let offset = inst.imm();
                        if self.rstack.range_body(inst.t0(), stack) {
                            frame.pc = Stack::offset(frame.pc, offset);
                        }
                    }

                    Opcode::TYPE_ASSERT => {
                        if let Some(err) = match stack.pop_some_interface() {
                            Ok(iface) => match &iface.borrow() as &InterfaceObj {
                                InterfaceObj::Gos(v, b) => {
                                    let val = v.copy_semantic(gcv);
                                    let meta = b.as_ref().unwrap().0;
                                    stack.push_value(val);
                                    let want_meta = consts[inst.imm() as usize].as_metadata();
                                    let do_try = inst.t2_as_index() > 0;
                                    match *want_meta == meta {
                                        true => {
                                            stack.push_value(v.copy_semantic(gcv));
                                            if do_try {
                                                stack.push_bool(true);
                                            }
                                            None
                                        }
                                        false => match do_try {
                                            true => {
                                                stack.push_value(want_meta.zero(&objs.metas, gcv));
                                                stack.push_bool(false);
                                                None
                                            }
                                            false => {
                                                Some("interface conversion: wrong type".to_owned())
                                            }
                                        },
                                    }
                                }
                                InterfaceObj::Ffi(_) => {
                                    Some("FFI interface do not support type assertion".to_owned())
                                }
                            },
                            Err(e) => Some(e),
                        } {
                            go_panic_str!(panic, err, frame, code);
                        }
                    }
                    Opcode::TYPE => {
                        let iface_value = stack.pop_value();
                        let (val, meta) = match iface_value.as_interface() {
                            Some(iface) => match &iface.borrow() as &InterfaceObj {
                                InterfaceObj::Gos(v, b) => {
                                    (v.copy_semantic(gcv), b.as_ref().unwrap().0)
                                }
                                _ => (iface_value.clone(), s_meta.none),
                            },
                            _ => (iface_value, s_meta.none),
                        };
                        let typ = meta.value_type(&objs.metas);
                        stack.push_value(GosValue::new_metadata(meta));
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
                            stack.set_value(s_index, val);

                            frame.pc += option_count;
                        }
                    }
                    Opcode::IMPORT => {
                        let pkey = pkgs[inst.imm() as usize];
                        stack.push_value(GosValue::new_bool(!objs.packages[pkey].inited()));
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
                            ValueType::Slice => match stack.pop_some_slice() {
                                Ok(sl) => {
                                    if max > sl.0.cap() as isize {
                                        Err(format!("index {} out of range", max).to_string())
                                    } else {
                                        Ok(GosValue::new_slice(sl.0.slice(begin, end, max), gcv))
                                    }
                                }
                                Err(e) => Err(e),
                            },
                            ValueType::Str => {
                                let s = stack.pop_str();
                                Ok(GosValue::from_str(Rc::new(s.slice(begin, end))))
                            }
                            ValueType::Array => Ok(GosValue::slice_with_array(
                                &stack.pop_value(),
                                begin,
                                end,
                                gcv,
                            )),
                            _ => unreachable!(),
                        };

                        match result {
                            Ok(v) => stack.push_value(v),
                            Err(e) => go_panic_str!(panic, e, frame, code),
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
                                if let Some(uvs) = &mut val.uvs {
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
                                        //dbg!(&desc, &upframe);
                                    }
                                    frame = self.frames.last_mut().unwrap();
                                }
                                GosValue::new_closure(val, gcv)
                            }
                            ValueType::Metadata => {
                                let count = stack.pop_int32();
                                let mut build_val = |m: &Meta| {
                                    let elem_type = m.value_type(&objs.metas);
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
                                    val
                                };
                                let md = arg.as_metadata();
                                match &objs.metas[md.key] {
                                    MetadataType::Slice(m) => {
                                        let val = build_val(m);
                                        GosValue::slice_with_data(val, gcv)
                                    }
                                    MetadataType::Array(m, _) => {
                                        let val = build_val(m);
                                        GosValue::array_with_data(val, gcv)
                                    }
                                    MetadataType::Map(km, vm) => {
                                        let map_val = GosValue::map_with_default_val(
                                            vm.zero(&objs.metas, gcv),
                                            gcv,
                                        );
                                        let map = map_val.as_map().unwrap();
                                        let tk = km.value_type(&objs.metas);
                                        let tv = vm.value_type(&objs.metas);
                                        for _ in 0..count {
                                            let k = stack.pop_value();
                                            let v = stack.pop_value();
                                            map.0.insert(k, v);
                                        }
                                        map_val
                                    }
                                    MetadataType::Struct(f, _) => {
                                        let struct_val = md.zero(&objs.metas, gcv);
                                        let mut sref = struct_val.as_struct().0.borrow_mut();
                                        for _ in 0..count {
                                            let index = stack.pop_uint();
                                            let tv = f.fields[index].0.value_type(&objs.metas);
                                            sref.fields[index] = stack.pop_value();
                                        }
                                        drop(sref);
                                        struct_val
                                    }
                                    _ => unreachable!(),
                                }
                            }
                            _ => unimplemented!(),
                        };
                        stack.push_value(new_val);
                    }

                    Opcode::NEW => {
                        let md = stack.pop_metadata();
                        let v = md.into_value_category().zero(&objs.metas, gcv);
                        let p = GosValue::new_pointer(PointerObj::UpVal(UpValue::new_closed(v)));
                        stack.push_value(p);
                    }
                    Opcode::MAKE => {
                        let index = inst.imm() - 1;
                        let i = Stack::offset(stack.len(), index - 1);
                        let meta_val = stack.get_value(i);
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
                                GosValue::slice_with_len(
                                    len,
                                    cap,
                                    Some(&vmeta.zero(&objs.metas, gcv)),
                                    gcv,
                                )
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
                        stack.push_value(val);
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
                        stack.push_value(val);
                    }
                    Opcode::REAL => {
                        let val = match inst.t0() {
                            ValueType::Complex64 => GosValue::new_float32(stack.pop_complex64().0),
                            ValueType::Complex128 => {
                                GosValue::new_float64(stack.pop_value().as_complex128().0)
                            }
                            _ => unreachable!(),
                        };
                        stack.push_value(val);
                    }
                    Opcode::IMAG => {
                        let val = match inst.t0() {
                            ValueType::Complex64 => GosValue::new_float32(stack.pop_complex64().1),
                            ValueType::Complex128 => {
                                GosValue::new_float64(stack.pop_value().as_complex128().1)
                            }
                            _ => unreachable!(),
                        };
                        stack.push_value(val);
                    }
                    Opcode::LEN => {
                        let l = match inst.t0() {
                            ValueType::Slice => stack.pop_slice().map_or(0, |x| x.0.len()),
                            ValueType::Map => stack.pop_map().map_or(0, |x| x.0.len()),
                            ValueType::Str => stack.pop_str().len(),
                            ValueType::Channel => stack.pop_channel().map_or(0, |x| x.len()),
                            _ => unreachable!(),
                        };
                        stack.push_value(GosValue::new_int(l as isize));
                    }
                    Opcode::CAP => {
                        let l = match inst.t0() {
                            ValueType::Slice => stack.pop_slice().map_or(0, |x| x.0.cap()),
                            ValueType::Channel => stack.pop_channel().map_or(0, |x| x.cap()),
                            _ => unreachable!(),
                        };
                        stack.push_value(GosValue::new_int(l as isize));
                    }
                    Opcode::APPEND => {
                        let index = Stack::offset(stack.len(), inst.imm() - 2);
                        match inst.t2() {
                            ValueType::FlagA => unreachable!(),
                            ValueType::FlagB => {} // default case, nothing to do
                            ValueType::FlagC => {
                                // special case, appending string as bytes
                                let bytes: Vec<GosValue> = stack
                                    .pop_str()
                                    .as_str()
                                    .as_bytes()
                                    .iter()
                                    .map(|x| GosValue::new_uint8(*x))
                                    .collect();
                                let b_slice = GosValue::slice_with_data(bytes, gcv);
                                stack.push_value(b_slice);
                            }
                            _ => {
                                // pack args into a slice
                                stack.pack_variadic(index + 1, inst.t2(), gcv);
                            }
                        };
                        let b = stack.pop_slice();
                        let a = stack.pop_slice();
                        let result = match b {
                            Some(y) => match a {
                                Some(x) => {
                                    let mut to = x.0.clone();
                                    to.append(&y.0);
                                    GosValue::new_slice(to, gcv)
                                }
                                None => GosValue::new_slice(SliceObj::clone(&y.0), gcv),
                            },
                            None => GosValue::from_slice(None),
                        };
                        stack.push_value(result);
                    }
                    Opcode::COPY => {
                        let t2 = match inst.t2() {
                            ValueType::FlagC => ValueType::Str,
                            _ => ValueType::Slice,
                        };
                        let b = stack.pop_value();
                        let count = match stack.pop_slice() {
                            Some(left) => match t2 {
                                ValueType::Str => {
                                    let bytes: Vec<GosValue> = b
                                        .as_str()
                                        .as_bytes()
                                        .iter()
                                        .map(|x| GosValue::new_uint8(*x))
                                        .collect();
                                    let b_slice = SliceObj::with_data(bytes);
                                    left.0.copy_from(&b_slice)
                                }
                                ValueType::Slice => match b.as_slice() {
                                    Some(right) => left.0.copy_from(&right.0),
                                    None => 0,
                                },
                                _ => unreachable!(),
                            },
                            None => 0,
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
                        stack.push_value(val);
                    }
                    Opcode::ASSERT => {
                        let ok = stack.pop_bool();
                        if !ok {
                            let msg = "Opcode::ASSERT: not true!".to_owned();
                            go_panic_str!(panic, msg, frame, code);
                        }
                    }
                    Opcode::FFI => {
                        let meta = stack.pop_value();
                        let total_params = inst.imm();
                        let index = Stack::offset(stack.len(), -total_params);
                        let itype = stack.get_value(index).clone();
                        let name = stack.get_value(index + 1).clone();
                        let name_str = name.as_str().as_str();
                        let ptypes = &objs.metas[meta.as_metadata().key]
                            .as_signature()
                            .params_type[2..];
                        let params = stack.pop_value_n(ptypes.len());
                        let v = match self.context.ffi_factory.create_by_name(name_str, params) {
                            Ok(v) => {
                                let meta = itype.as_metadata().underlying(&objs.metas).clone();
                                let info = objs.metas[meta.key].as_interface().iface_methods_info();
                                GosValue::new_interface(InterfaceObj::Ffi(UnderlyingFfi::new(
                                    v, info,
                                )))
                            }
                            Err(e) => {
                                go_panic_str!(panic, e, frame, code);
                                continue;
                            }
                        };
                        stack.pop_str();
                        stack.pop_metadata();
                        stack.push_value(v);
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
                        let borrow = p.msg.as_interface().as_ref().unwrap().borrow();
                        let val = borrow.underlying_value().unwrap();
                        if val.typ() == ValueType::Str
                            && val.as_str().as_str().starts_with("Opcode::ASSERT")
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

fn deref_value(v: &GosValue, stack: &Stack, objs: &VMObjects) -> RuntimeResult<GosValue> {
    Ok(v.as_some_pointer()?.deref(stack, &objs.packages))
}

#[inline]
fn pop_try_deref_value(
    stack: &mut Stack,
    t: ValueType,
    objs: &VMObjects,
) -> RuntimeResult<GosValue> {
    let v = match t {
        ValueType::Pointer => stack.pop_some_pointer()?.deref(stack, &objs.packages),
        _ => stack.pop_value(),
    };
    Ok(v)
}

#[inline]
fn get_try_deref_value(
    stack: &mut Stack,
    index: usize,
    t: ValueType,
    objs: &VMObjects,
) -> RuntimeResult<GosValue> {
    let v = match t {
        ValueType::Pointer => stack
            .get_value(index)
            .as_some_pointer()?
            .deref(stack, &objs.packages),
        _ => stack.get_value(index).clone(),
    };
    Ok(v)
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
                        Some(inds) => StructObj::get_embeded(obj.clone(), inds).copy_semantic(gcv),
                    };
                    let obj = cast_receiver(obj, *ptr_recv, stack, objs)?;
                    let cls = ClosureObj::new_gos(*func, &objs.functions, Some(obj));
                    Ok(GosValue::new_closure(cls, gcv))
                }
                Binding4Runtime::Iface(i, indices) => {
                    let bind = |obj: &GosValue| {
                        bind_method(
                            &obj.as_interface().as_ref().unwrap().borrow(),
                            *i,
                            stack,
                            objs,
                            gcv,
                        )
                    };
                    match indices {
                        None => bind(obj),
                        Some(inds) => bind(&StructObj::get_embeded(obj.clone(), inds)),
                    }
                }
            }
        }
        InterfaceObj::Ffi(ffi) => {
            let (name, meta) = ffi.methods[index].clone();
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
