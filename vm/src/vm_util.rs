//#![allow(dead_code)]
//use super::opcode::OpIndex;
use super::gc::GcoVec;
use super::instruction::*;
use super::objects::MetadataObjs;
use super::stack::Stack;
use super::value::{GosValue, RtEmptyResult, RtValueResult, VMObjects};

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

macro_rules! store_local_to {
    ($stack:expr, $to:expr, $s_index:expr, $rhs_index:expr, $typ:expr, $gcos:expr) => {{
        if $rhs_index < 0 {
            let rhs_s_index = Stack::offset($stack.len(), $rhs_index);
            Stack::store_to_copy_semantic($stack, $to, $s_index, rhs_s_index, $typ, $gcos);
        } else {
            let op_ex = Instruction::index2code($rhs_index);
            Stack::store_to_with_op($stack, $to, $s_index, $stack.len() - 1, op_ex, $typ);
        }
    }};
}

macro_rules! store_local {
    ($stack:expr, $s_index:expr, $rhs_index:expr, $typ:expr, $gcos:expr) => {{
        if $rhs_index < 0 {
            let rhs_s_index = Stack::offset($stack.len(), $rhs_index);
            $stack.store_copy_semantic($s_index, rhs_s_index, $typ, $gcos);
        } else {
            let op_ex = Instruction::index2code($rhs_index);
            $stack.store_with_op($s_index, $stack.len() - 1, op_ex, $typ);
        }
    }};
}

macro_rules! load_up_value {
    ($upvalue:expr, $self_:ident, $stack:ident, $frames:expr) => {{
        let uv: &UpValueState = &$upvalue.inner.borrow();
        match &uv {
            UpValueState::Open(desc) => {
                let index = (desc.stack_base + desc.index) as usize;
                let uv_stack = desc.stack.upgrade().unwrap();
                if ptr::eq(uv_stack.as_ptr(), $stack) {
                    $stack.get_with_type(index, desc.typ)
                } else {
                    uv_stack.borrow().get_with_type(index, desc.typ)
                }
            }
            UpValueState::Closed(val) => val.clone(),
        }
    }};
}

macro_rules! store_up_value {
    ($upvalue:expr, $self_:ident, $stack:ident, $frames:expr, $rhs_index:ident, $typ:expr, $gcos:expr) => {{
        let uv: &mut UpValueState = &mut $upvalue.inner.borrow_mut();
        match uv {
            UpValueState::Open(desc) => {
                let index = (desc.stack_base + desc.index) as usize;
                let uv_stack = desc.stack.upgrade().unwrap();
                if ptr::eq(uv_stack.as_ptr(), $stack) {
                    store_local!($stack, index, $rhs_index, $typ, $gcos);
                } else {
                    store_local_to!(
                        $stack,
                        &mut uv_stack.borrow_mut(),
                        index,
                        $rhs_index,
                        $typ,
                        $gcos
                    );
                }
            }
            UpValueState::Closed(v) => {
                $stack.store_val(v, $rhs_index, $typ, $gcos);
            }
        }
    }};
}

macro_rules! deref_value {
    ($pointers:expr, $self_:ident, $stack:ident, $frames:expr, $objs:expr) => {{
        match $pointers {
            GosValue::Pointer(b) => {
                let r: &PointerObj = &b;
                match r {
                    PointerObj::UpVal(uv) => load_up_value!(&uv, $self_, $stack, $frames),
                    PointerObj::Struct(s, md) => match md {
                        GosMetadata::Untyped => GosValue::Struct(s.clone()),
                        _ => GosValue::Named(Box::new((GosValue::Struct(s.clone()), *md))),
                    },
                    PointerObj::Array(a, md) => match md {
                        GosMetadata::Untyped => GosValue::Array(a.clone()),
                        _ => GosValue::Named(Box::new((GosValue::Array(a.clone()), *md))),
                    },
                    PointerObj::Slice(s, md) => match md {
                        GosMetadata::Untyped => GosValue::Slice(s.clone()),
                        _ => GosValue::Named(Box::new((GosValue::Slice(s.clone()), *md))),
                    },
                    PointerObj::Map(s, md) => match md {
                        GosMetadata::Untyped => GosValue::Map(s.clone()),
                        _ => GosValue::Named(Box::new((GosValue::Map(s.clone()), *md))),
                    },
                    PointerObj::SliceMember(s, index) => s.0.get(*index as usize).unwrap(),
                    PointerObj::StructField(s, index) => {
                        s.0.borrow().fields[*index as usize].clone()
                    }
                    PointerObj::PkgMember(pkg, index) => {
                        $objs.packages[*pkg].member(*index).clone()
                    }
                    PointerObj::UserData(ud) => {
                        let i = Rc::as_ptr(ud) as *const () as usize;
                        GosValue::Uint(i)
                    }
                    // todo: report error instead of crash?
                    PointerObj::Released => unreachable!(),
                }
            }
            _ => unreachable!(),
        }
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
pub fn char_from_u32(u: u32) -> char {
    unsafe { char::from_u32_unchecked(u) }
}

#[inline]
pub fn char_from_i32(i: i32) -> char {
    unsafe { char::from_u32_unchecked(i as u32) }
}

#[inline(always)]
pub fn load_index(val: &GosValue, ind: &GosValue) -> RtValueResult {
    match val {
        GosValue::Map(map) => Ok(map.0.get(&ind).clone()),
        GosValue::Slice(slice) => {
            let index = ind.as_index();
            slice
                .0
                .get(index)
                .map_or_else(|| Err(format!("index {} out of range", index)), |x| Ok(x))
        }
        GosValue::Str(s) => {
            let index = ind.as_index();
            s.get_byte(index).map_or_else(
                || Err(format!("index {} out of range", index)),
                |x| Ok(GosValue::Int((*x).into())),
            )
        }
        GosValue::Array(arr) => {
            let index = ind.as_index();
            arr.0
                .get(index)
                .map_or_else(|| Err(format!("index {} out of range", index)), |x| Ok(x))
        }
        _ => unreachable!(),
    }
}

#[inline]
pub fn load_index_int(val: &GosValue, i: usize) -> RtValueResult {
    match val {
        GosValue::Slice(slice) => slice
            .0
            .get(i)
            .map_or_else(|| Err(format!("index {} out of range", i)), |x| Ok(x)),
        GosValue::Map(map) => {
            let ind = GosValue::Int(i as isize);
            Ok(map.0.get(&ind).clone())
        }
        GosValue::Str(s) => s.get_byte(i).map_or_else(
            || Err(format!("index {} out of range", i)),
            |x| Ok(GosValue::Int((*x).into())),
        ),
        GosValue::Array(arr) => arr
            .0
            .get(i)
            .map_or_else(|| Err(format!("index {} out of range", i)), |x| Ok(x)),
        GosValue::Named(n) => load_index_int(&n.0, i),
        _ => {
            dbg!(val);
            unreachable!();
        }
    }
}

#[inline]
pub fn load_field(val: &GosValue, ind: &GosValue, objs: &VMObjects) -> GosValue {
    match val {
        GosValue::Struct(sval) => match &ind {
            GosValue::Int(i) => sval.0.borrow().fields[*i as usize].clone(),
            _ => unreachable!(),
        },
        GosValue::Package(pkey) => {
            let pkg = &objs.packages[*pkey];
            pkg.member(*ind.as_int() as OpIndex).clone()
        }
        _ => unreachable!(),
    }
}

#[inline]
pub fn store_index(
    stack: &Stack,
    target: &GosValue,
    key: &GosValue,
    r_index: OpIndex,
    t: ValueType,
    gcos: &GcoVec,
) {
    match target {
        GosValue::Array(arr) => {
            let target_cell = &arr.0.borrow_data()[*key.as_int() as usize];
            stack.store_val(&mut target_cell.borrow_mut(), r_index, t, gcos);
        }
        GosValue::Slice(s) => {
            let target_cell = &s.0.borrow_data()[*key.as_int() as usize];
            stack.store_val(&mut target_cell.borrow_mut(), r_index, t, gcos);
        }
        GosValue::Map(map) => {
            map.0.touch_key(&key);
            let borrowed = map.0.borrow_data();
            let target_cell = borrowed.get(&key).unwrap();
            stack.store_val(&mut target_cell.borrow_mut(), r_index, t, gcos);
        }
        _ => unreachable!(),
    }
}

#[inline]
pub fn store_index_int(
    stack: &Stack,
    target: &GosValue,
    i: usize,
    r_index: OpIndex,
    t: ValueType,
    gcos: &GcoVec,
) -> RtEmptyResult {
    let err = Err("assignment to entry in nil map or slice".to_string());
    match target {
        GosValue::Array(arr) => {
            let target_cell = &arr.0.borrow_data()[i];
            stack.store_val(&mut target_cell.borrow_mut(), r_index, t, gcos);
            Ok(())
        }
        GosValue::Slice(s) => {
            if s.0.is_nil() {
                err
            } else {
                let target_cell = &s.0.borrow_data()[i];
                stack.store_val(&mut target_cell.borrow_mut(), r_index, t, gcos);
                Ok(())
            }
        }
        GosValue::Map(map) => {
            if map.0.is_nil() {
                err
            } else {
                let key = GosValue::Int(i as isize);
                map.0.touch_key(&key);
                let borrowed = map.0.borrow_data();
                let target_cell = borrowed.get(&key).unwrap();
                stack.store_val(&mut target_cell.borrow_mut(), r_index, t, gcos);
                Ok(())
            }
        }
        GosValue::Nil(_) => err,
        _ => {
            dbg!(target);
            unreachable!()
        }
    }
}

#[inline]
pub fn store_field(
    stack: &Stack,
    target: &GosValue,
    key: &GosValue,
    r_index: OpIndex,
    t: ValueType,
    metas: &MetadataObjs,
    gcos: &GcoVec,
) {
    match target {
        GosValue::Struct(s) => {
            match key {
                GosValue::Int(i) => {
                    let target = &mut s.0.borrow_mut().fields[*i as usize];
                    stack.store_val(target, r_index, t, gcos);
                }
                GosValue::Str(sval) => {
                    let i = s.0.borrow().meta.field_index(sval.as_str(), metas);
                    let target = &mut s.0.borrow_mut().fields[i as usize];
                    stack.store_val(target, r_index, t, gcos);
                }
                _ => unreachable!(),
            };
        }
        _ => unreachable!(),
    }
}

#[inline]
pub fn push_index_comma_ok(stack: &mut Stack, map: &GosValue, index: &GosValue) {
    let (v, b) = match map.as_map().0.try_get(index) {
        Some(v) => (v, true),
        None => (GosValue::new_nil(), false),
    };
    stack.push(v);
    stack.push_bool(b);
}
