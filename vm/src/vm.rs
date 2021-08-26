#![allow(dead_code)]
use super::ffi::FfiFactory;
use super::gc::{gc, GcoVec};
use super::instruction::*;
use super::metadata::*;
use super::objects::{u64_to_key, ClosureObj, GosHashMap};
use super::stack::{RangeStack, Stack};
use super::value::*;
use super::vm_util;
use goscript_parser::FileSet;
use smol::future;
use smol::LocalExecutor;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::convert::TryInto;
use std::pin::Pin;
use std::ptr;
use std::rc::Rc;
use std::str;

#[derive(Debug)]
pub struct ByteCode {
    pub objects: Pin<Box<VMObjects>>,
    pub packages: Vec<PackageKey>,
    pub ifaces: Vec<(GosMetadata, Rc<Vec<FunctionKey>>)>,
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
}

impl CallFrame {
    fn with_closure(c: Rc<(RefCell<ClosureObj>, RCount)>, sbase: usize) -> CallFrame {
        CallFrame {
            closure: c,
            pc: 0,
            stack_base: sbase,
            var_ptrs: None,
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

#[derive(Debug)]
enum Result {
    Continue,
    End,
    Error(String),
}

#[derive(Clone)]
struct Context<'a> {
    exec: Rc<LocalExecutor<'a>>,
    code: &'a ByteCode,
    gcv: &'a GcoVec,
    ffi_factory: &'a FfiFactory,
    fs: Option<&'a FileSet>,
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
}

impl<'a> Fiber<'a> {
    fn new(c: Context<'a>, stack: Stack, first_frame: CallFrame) -> Fiber<'a> {
        Fiber {
            stack: Rc::new(RefCell::new(stack)),
            rstack: RangeStack::new(),
            frames: vec![first_frame],
            next_frames: Vec::new(),
            context: c,
        }
    }

    async fn main_loop(&mut self) {
        let ctx = &self.context;
        let gcv = ctx.gcv;
        let objs: &VMObjects = &ctx.code.objects;
        let pkgs = &ctx.code.packages;
        let ifaces = &ctx.code.ifaces;
        let frame = self.frames.last_mut().unwrap();
        let mut func = &objs.functions[frame.func()];

        let mut stack_mut_ref = self.stack.borrow_mut();
        let mut stack: &mut Stack = &mut stack_mut_ref;
        // allocate local variables
        stack.append(&mut func.local_zeros.clone());

        let mut consts = &func.consts;
        let mut code = func.code();
        let mut stack_base = frame.stack_base;
        let mut frame_height = self.frames.len();

        let mut total_inst = 0;
        //let mut stats: HashMap<Opcode, usize> = HashMap::new();
        loop {
            let mut frame = self.frames.last_mut().unwrap();
            let mut result: Result = Result::Continue;
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
                        let gos_val = &consts[index as usize];
                        // Slice/Map/Array are special cases here because, they are stored literal,
                        // and when it gets cloned, the underlying rust vec is not copied
                        // which leads to all function calls shares the same vec instance
                        stack.push(gos_val.deep_clone(gcv));
                    }
                    Opcode::PUSH_NIL => stack.push_nil(),
                    Opcode::PUSH_FALSE => stack.push_bool(false),
                    Opcode::PUSH_TRUE => stack.push_bool(true),
                    Opcode::PUSH_IMM => stack.push_int32_as(inst.imm(), inst.t0()),
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
                        store_local!(stack, s_index, rhs_index, inst.t0(), gcv);
                    }
                    Opcode::LOAD_UPVALUE => {
                        let index = inst.imm();
                        let upvalue = frame.var_ptrs.as_ref().unwrap()[index as usize].clone();
                        let val = load_up_value!(upvalue, self, stack, self.frames);
                        stack.push(val);
                        frame = self.frames.last_mut().unwrap();
                    }
                    Opcode::STORE_UPVALUE => {
                        let (rhs_index, index) = inst.imm824();
                        let upvalue = frame.var_ptrs.as_ref().unwrap()[index as usize].clone();
                        store_up_value!(
                            upvalue,
                            self,
                            stack,
                            self.frames,
                            rhs_index,
                            inst.t0(),
                            gcv
                        );
                        frame = self.frames.last_mut().unwrap();
                    }
                    Opcode::LOAD_INDEX => {
                        let ind = stack.pop_with_type(inst.t1());
                        let val = &stack.pop_with_type(inst.t0());
                        if inst.t2_as_index() == 0 {
                            match vm_util::load_index(val, &ind) {
                                Ok(v) => stack.push(v),
                                Err(e) => {
                                    result = Result::Error(e);
                                    break;
                                }
                            }
                        } else {
                            vm_util::push_index_comma_ok(stack, val, &ind);
                        }
                    }
                    Opcode::LOAD_INDEX_IMM => {
                        let val = &stack.pop_with_type(inst.t0());
                        let index = inst.imm() as usize;
                        if inst.t2_as_index() == 0 {
                            match vm_util::load_index_int(val, index) {
                                Ok(v) => stack.push(v),
                                Err(e) => {
                                    result = Result::Error(e);
                                    break;
                                }
                            }
                        } else {
                            vm_util::push_index_comma_ok(
                                stack,
                                val,
                                &GosValue::Int(index as isize),
                            );
                        }
                    }
                    Opcode::STORE_INDEX => {
                        let (rhs_index, index) = inst.imm824();
                        let s_index = Stack::offset(stack.len(), index);
                        let key = stack.get_with_type(s_index + 1, inst.t2());
                        let target = &stack.get_with_type(s_index, inst.t1());
                        vm_util::store_index(stack, target, &key, rhs_index, inst.t0(), gcv);
                    }
                    Opcode::STORE_INDEX_IMM => {
                        // the only place we can store the immediate index is t2
                        let (rhs_index, imm) = inst.imm824();
                        let index = inst.t2_as_index();
                        let s_index = Stack::offset(stack.len(), index);
                        let target = &stack.get_with_type(s_index, inst.t1());
                        if let Err(e) = vm_util::store_index_int(
                            stack,
                            target,
                            imm as usize,
                            rhs_index,
                            inst.t0(),
                            gcv,
                        ) {
                            result = Result::Error(e);
                            break;
                        }
                    }
                    Opcode::LOAD_FIELD => {
                        let ind = stack.pop_with_type(inst.t1());
                        let val = stack.pop_with_type(inst.t0());
                        stack.push(vm_util::load_field(&val, &ind, objs));
                    }
                    Opcode::LOAD_STRUCT_FIELD => {
                        let ind = inst.imm();
                        let mut target = stack.pop_with_type(inst.t0());
                        if let GosValue::Pointer(_) = &target {
                            target = deref_value!(target, self, stack, self.frames, objs);
                            frame = self.frames.last_mut().unwrap();
                        }
                        let val = match &target {
                            GosValue::Named(n) => {
                                n.0.as_struct().0.borrow().fields[ind as usize].clone()
                            }
                            GosValue::Struct(sval) => sval.0.borrow().fields[ind as usize].clone(),
                            _ => {
                                dbg!(&target);
                                unreachable!()
                            }
                        };

                        stack.push(val);
                    }
                    Opcode::BIND_METHOD => {
                        let val = stack.pop_with_type(inst.t0());
                        let func = *consts[inst.imm() as usize].as_function();
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
                        let val = stack.pop_with_type(inst.t0());
                        let val = match &val {
                            GosValue::Named(n) => n.0.clone(),
                            GosValue::Interface(_) => val,
                            _ => unreachable!(),
                        };
                        let borrowed = val.as_interface().0.borrow();
                        let cls = match borrowed.underlying() {
                            IfaceUnderlying::Gos(val, funcs) => {
                                let func = funcs[inst.imm() as usize];
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
                                let msg = "access nil interface".to_string();
                                result = Result::Error(msg);
                                break;
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
                                let unboxed = deref_value!(target, self, stack, self.frames, objs);
                                frame = self.frames.last_mut().unwrap();
                                vm_util::store_field(
                                    stack,
                                    &unboxed,
                                    &key,
                                    rhs_index,
                                    inst.t0(),
                                    &objs.metas,
                                    gcv,
                                );
                            }
                            _ => vm_util::store_field(
                                stack,
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
                            target = deref_value!(target, self, stack, self.frames, objs);
                            frame = self.frames.last_mut().unwrap();
                        }
                        match &target {
                            GosValue::Named(n) => {
                                let field =
                                    &mut n.0.as_struct().0.borrow_mut().fields[imm as usize];
                                stack.store_val(field, rhs_index, inst.t0(), gcv);
                            }
                            GosValue::Struct(s) => {
                                let field = &mut s.0.borrow_mut().fields[imm as usize];
                                stack.store_val(field, rhs_index, inst.t0(), gcv);
                            }
                            _ => {
                                dbg!(&target);
                                unreachable!()
                            }
                        }
                    }
                    Opcode::LOAD_PKG_FIELD => {
                        let index = inst.imm();
                        let pkg_key = read_imm_pkg!(code, frame, objs);
                        let pkg = &objs.packages[pkg_key];
                        stack.push(pkg.member(index).clone());
                    }
                    Opcode::STORE_PKG_FIELD => {
                        let (rhs_index, imm) = inst.imm824();
                        let pkg = &objs.packages[read_imm_pkg!(code, frame, objs)];
                        stack.store_val(&mut pkg.member_mut(imm), rhs_index, inst.t0(), gcv);
                    }
                    Opcode::STORE_DEREF => {
                        let (rhs_index, index) = inst.imm824();
                        let s_index = Stack::offset(stack.len(), index);
                        match stack.get_with_type(s_index, ValueType::Pointer) {
                            GosValue::Pointer(b) => {
                                let r: &PointerObj = &b;
                                match r {
                                    PointerObj::UpVal(uv) => {
                                        store_up_value!(
                                            uv,
                                            self,
                                            stack,
                                            self.frames,
                                            rhs_index,
                                            inst.t0(),
                                            gcv
                                        );
                                        frame = self.frames.last_mut().unwrap();
                                    }
                                    PointerObj::Struct(r, _) => {
                                        let rhs_s_index = Stack::offset(stack.len(), rhs_index);
                                        let val = stack.get_with_type(rhs_s_index, inst.t0());
                                        let mref: &mut StructObj = &mut r.0.borrow_mut();
                                        *mref = val.try_get_struct().unwrap().0.borrow().clone();
                                    }
                                    PointerObj::Array(a, _) => {
                                        let rhs_s_index = Stack::offset(stack.len(), rhs_index);
                                        let val = stack.get_with_type(rhs_s_index, inst.t0());
                                        a.0.set_from(&val.as_array().0);
                                    }
                                    PointerObj::Slice(r, _) => {
                                        let rhs_s_index = Stack::offset(stack.len(), rhs_index);
                                        let val = stack.get_with_type(rhs_s_index, inst.t0());
                                        r.0.set_from(&val.as_slice().0);
                                    }
                                    PointerObj::Map(r, _) => {
                                        let rhs_s_index = Stack::offset(stack.len(), rhs_index);
                                        let val = stack.get_with_type(rhs_s_index, inst.t0());
                                        let mref: &mut GosHashMap = &mut r.0.borrow_data_mut();
                                        *mref = val.try_get_map().unwrap().0.borrow_data().clone();
                                    }
                                    PointerObj::SliceMember(s, index) => {
                                        let vborrow = s.0.borrow_data();
                                        let target: &mut GosValue = &mut vborrow
                                            [s.0.begin() + *index as usize]
                                            .borrow_mut();
                                        stack.store_val(target, rhs_index, inst.t0(), gcv);
                                    }
                                    PointerObj::StructField(s, index) => {
                                        let target: &mut GosValue =
                                            &mut s.0.borrow_mut().fields[*index as usize];
                                        stack.store_val(target, rhs_index, inst.t0(), gcv);
                                    }
                                    PointerObj::PkgMember(p, index) => {
                                        let target: &mut GosValue =
                                            &mut objs.packages[*p].member_mut(*index);
                                        stack.store_val(target, rhs_index, inst.t0(), gcv);
                                    }
                                    PointerObj::Released => unreachable!(),
                                };
                            }
                            _ => unreachable!(),
                        }
                    }
                    Opcode::CAST => {
                        let (target, mapping) = inst.imm824();
                        let rhs_s_index = Stack::offset(stack.len(), target);
                        match inst.t0() {
                            ValueType::Interface => {
                                let iface = ifaces[mapping as usize].clone();
                                let under = stack.get_with_type(rhs_s_index, inst.t1());
                                let val = match &objs.metas[iface.0.as_non_ptr()] {
                                    MetadataType::Named(_, md) => GosValue::Named(Box::new((
                                        GosValue::new_iface(
                                            *md,
                                            IfaceUnderlying::Gos(under, iface.1),
                                            gcv,
                                        ),
                                        iface.0,
                                    ))),
                                    MetadataType::Interface(_) => GosValue::new_iface(
                                        iface.0,
                                        IfaceUnderlying::Gos(under, iface.1),
                                        gcv,
                                    ),
                                    _ => unreachable!(),
                                };
                                stack.set(rhs_s_index, val);
                            }
                            ValueType::Str => {
                                let result = match inst.t1() {
                                    ValueType::Slice => {
                                        let slice = stack.get_rc(rhs_s_index).as_slice();
                                        match inst.t2() {
                                            ValueType::Int32 => slice
                                                .0
                                                .borrow_data()
                                                .iter()
                                                .map(|x| {
                                                    vm_util::char_from_i32(*(x.borrow().as_int32()))
                                                })
                                                .collect(),
                                            ValueType::Uint8 => {
                                                let buf: Vec<u8> = slice
                                                    .0
                                                    .borrow_data()
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
                                        let target = stack.get_c_mut(rhs_s_index);
                                        target.to_uint32(inst.t1());
                                        vm_util::char_from_u32(target.get_uint32()).to_string()
                                    }
                                };
                                stack.set(rhs_s_index, GosValue::new_str(result));
                            }
                            ValueType::Slice => {
                                let from = stack.get_rc(rhs_s_index).as_str();
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
                                stack.set(
                                    rhs_s_index,
                                    GosValue::slice_with_val(result.1, result.0, gcv),
                                )
                            }
                            ValueType::Uint => stack.get_c_mut(rhs_s_index).to_uint(inst.t1()),
                            ValueType::Uint8 => stack.get_c_mut(rhs_s_index).to_uint8(inst.t1()),
                            ValueType::Uint16 => stack.get_c_mut(rhs_s_index).to_uint16(inst.t1()),
                            ValueType::Uint32 => stack.get_c_mut(rhs_s_index).to_uint32(inst.t1()),
                            ValueType::Uint64 => stack.get_c_mut(rhs_s_index).to_uint64(inst.t1()),
                            ValueType::Int => stack.get_c_mut(rhs_s_index).to_int(inst.t1()),
                            ValueType::Int8 => stack.get_c_mut(rhs_s_index).to_int8(inst.t1()),
                            ValueType::Int16 => stack.get_c_mut(rhs_s_index).to_int16(inst.t1()),
                            ValueType::Int32 => stack.get_c_mut(rhs_s_index).to_int32(inst.t1()),
                            ValueType::Int64 => stack.get_c_mut(rhs_s_index).to_int64(inst.t1()),
                            ValueType::Float32 => {
                                stack.get_c_mut(rhs_s_index).to_float32(inst.t1())
                            }
                            ValueType::Float64 => {
                                stack.get_c_mut(rhs_s_index).to_float64(inst.t1())
                            }
                            _ => {
                                // we do not support tags yet, is there anything to implement?
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
                    Opcode::EQL => stack.compare_eql(inst.t0()),
                    Opcode::LSS => stack.compare_lss(inst.t0()),
                    Opcode::GTR => stack.compare_gtr(inst.t0()),
                    Opcode::NEQ => stack.compare_neq(inst.t0()),
                    Opcode::LEQ => stack.compare_leq(inst.t0()),
                    Opcode::GEQ => stack.compare_geq(inst.t0()),
                    Opcode::SEND => {
                        let val = stack.pop_with_type(inst.t0());
                        let chan = stack.pop_rc();
                        release_stack_ref!(stack, stack_mut_ref);
                        let re = chan.as_channel().send(val).await;
                        restore_stack_ref!(self, stack, stack_mut_ref);
                        if re.is_err() {
                            result = Result::Error(re.unwrap_err());
                            break;
                        }
                    }
                    Opcode::RECV => {
                        let chan_val = stack.pop_rc();
                        let chan = chan_val.as_channel();
                        release_stack_ref!(stack, stack_mut_ref);
                        let val = chan.recv().await;
                        restore_stack_ref!(self, stack, stack_mut_ref);
                        let unwrapped = match val {
                            Some(v) => v,
                            None => {
                                let val_meta = objs.metas[chan.meta.as_non_ptr()].as_channel().1;
                                val_meta.zero_val(&objs.metas, gcv)
                            }
                        };
                        stack.push(unwrapped);
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
                        let boxed = PointerObj::new_local(val);
                        stack.push(GosValue::new_pointer(boxed));
                    }
                    Opcode::REF_SLICE_MEMBER => {
                        let index = stack.pop_int();
                        let typ = inst.t0();
                        let mut slice = stack.pop_with_type(typ);
                        // create a slice if it's an array
                        if typ == ValueType::Array {
                            slice = GosValue::slice_with_array(&slice, 0, -1, gcv);
                        }
                        stack.push(GosValue::new_pointer(PointerObj::SliceMember(
                            slice.as_slice().clone(),
                            index.try_into().unwrap(),
                        )));
                    }
                    Opcode::REF_STRUCT_FIELD => {
                        let struct_ = stack.pop_with_type(inst.t0());
                        let struct_ = match &struct_ {
                            GosValue::Named(n) => n.0.clone(),
                            GosValue::Struct(_) => struct_,
                            _ => unreachable!(),
                        };
                        stack.push(GosValue::new_pointer(PointerObj::StructField(
                            struct_.as_struct().clone(),
                            inst.imm(),
                        )));
                    }
                    Opcode::REF_PKG_MEMBER => {
                        let pkg = read_imm_pkg!(code, frame, objs);
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
                        let val = deref_value!(boxed, self, stack, self.frames, objs);
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
                                stack.append(&mut next_func.ret_zeros.clone());
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
                        let is_async = inst.t0() == ValueType::Flag;

                        let sig = &objs.metas[cls.meta.as_non_ptr()].as_signature();
                        if let Some((meta, v_meta)) = sig.variadic {
                            let has_ellipsis = inst.t1() == ValueType::Flag;
                            if !has_ellipsis {
                                let vt = v_meta.get_value_type(&objs.metas);
                                let index =
                                    nframe.stack_base + sig.params.len() + sig.results.len() - 1;
                                stack.pack_variadic(index, meta, vt, gcv);
                            }
                        }
                        match cls.func {
                            Some(key) => {
                                let nfunc = &objs.functions[key];
                                if let Some(uvs) = &cls.uvs {
                                    // todo: make this work for goroutine
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
                                if !is_async {
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
                                    stack.append(&mut func.local_zeros.clone());
                                } else {
                                    nframe.stack_base = 0;
                                    let nstack = Stack::move_from(stack, nfunc.param_count());
                                    self.context.spawn_fiber(nstack, nframe);
                                }
                            }
                            None => {
                                let call = cls.ffi.as_ref().unwrap();
                                let ptypes = &objs.metas[call.meta.as_non_ptr()]
                                    .as_signature()
                                    .params_type;
                                let params = stack.pop_with_type_n(ptypes);
                                // release stack so that code in ffi can yield
                                release_stack_ref!(stack, stack_mut_ref);
                                let mut returns = call.ffi.borrow().call(&call.func_name, params);
                                restore_stack_ref!(self, stack, stack_mut_ref);
                                stack.append(&mut returns);
                            }
                        }
                    }
                    Opcode::RETURN => {
                        //dbg!(stack.len());
                        //for s in stack.iter() {
                        //    dbg!(GosValueDebug::new(&s, &objs));
                        //}
                        let init_pkg = inst.t0() == ValueType::Flag;
                        if !init_pkg {
                            stack.truncate(stack_base + frame.ret_count(objs));
                        } else {
                            let index = inst.imm() as usize;
                            let pkey = pkgs[index];
                            let pkg = &objs.packages[pkey];
                            let count = pkg.var_count();
                            // remove garbage first
                            debug_assert!(stack.len() == stack_base + count);
                            // the var values left on the stack are for pkg members
                            stack.init_pkg_vars(pkg, count);
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
                        if stack.switch_cmp(inst.t0(), objs) {
                            frame.pc = Stack::offset(frame.pc, inst.imm());
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
                        let val = match stack.pop_interface().0.borrow().underlying() {
                            IfaceUnderlying::Gos(v, _) => v.copy_semantic(gcv),
                            _ => GosValue::new_nil(),
                        };
                        let meta = GosValue::Metadata(val.get_meta(objs, stack));
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
                        let val = match stack.pop_interface().0.borrow().underlying() {
                            IfaceUnderlying::Gos(v, _) => v.copy_semantic(gcv),
                            _ => GosValue::new_nil(),
                        };
                        stack.push(GosValue::Metadata(val.get_meta(objs, stack)));
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
                            GosValue::Slice(sl) => GosValue::Slice(Rc::new((
                                sl.0.slice(begin, end, max),
                                Cell::new(0),
                            ))),
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
                                let umd = md.get_underlying(&objs.metas);
                                let (key, mc) = umd.unwrap_non_ptr();
                                let count = stack.pop_int32();
                                let val = match &objs.metas[key] {
                                    MetadataType::SliceOrArray(asm, _) => {
                                        let elem_type = asm.get_value_type(&objs.metas);
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
                                        let tk = km.get_value_type(&objs.metas);
                                        let tv = vm.get_value_type(&objs.metas);
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
                                            let tv = f.fields[index].get_value_type(&objs.metas);
                                            sref.fields[index] = stack.pop_with_type(tv);
                                        }
                                        drop(sref);
                                        struct_val
                                    }
                                    _ => unreachable!(),
                                };
                                if umd == *md {
                                    val
                                } else {
                                    GosValue::Named(Box::new((val, *md)))
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
                                let v = md.default_val(&objs.metas, gcv);
                                GosValue::new_pointer(PointerObj::UpVal(UpValue::new_closed(v)))
                            }
                            _ => unimplemented!(),
                        };
                        stack.push(new_val);
                    }
                    Opcode::MAKE => {
                        let index = inst.imm();
                        let i = Stack::offset(stack.len(), index - 1);
                        let meta_val = stack.get_with_type(i, ValueType::Metadata);
                        let meta = meta_val.as_meta();
                        let metadata = &objs.metas[meta.as_non_ptr()];
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
                                    *meta,
                                    Some(&zero_val!(vmeta, objs, gcv)),
                                    gcv,
                                )
                            }
                            MetadataType::Map(_, v) => {
                                let default = zero_val!(v, objs, gcv);
                                GosValue::new_map(*meta, default, gcv)
                            }
                            MetadataType::Channel(_, _) => {
                                let cap = match index {
                                    -1 => stack.pop_int() as usize,
                                    0 => 0,
                                    _ => unreachable!(),
                                };
                                GosValue::new_channel(*meta, cap)
                            }
                            _ => unreachable!(),
                        };
                        stack.pop_discard();
                        stack.push(val);
                    }
                    Opcode::LEN => match &stack.pop_with_type(inst.t0()) {
                        GosValue::Slice(slice) => {
                            stack.push(GosValue::Int(slice.0.len() as isize));
                        }
                        GosValue::Map(map) => {
                            stack.push(GosValue::Int(map.0.len() as isize));
                        }
                        GosValue::Str(sval) => {
                            stack.push(GosValue::Int(sval.len() as isize));
                        }
                        _ => unreachable!(),
                    },
                    Opcode::CAP => match &stack.pop_with_type(inst.t0()) {
                        GosValue::Slice(slice) => {
                            stack.push(GosValue::Int(slice.0.cap() as isize));
                        }
                        _ => unreachable!(),
                    },
                    Opcode::APPEND => {
                        let index = Stack::offset(stack.len(), inst.imm());
                        let a = stack.get_with_type(index - 2, ValueType::Slice);
                        let vala = a.as_slice();
                        stack.pack_variadic(index, vala.0.meta, inst.t1(), gcv);
                        let b = stack.pop_with_type(ValueType::Slice);
                        let valb = b.as_slice();
                        vala.0
                            .borrow_data_mut()
                            .append(&mut valb.0.borrow_data().clone());
                    }
                    Opcode::CLOSE => {
                        let chan = stack.pop_with_type(ValueType::Channel);
                        chan.as_channel().close();
                    }
                    Opcode::ASSERT => {
                        if !stack.pop_bool() {
                            let msg = "Opcode::ASSERT: not true!".to_string();
                            result = Result::Error(msg);
                            break;
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
                                let meta = itype.as_meta().get_underlying(&objs.metas).clone();
                                let info = objs.metas[meta.as_non_ptr()]
                                    .as_interface()
                                    .iface_ffi_info();
                                GosValue::new_iface(
                                    meta,
                                    IfaceUnderlying::Ffi(UnderlyingFfi::new(v, info)),
                                    gcv,
                                )
                            }
                            Err(m) => {
                                result = Result::Error(m);
                                break;
                            }
                        };
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
                Result::Error(msg) => {
                    println!("panic: {}", msg);
                    if let Some(files) = self.context.fs {
                        for frame in self.frames.iter().rev() {
                            let func = &objs.functions[frame.func()];
                            if let Some(p) = func.pos()[frame.pc - 1] {
                                println!("{}", files.position(p));
                            } else {
                                println!("<no debug info available>");
                            }
                        }
                    }
                    // a hack to make the test case fail
                    if msg.starts_with("Opcode::ASSERT") {
                        panic!("ASSERT");
                    }
                    break;
                }
                Result::End => {
                    break;
                }
                Result::Continue => {
                    release_stack_ref!(stack, stack_mut_ref);
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
