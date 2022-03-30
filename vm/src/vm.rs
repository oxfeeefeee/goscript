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
    ($panic:ident, $msg:expr, $frame:ident, $code:ident) => {
        let mut data = PanicData::new($msg);
        data.call_stack.push(($frame.func(), $frame.pc - 1));
        $panic = Some(data);
        $frame.pc = $code.len() - 1;
    };
}

macro_rules! go_panic_str {
    ($panic:ident, $mdata:expr, $msg:expr, $frame:ident, $code:ident) => {
        let str_val = GosValue::new_str($msg);
        let iface = GosValue::new_empty_iface($mdata, str_val);
        let mut data = PanicData::new(iface);
        data.call_stack.push(($frame.func(), $frame.pc - 1));
        $panic = Some(data);
        $frame.pc = $code.len() - 1;
    };
}

macro_rules! read_imm_key {
    ($code:ident, $frame:ident, $objs:ident) => {{
        let inst = $code[$frame.pc];
        $frame.pc += 1;
        u64_to_key(inst.get_u64())
    }};
}

macro_rules! unwrap_recv_val {
    ($chan:expr, $val:expr, $metas:expr, $gcv:expr) => {
        match $val {
            Some(v) => (v, true),
            None => {
                let val_meta = $metas[$chan.meta.as_non_ptr()].as_channel().1;
                (val_meta.zero_val(&$metas, $gcv), false)
            }
        }
    };
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
fn deref_value(v: &GosValue, stack: &Stack, objs: &VMObjects) -> GosValue {
    v.as_pointer().deref(stack, &objs.packages)
}

#[derive(Debug)]
pub struct ByteCode {
    pub objects: Pin<Box<VMObjects>>,
    pub packages: Vec<PackageKey>,
    pub ifaces: Vec<(GosMetadata, Option<Rc<Vec<FunctionKey>>>)>,
    pub entry: FunctionKey,
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
    fn ret_count(&self, objs: &VMObjects) -> usize {
        let fkey = self.func();
        objs.functions[fkey].ret_count()
    }

    #[inline]
    fn on_drop(&mut self, stack: &Stack) {
        if let Some(referred) = &self.referred_by {
            for (ind, referrers) in referred {
                if referrers.weaks.len() == 0 {
                    continue;
                }
                let val = stack.get_with_type(Stack::offset(self.stack_base, *ind), referrers.typ);
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
    stack_c: Vec<GosValue64>,
    stack_rc: Vec<GosValue>,
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
        let cls = GosValue::new_closure(entry, &self.code.objects.functions);
        CallFrame::with_closure(cls.as_closure().clone(), 0)
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
        let metadata: &Metadata = &objs.metadata;
        let pkgs = &ctx.code.packages;
        let ifaces = &ctx.code.ifaces;
        let frame = self.frames.last_mut().unwrap();
        let mut func = &objs.functions[frame.func()];

        let mut stack_mut_ref = self.stack.borrow_mut();
        let mut stack: &mut Stack = &mut stack_mut_ref;
        // allocate local variables
        stack.append(func.local_zeros.clone());

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
                    Opcode::PUSH_NIL => stack.push_nil(),
                    Opcode::PUSH_FALSE => stack.push_bool(false),
                    Opcode::PUSH_TRUE => stack.push_bool(true),
                    Opcode::PUSH_IMM => stack.push_int32_as(inst.imm(), inst.t0()),
                    Opcode::PUSH_ZERO_VALUE => {
                        let meta = consts[inst.imm() as usize].as_meta();
                        stack.push(zero_val!(meta, objs, gcv));
                    }
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
                        let ind = stack.pop_with_type(inst.t1());
                        let mut val = &stack.pop_with_type(inst.t0());
                        if inst.t0() == ValueType::Named {
                            val = &val.as_named().0;
                        }
                        if inst.t2_as_index() == 0 {
                            match val.load_index(&ind) {
                                Ok(v) => stack.push(v),
                                Err(e) => {
                                    go_panic_str!(panic, &objs.metadata, e, frame, code);
                                }
                            }
                        } else {
                            stack.push_index_comma_ok(val, &ind);
                        }
                    }
                    Opcode::LOAD_INDEX_IMM => {
                        let mut val = &stack.pop_with_type(inst.t0());
                        if inst.t0() == ValueType::Named {
                            val = &val.as_named().0;
                        }
                        let index = inst.imm() as usize;
                        if inst.t2_as_index() == 0 {
                            match val.load_index_int(index) {
                                Ok(v) => stack.push(v),
                                Err(e) => {
                                    go_panic_str!(panic, metadata, e, frame, code);
                                }
                            }
                        } else {
                            stack.push_index_comma_ok(val, &GosValue::Int(index as isize));
                        }
                    }
                    Opcode::STORE_INDEX => {
                        let (rhs_index, index) = inst.imm824();
                        let s_index = Stack::offset(stack.len(), index);
                        let key = stack.get_with_type(s_index + 1, inst.t2());
                        let mut target = &stack.get_with_type(s_index, inst.t1());
                        if inst.t1() == ValueType::Named {
                            target = &target.as_named().0;
                        }
                        if let Err(e) = stack.store_index(target, &key, rhs_index, inst.t0(), gcv) {
                            go_panic_str!(panic, metadata, e, frame, code);
                        }
                    }
                    Opcode::STORE_INDEX_IMM => {
                        // the only place we can store the immediate index is t2
                        let (rhs_index, imm) = inst.imm824();
                        let index = inst.t2_as_index();
                        let s_index = Stack::offset(stack.len(), index);
                        let mut target = &stack.get_with_type(s_index, inst.t1());
                        if inst.t1() == ValueType::Named {
                            target = &target.as_named().0;
                        }
                        if let Err(e) =
                            stack.store_index_int(target, imm, rhs_index, inst.t0(), gcv)
                        {
                            go_panic_str!(panic, metadata, e, frame, code);
                        }
                    }
                    Opcode::LOAD_FIELD => {
                        let ind = stack.pop_with_type(inst.t1());
                        let val = stack.pop_with_type(inst.t0());
                        stack.push(val.load_field(&ind, objs));
                    }
                    Opcode::LOAD_STRUCT_FIELD => {
                        let ind = inst.imm();
                        let mut target = stack.pop_with_type(inst.t0());
                        if let GosValue::Pointer(_) = &target {
                            target = deref_value(&target, stack, objs);
                            frame = self.frames.last_mut().unwrap();
                        }
                        target = target.unwrap_named();
                        let val = target.as_struct().0.borrow().fields[ind as usize].clone();
                        stack.push(val);
                    }
                    Opcode::BIND_METHOD => {
                        let val = stack.pop_with_type(inst.t0());
                        let func = read_imm_key!(code, frame, objs);
                        stack.push(GosValue::Closure(Rc::new((
                            RefCell::new(ClosureObj::new_gos(
                                func,
                                &objs.functions,
                                Some(val.copy_semantic(gcv)),
                            )),
                            Cell::new(0),
                        ))));
                    }
                    Opcode::BIND_INTERFACE_METHOD => {
                        let val = stack.pop_with_type(inst.t0()).unwrap_named();
                        let borrowed = val.as_interface().borrow();
                        let cls = match borrowed.underlying() {
                            IfaceUnderlying::Gos(val, funcs) => {
                                let func = funcs.as_ref().unwrap()[inst.imm() as usize];
                                let cls = ClosureObj::new_gos(
                                    func,
                                    &objs.functions,
                                    Some(val.copy_semantic(gcv)),
                                );
                                GosValue::Closure(Rc::new((RefCell::new(cls), Cell::new(0))))
                            }
                            IfaceUnderlying::Ffi(ffi) => {
                                let (name, meta) = ffi.methods[inst.imm() as usize].clone();
                                let cls = FfiClosureObj {
                                    ffi: ffi.ffi_obj.clone(),
                                    func_name: name,
                                    meta: meta,
                                };
                                GosValue::Closure(Rc::new((
                                    RefCell::new(ClosureObj::new_ffi(cls)),
                                    Cell::new(0),
                                )))
                            }
                            IfaceUnderlying::None => {
                                let msg = "access nil interface".to_owned();
                                go_panic_str!(panic, metadata, msg, frame, code);
                                continue;
                            }
                        };
                        stack.push(cls);
                    }
                    Opcode::STORE_FIELD => {
                        let (rhs_index, _) = inst.imm824();
                        let index = inst.t2_as_index();
                        let s_index = Stack::offset(stack.len(), index);
                        let key = stack.get_with_type(s_index + 1, inst.t2());
                        let target = stack.get_with_type(s_index, inst.t1());
                        match target {
                            GosValue::Pointer(_) => {
                                let unboxed = deref_value(&target, stack, objs);
                                frame = self.frames.last_mut().unwrap();
                                stack.store_field(
                                    &unboxed,
                                    &key,
                                    rhs_index,
                                    inst.t0(),
                                    &objs.metas,
                                    gcv,
                                );
                            }
                            _ => stack.store_field(
                                &target,
                                &key,
                                rhs_index,
                                inst.t0(),
                                &objs.metas,
                                gcv,
                            ),
                        };
                    }
                    Opcode::STORE_STRUCT_FIELD => {
                        let (rhs_index, imm) = inst.imm824();
                        let index = inst.t2_as_index();
                        let s_index = Stack::offset(stack.len(), index);
                        let mut target = stack.get_with_type(s_index, inst.t1());
                        if let GosValue::Pointer(_) = &target {
                            target = deref_value(&target, stack, objs);
                            frame = self.frames.last_mut().unwrap();
                        }
                        target = target.unwrap_named();
                        let field = &mut target.as_struct().0.borrow_mut().fields[imm as usize];
                        stack.store_val(field, rhs_index, inst.t0(), gcv);
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
                                stack.push(GosValue::Int32(index + 1));
                                stack.push(f.clone());
                                stack.push(GosValue::Bool(true));
                            }
                            None => stack.push(GosValue::Bool(false)),
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
                        let p = stack.get_with_type(s_index, ValueType::Pointer);
                        stack.store_to_pointer(
                            p.as_pointer(),
                            rhs_index,
                            inst.t0(),
                            &objs.packages,
                            gcv,
                        );
                    }
                    Opcode::CAST => {
                        let (target, mapping) = inst.imm824();
                        let target_index = Stack::offset(stack.len(), target);
                        match inst.t0() {
                            ValueType::Interface => {
                                let iface = ifaces[mapping as usize].clone();
                                let under = stack.get_with_type(target_index, inst.t1());
                                let val = match &objs.metas[iface.0.as_non_ptr()] {
                                    MetadataType::Named(_, md) => GosValue::Named(Box::new((
                                        GosValue::new_iface(
                                            *md,
                                            IfaceUnderlying::Gos(under, iface.1),
                                        ),
                                        iface.0,
                                    ))),
                                    MetadataType::Interface(_) => GosValue::new_iface(
                                        iface.0,
                                        IfaceUnderlying::Gos(under, iface.1),
                                    ),
                                    _ => unreachable!(),
                                };
                                stack.set(target_index, val);
                            }
                            ValueType::Str => {
                                let result = match inst.t1() {
                                    ValueType::Slice => {
                                        let slice = stack.get_rc(target_index).as_slice();
                                        match inst.t2() {
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
                                        }
                                    }
                                    _ => {
                                        let target = stack.get_c_mut(target_index);
                                        target.to_uint32(inst.t1());
                                        char_from_u32(target.get_uint32()).to_string()
                                    }
                                };
                                stack.set(target_index, GosValue::new_str(result));
                            }
                            ValueType::Slice => {
                                let from = stack.get_rc(target_index).as_str();
                                let result = match inst.t2() {
                                    ValueType::Int32 => (
                                        objs.metadata.mint32,
                                        from.as_str()
                                            .chars()
                                            .map(|x| GosValue::Int32(x as i32))
                                            .collect(),
                                    ),
                                    ValueType::Uint8 => (
                                        objs.metadata.muint8,
                                        from.as_str().bytes().map(|x| GosValue::Uint8(x)).collect(),
                                    ),
                                    _ => unreachable!(),
                                };
                                let v = GosValue::slice_with_val(result.1, result.0, gcv);
                                stack.set(target_index, v);
                            }
                            ValueType::Pointer => {
                                // the underlying types are identical, we just need to replace the metadata
                                let target = stack.get_rc(target_index);
                                let pointee = target.as_pointer().deref(stack, &objs.packages);
                                let pointee = pointee.unwrap_named();
                                let pointee = if inst.t2() != ValueType::Zero {
                                    let meta_key = read_imm_key!(code, frame, objs);
                                    let meta = GosMetadata::NonPtr(meta_key, MetaCategory::Default);
                                    GosValue::Named(Box::new((pointee, meta)))
                                } else {
                                    pointee
                                };
                                let v = GosValue::new_pointer(PointerObj::UpVal(
                                    UpValue::new_closed(pointee),
                                ));
                                stack.set(target_index, v);
                            }
                            ValueType::Channel => {
                                let target = stack.get_rc(target_index);
                                let chan = target.as_channel().chan.clone();
                                let meta_key = read_imm_key!(code, frame, objs);
                                let meta = GosMetadata::NonPtr(meta_key, MetaCategory::Default);
                                let v = GosValue::channel_with_chan(meta, chan);
                                stack.set(target_index, v);
                            }
                            ValueType::UintPtr => match inst.t1() {
                                ValueType::Pointer => {
                                    let v = stack.pop_rc();
                                    let ud = v.as_pointer().as_user_data();
                                    stack.push(GosValue::UintPtr(
                                        Rc::as_ptr(ud) as *const () as usize
                                    ));
                                }
                                _ => stack.get_c_mut(target_index).to_uint_ptr(inst.t1()),
                            },
                            ValueType::Uint => stack.get_c_mut(target_index).to_uint(inst.t1()),
                            ValueType::Uint8 => stack.get_c_mut(target_index).to_uint8(inst.t1()),
                            ValueType::Uint16 => stack.get_c_mut(target_index).to_uint16(inst.t1()),
                            ValueType::Uint32 => stack.get_c_mut(target_index).to_uint32(inst.t1()),
                            ValueType::Uint64 => stack.get_c_mut(target_index).to_uint64(inst.t1()),
                            ValueType::Int => stack.get_c_mut(target_index).to_int(inst.t1()),
                            ValueType::Int8 => stack.get_c_mut(target_index).to_int8(inst.t1()),
                            ValueType::Int16 => stack.get_c_mut(target_index).to_int16(inst.t1()),
                            ValueType::Int32 => stack.get_c_mut(target_index).to_int32(inst.t1()),
                            ValueType::Int64 => stack.get_c_mut(target_index).to_int64(inst.t1()),
                            ValueType::Float32 => {
                                stack.get_c_mut(target_index).to_float32(inst.t1())
                            }
                            ValueType::Float64 => {
                                stack.get_c_mut(target_index).to_float64(inst.t1())
                            }
                            _ => {
                                dbg!(inst.t0());
                                unimplemented!()
                            }
                        }
                    }
                    Opcode::UNWRAP => {
                        let i = Stack::offset(stack.len(), inst.imm());
                        stack.unwrap_named(i);
                    }
                    Opcode::WRAP => {
                        let i = Stack::offset(stack.len(), inst.imm());
                        if inst.t1() != ValueType::FlagA {
                            stack.wrap_restore_named(i, inst.t0());
                        } else {
                            let meta_key = read_imm_key!(code, frame, objs);
                            let meta = GosMetadata::NonPtr(meta_key, MetaCategory::Default);
                            *stack.get_rc_mut(i) = GosValue::Named(Box::new((
                                stack.get_with_type(i, inst.t0()),
                                meta,
                            )));
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
                    Opcode::EQL => stack.compare_eql(inst.t0()),
                    Opcode::LSS => stack.compare_lss(inst.t0()),
                    Opcode::GTR => stack.compare_gtr(inst.t0()),
                    Opcode::NEQ => stack.compare_neq(inst.t0()),
                    Opcode::LEQ => stack.compare_leq(inst.t0()),
                    Opcode::GEQ => stack.compare_geq(inst.t0()),
                    Opcode::SEND => {
                        let val = stack.pop_with_type(inst.t0());
                        let chan = stack.pop_rc();
                        drop(stack_mut_ref);
                        let re = chan.as_channel().send(&val).await;
                        restore_stack_ref!(self, stack, stack_mut_ref);
                        if let Err(e) = re {
                            go_panic_str!(panic, metadata, e, frame, code);
                        }
                    }
                    Opcode::RECV => {
                        let chan_val = stack.pop_rc();
                        let chan = chan_val.as_channel();
                        drop(stack_mut_ref);
                        let val = chan.recv().await;
                        restore_stack_ref!(self, stack, stack_mut_ref);
                        let (unwrapped, ok) = unwrap_recv_val!(chan, val, objs.metas, gcv);
                        stack.push(unwrapped);
                        if inst.t1() == ValueType::FlagA {
                            stack.push(GosValue::Bool(ok));
                        }
                    }
                    Opcode::REF_UPVALUE => {
                        let index = inst.imm();
                        let upvalue = frame.var_ptrs.as_ref().unwrap()[index as usize].clone();
                        stack.push(GosValue::new_pointer(PointerObj::UpVal(upvalue.clone())));
                    }
                    Opcode::REF_LOCAL => {
                        let t = inst.t0();
                        let val = if inst.imm() >= 0 {
                            let s_index = Stack::offset(stack_base, inst.imm());
                            stack.get_with_type(s_index, t)
                        } else {
                            stack.pop_with_type(t)
                        };
                        let boxed = PointerObj::try_new_local(&val).unwrap();
                        stack.push(GosValue::new_pointer(boxed));
                    }
                    Opcode::REF_SLICE_MEMBER => {
                        let i = stack.pop_int() as OpIndex;
                        let typ = inst.t0();
                        let arr_or_slice = stack.pop_with_type(typ);
                        let v = match typ {
                            ValueType::Array => PointerObj::new_array_member(&arr_or_slice, i, gcv),
                            ValueType::Slice => {
                                PointerObj::SliceMember(arr_or_slice.as_slice().clone(), i)
                            }
                            _ => unreachable!(),
                        };
                        stack.push(GosValue::new_pointer(v));
                    }
                    Opcode::REF_STRUCT_FIELD => {
                        let mut struct_ = stack.pop_with_type(inst.t0());
                        // todo: do this check in codegen
                        if inst.t0() == ValueType::Pointer {
                            struct_ = deref_value(&struct_, stack, objs);
                        }
                        let struct_ = struct_.unwrap_named();
                        stack.push(GosValue::new_pointer(PointerObj::StructField(
                            struct_.as_struct().clone(),
                            inst.imm(),
                        )));
                    }
                    Opcode::REF_PKG_MEMBER => {
                        let pkg = read_imm_key!(code, frame, objs);
                        stack.push(GosValue::new_pointer(PointerObj::PkgMember(
                            pkg,
                            inst.imm(),
                        )));
                    }
                    Opcode::REF_LITERAL => {
                        let v = stack.pop_with_type(inst.t0());
                        stack.push(GosValue::new_pointer(PointerObj::UpVal(
                            UpValue::new_closed(v),
                        )))
                    }
                    Opcode::DEREF => {
                        let boxed = stack.pop_with_type(inst.t0());
                        let val = deref_value(&boxed, stack, objs);
                        stack.push(val);
                        frame = self.frames.last_mut().unwrap();
                    }
                    Opcode::PRE_CALL => {
                        let val = stack.pop_with_type(ValueType::Closure);
                        let cls_rc = val.as_closure();
                        let cls: &ClosureObj = &*cls_rc.0.borrow();
                        let next_frame = CallFrame::with_closure(cls_rc.clone(), stack.len());
                        match cls.func {
                            Some(key) => {
                                let next_func = &objs.functions[key];
                                stack.append(next_func.ret_zeros.clone());
                                if let Some(r) = &cls.recv {
                                    // push receiver on stack as the first parameter
                                    // don't call copy_semantic because BIND_METHOD did it already
                                    stack.push(r.clone());
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
                            let sig = &objs.metas[cls.meta.as_non_ptr()].as_signature();
                            let (meta, v_meta) = sig.variadic.unwrap();
                            let vt = v_meta.value_type(&objs.metas);
                            let is_ffi = cls.func.is_none();
                            let index = nframe.stack_base
                                + sig.params.len()
                                + if is_ffi { 0 } else { sig.results.len() }
                                - 1;
                            stack.pack_variadic(index, meta, vt, gcv);
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
                                    ValueType::Zero => {
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
                                        stack.append(func.local_zeros.clone());
                                    }
                                    ValueType::FlagA => {
                                        // goroutine
                                        nframe.stack_base = 0;
                                        let nstack = Stack::move_from(stack, nfunc.param_count());
                                        self.context.spawn_fiber(nstack, nframe);
                                    }
                                    ValueType::FlagB => {
                                        let (c, rc) = stack.pop_n(nfunc.param_count());
                                        let deferred = DeferredCall {
                                            frame: nframe,
                                            stack_c: c,
                                            stack_rc: rc,
                                        };
                                        frame.defer_stack.get_or_insert(vec![]).push(deferred);
                                    }
                                    _ => unreachable!(),
                                }
                            }
                            None => {
                                let call = cls.ffi.as_ref().unwrap();
                                let ptypes = &objs.metas[call.meta.as_non_ptr()]
                                    .as_signature()
                                    .params_type;
                                let params = stack.pop_with_type_n(ptypes);
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
                                    Ok(result) => stack.append(result),
                                    Err(e) => {
                                        go_panic_str!(panic, &objs.metadata, e, frame, code);
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
                        match inst.t0() {
                            // default case
                            ValueType::Zero => {
                                stack.truncate(stack_base + frame.ret_count(objs));
                            }
                            // init_package func
                            ValueType::FlagA => {
                                let index = inst.imm() as usize;
                                let pkey = pkgs[index];
                                let pkg = &objs.packages[pkey];
                                let count = pkg.var_count();
                                // remove garbage first
                                debug_assert!(stack.len() == stack_base + count);
                                // the var values left on the stack are for pkg members
                                stack.init_pkg_vars(pkg, count);
                            }
                            // func with deferred calls
                            ValueType::FlagB => {
                                match frame.defer_stack.as_mut().map(|x| x.pop()).flatten() {
                                    Some(call) => {
                                        // run Opcode::RETURN to check if deferred_stack is empty
                                        frame.pc -= 1;

                                        stack.push_n(call.stack_c, call.stack_rc);
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
                                        stack.append(func.local_zeros.clone());
                                        continue;
                                    }
                                    None => {
                                        stack.truncate(stack_base + frame.ret_count(objs));
                                    }
                                }
                            }
                            _ => unreachable!(),
                        }

                        frame.on_drop(&stack);
                        drop(frame);
                        self.frames.pop();
                        frame_height -= 1;
                        if self.frames.is_empty() {
                            dbg!(total_inst);
                            /* dbg!
                            let mut s = stats
                                .iter()
                                .map(|(&k, &v)| (k, v))
                                .collect::<Vec<(Opcode, usize)>>();
                            s.sort_by(|a, b| b.1.cmp(&a.1));
                            dbg!(s);
                            */
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
                        if stack.get_c(stack.len() - 1).get_bool() {
                            frame.pc = Stack::offset(frame.pc, inst.imm());
                        } else {
                            stack.pop_discard()
                        }
                    }
                    Opcode::SHORT_CIRCUIT_AND => {
                        if !stack.get_c(stack.len() - 1).get_bool() {
                            frame.pc = Stack::offset(frame.pc, inst.imm());
                        } else {
                            stack.pop_discard()
                        }
                    }
                    Opcode::SWITCH => {
                        if stack.switch_cmp(inst.t0(), objs) {
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
                                        let val = stack.pop_with_type(sel_code.t1());
                                        let chan = stack.pop_rc();
                                        channel::SelectComm::Send(chan, val, offset)
                                    }
                                    ValueType::FlagB | ValueType::FlagC | ValueType::FlagD => {
                                        let chan = stack.pop_rc();
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
                                                c.as_channel(),
                                                val,
                                                objs.metas,
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
                                go_panic_str!(panic, &objs.metadata, e, frame, code);
                            }
                        }
                    }
                    Opcode::RANGE_INIT => {
                        let len = stack.len();
                        let t = stack.get_with_type(len - 1, inst.t0());
                        self.rstack.range_init(&t);
                        stack.pop_discard();
                    }
                    // Opcode::RANGE assumes a container and an int(as the cursor) on the stack
                    Opcode::RANGE => {
                        let offset = inst.imm();
                        if self.rstack.range_body(inst.t0(), stack) {
                            frame.pc = Stack::offset(frame.pc, offset);
                        }
                    }

                    Opcode::TYPE_ASSERT => {
                        let val = match stack.pop_interface().borrow().underlying() {
                            IfaceUnderlying::Gos(v, _) => v.copy_semantic(gcv),
                            _ => GosValue::new_nil(),
                        };
                        let meta = GosValue::Metadata(val.meta(objs, stack));
                        stack.push(val);
                        let ok = &consts[inst.imm() as usize] == &meta;
                        let do_try = inst.t2_as_index() > 0;
                        if !do_try {
                            if !ok {
                                // todo go_panic
                                unimplemented!()
                            }
                        } else {
                            stack.push_bool(ok);
                        }
                    }
                    Opcode::TYPE => {
                        let val = match stack.pop_interface().borrow().underlying() {
                            IfaceUnderlying::Gos(v, _) => v.copy_semantic(gcv),
                            _ => GosValue::new_nil(),
                        };
                        stack.push(GosValue::Metadata(val.meta(objs, stack)));
                        if inst.t2_as_index() > 0 {
                            let index = inst.imm();
                            let s_index = Stack::offset(stack_base, index);
                            stack.set(s_index, val);
                        }
                    }
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
                            GosValue::Slice(sl) => {
                                if max > sl.0.cap() as isize {
                                    let msg = format!("index {} out of range", max).to_string();
                                    go_panic_str!(panic, metadata, msg, frame, code);
                                }
                                GosValue::Slice(Rc::new((
                                    sl.0.slice(begin, end, max),
                                    Cell::new(0),
                                )))
                            }
                            GosValue::Str(s) => GosValue::Str(Rc::new(s.slice(begin, end))),
                            GosValue::Array(_) => {
                                GosValue::slice_with_array(&target, begin, end, gcv)
                            }
                            _ => unreachable!(),
                        };
                        stack.push(result);
                    }
                    Opcode::LITERAL => {
                        let index = inst.imm();
                        let param = &consts[index as usize];
                        let new_val = match param {
                            GosValue::Function(fkey) => {
                                // NEW a closure
                                let mut val = ClosureObj::new_gos(*fkey, &objs.functions, None);
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
                                GosValue::new_runtime_closure(val, gcv)
                            }
                            GosValue::Metadata(md) => {
                                let is_named = inst.t1() == ValueType::Named;
                                let umd =
                                    is_named.then(|| md.underlying(&objs.metas)).unwrap_or(*md);
                                let (key, mc) = umd.unwrap_non_ptr();
                                let count = stack.pop_int32();
                                let val = match &objs.metas[key] {
                                    MetadataType::SliceOrArray(asm, _) => {
                                        let elem_type = asm.value_type(&objs.metas);
                                        let zero_val = asm.zero_val(&objs.metas, gcv);
                                        let mut val = vec![];
                                        let mut cur_index = -1;
                                        for _ in 0..count {
                                            let i = stack.pop_int();
                                            let elem = stack.pop_with_type(elem_type);
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
                                        match mc {
                                            MetaCategory::Default => {
                                                GosValue::slice_with_val(val, *md, gcv)
                                            }
                                            MetaCategory::Array => {
                                                GosValue::array_with_val(val, *md, gcv)
                                            }
                                            _ => unreachable!(),
                                        }
                                    }
                                    MetadataType::Map(km, vm) => {
                                        let gosv =
                                            GosValue::new_map(*md, zero_val!(vm, objs, gcv), gcv);
                                        let map = gosv.as_map();
                                        let tk = km.value_type(&objs.metas);
                                        let tv = vm.value_type(&objs.metas);
                                        for _ in 0..count {
                                            let k = stack.pop_with_type(tk);
                                            let v = stack.pop_with_type(tv);
                                            map.0.insert(k, v);
                                        }
                                        gosv
                                    }
                                    MetadataType::Struct(f, zero) => {
                                        let struct_val = zero.copy_semantic(gcv);
                                        let mut sref = struct_val.as_struct().0.borrow_mut();
                                        for _ in 0..count {
                                            let index = stack.pop_uint();
                                            let tv = f.fields[index].0.value_type(&objs.metas);
                                            sref.fields[index] = stack.pop_with_type(tv);
                                        }
                                        drop(sref);
                                        struct_val
                                    }
                                    _ => unreachable!(),
                                };
                                match !is_named {
                                    true => val,
                                    false => GosValue::Named(Box::new((val, *md))),
                                }
                            }
                            _ => unimplemented!(),
                        };
                        stack.push(new_val);
                    }

                    Opcode::NEW => {
                        let param = stack.pop_with_type(inst.t0());
                        let new_val = match param {
                            GosValue::Metadata(md) => {
                                let is_named = inst.t1() == ValueType::Named;
                                let umd =
                                    is_named.then(|| md.underlying(&objs.metas)).unwrap_or(md);
                                let v = umd.into_value_category().zero_val(&objs.metas, gcv);
                                let v = match !is_named {
                                    true => v,
                                    false => GosValue::Named(Box::new((v, md))),
                                };
                                GosValue::new_pointer(PointerObj::UpVal(UpValue::new_closed(v)))
                            }
                            _ => unimplemented!(),
                        };
                        stack.push(new_val);
                    }
                    Opcode::MAKE => {
                        let index = inst.imm() - 1;
                        let i = Stack::offset(stack.len(), index - 1);
                        let meta_val = stack.get_with_type(i, ValueType::Metadata);
                        let md = meta_val.as_meta();
                        let is_named = inst.t1() == ValueType::Named;
                        let umd = is_named.then(|| md.underlying(&objs.metas)).unwrap_or(*md);
                        let metadata = &objs.metas[umd.as_non_ptr()];
                        let val = match metadata {
                            MetadataType::SliceOrArray(vmeta, _) => {
                                let (cap, len) = match index {
                                    -2 => (stack.pop_int() as usize, stack.pop_int() as usize),
                                    -1 => {
                                        let len = stack.pop_int() as usize;
                                        (len, len)
                                    }
                                    _ => unreachable!(),
                                };
                                GosValue::new_slice(
                                    len,
                                    cap,
                                    umd,
                                    Some(&zero_val!(vmeta, objs, gcv)),
                                    gcv,
                                )
                            }
                            MetadataType::Map(_, v) => {
                                let default = zero_val!(v, objs, gcv);
                                GosValue::new_map(umd, default, gcv)
                            }
                            MetadataType::Channel(_, _) => {
                                let cap = match index {
                                    -1 => stack.pop_int() as usize,
                                    0 => 0,
                                    _ => unreachable!(),
                                };
                                GosValue::new_channel(umd, cap)
                            }
                            _ => unreachable!(),
                        };
                        let val = match !is_named {
                            true => val,
                            false => GosValue::Named(Box::new((val, *md))),
                        };
                        stack.pop_discard();
                        stack.push(val);
                    }
                    Opcode::COMPLEX => {
                        // for the specs: For complex, the two arguments must be of the same
                        // floating-point type and the return type is the complex type with
                        // the corresponding floating-point constituents
                        let t = inst.t0();
                        let val = match t {
                            ValueType::Float32 => {
                                let i = stack.pop_c().get_float32();
                                let r = stack.pop_c().get_float32();
                                GosValue::Complex64(r, i)
                            }
                            ValueType::Float64 => {
                                let i = stack.pop_c().get_float64();
                                let r = stack.pop_c().get_float64();
                                GosValue::Complex128(Box::new((r, i)))
                            }
                            _ => unreachable!(),
                        };
                        stack.push(val);
                    }
                    Opcode::REAL => {
                        let val = match inst.t0() {
                            ValueType::Complex64 => {
                                GosValue::Float32(stack.pop_c().get_complex64().0)
                            }
                            ValueType::Complex128 => {
                                GosValue::Float64(stack.pop_rc().as_complex128().0)
                            }
                            _ => unreachable!(),
                        };
                        stack.push(val);
                    }
                    Opcode::IMAG => {
                        let val = match inst.t0() {
                            ValueType::Complex64 => {
                                GosValue::Float32(stack.pop_c().get_complex64().1)
                            }
                            ValueType::Complex128 => {
                                GosValue::Float64(stack.pop_rc().as_complex128().1)
                            }
                            _ => unreachable!(),
                        };
                        stack.push(val);
                    }
                    Opcode::LEN => {
                        let l = match &stack.pop_with_type(inst.t0()) {
                            GosValue::Slice(slice) => slice.0.len(),
                            GosValue::Map(map) => map.0.len(),
                            GosValue::Str(sval) => sval.len(),
                            GosValue::Channel(chan) => chan.len(),
                            _ => unreachable!(),
                        };
                        stack.push(GosValue::Int(l as isize));
                    }
                    Opcode::CAP => {
                        let l = match &stack.pop_with_type(inst.t0()) {
                            GosValue::Slice(slice) => slice.0.cap(),
                            GosValue::Channel(chan) => chan.cap(),
                            _ => unreachable!(),
                        };
                        stack.push(GosValue::Int(l as isize));
                    }
                    Opcode::APPEND => {
                        let index = Stack::offset(stack.len(), inst.imm() - 2);
                        let a = stack.get_with_type(index, ValueType::Slice);
                        let vala = a.as_slice();
                        match inst.t2() {
                            ValueType::FlagA => unreachable!(),
                            ValueType::FlagB => {} // default case, nothing to do
                            ValueType::FlagC => {
                                // special case, appending string as bytes
                                let b = stack.pop_with_type(ValueType::Str);
                                let bytes: Vec<GosValue> = b
                                    .as_str()
                                    .as_bytes()
                                    .iter()
                                    .map(|x| GosValue::Uint8(*x))
                                    .collect();
                                let b_slice = GosValue::slice_with_val(bytes, vala.0.meta, gcv);
                                stack.push(b_slice);
                            }
                            _ => {
                                // pack args into a slice
                                stack.pack_variadic(index + 1, vala.0.meta, inst.t2(), gcv);
                            }
                        };
                        let mut result = vala.0.clone();
                        let b = stack.pop_with_type(ValueType::Slice);
                        let valb = b.as_slice();
                        result.append(&valb.0);

                        stack.set(index, GosValue::slice_with_obj(result, gcv));
                    }
                    Opcode::COPY => {
                        let t2 = match inst.t2() {
                            ValueType::FlagC => ValueType::Str,
                            _ => ValueType::Slice,
                        };
                        let index = Stack::offset(stack.len(), -2);
                        let a = stack.get_with_type(index, ValueType::Slice);
                        let b = stack.pop_with_type(t2);
                        let vala = a.as_slice();
                        let count = if t2 == ValueType::Str {
                            let bytes: Vec<GosValue> = b
                                .as_str()
                                .as_bytes()
                                .iter()
                                .map(|x| GosValue::Uint8(*x))
                                .collect();
                            let b_slice = SliceObj::with_data(bytes, vala.0.meta);
                            vala.0.copy_from(&b_slice)
                        } else {
                            vala.0.copy_from(&b.as_slice().0)
                        };
                        stack.push_int(count as isize);
                    }
                    Opcode::DELETE => {
                        let key = &stack.pop_with_type(inst.t1());
                        let mut map = &stack.pop_with_type(inst.t0());
                        if inst.t0() == ValueType::Named {
                            map = &map.as_named().0;
                        }
                        map.as_map().0.delete(key);
                    }
                    Opcode::CLOSE => {
                        let chan = stack.pop_with_type(ValueType::Channel);
                        chan.as_channel().close();
                    }
                    Opcode::PANIC => {
                        let val = stack.pop_rc();
                        go_panic!(panic, val, frame, code);
                    }
                    Opcode::RECOVER => {
                        let p = panic.take();
                        let val = p.map_or(GosValue::new_nil(), |x| x.msg);
                        stack.push(val);
                    }
                    Opcode::ASSERT => {
                        if !stack.pop_bool() {
                            let msg = "Opcode::ASSERT: not true!".to_owned();
                            go_panic_str!(panic, metadata, msg, frame, code);
                        }
                    }
                    Opcode::FFI => {
                        let meta = stack.pop_with_type(ValueType::Metadata);
                        let total_params = inst.imm();
                        let index = Stack::offset(stack.len(), -total_params);
                        let itype = stack.get_with_type(index, ValueType::Metadata);
                        let name = stack.get_with_type(index + 1, ValueType::Str);
                        let name_str = name.as_str().as_str();
                        let ptypes = &objs.metas[meta.as_meta().as_non_ptr()]
                            .as_signature()
                            .params_type[2..];
                        let params = stack.pop_with_type_n(ptypes);
                        let v = match self.context.ffi_factory.create_by_name(name_str, params) {
                            Ok(v) => {
                                let meta = itype.as_meta().underlying(&objs.metas).clone();
                                let info = objs.metas[meta.as_non_ptr()]
                                    .as_interface()
                                    .iface_methods_info();
                                GosValue::new_iface(
                                    meta,
                                    IfaceUnderlying::Ffi(UnderlyingFfi::new(v, info)),
                                )
                            }
                            Err(e) => {
                                go_panic_str!(panic, metadata, e, frame, code);
                                continue;
                            }
                        };
                        stack.pop_n(2);
                        stack.push(v);
                    }
                    _ => {
                        dbg!(inst_op);
                        unimplemented!();
                    }
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
                        if let GosValue::Str(s) =
                            p.msg.as_interface().borrow().underlying_value().unwrap()
                        {
                            if s.as_str().starts_with("Opcode::ASSERT") {
                                panic!("ASSERT");
                            }
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

        stack.clear_rc_garbage();
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

#[cfg(test)]
mod test {}
