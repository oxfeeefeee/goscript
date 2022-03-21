use super::gc::GcoVec;
use super::instruction::*;
use super::objects::MetadataObjs;
use super::stack::Stack;
use super::value::{GosValue, VMObjects};
use crate::value::RuntimeResult;

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
pub fn char_from_u32(u: u32) -> char {
    unsafe { char::from_u32_unchecked(u) }
}

#[inline]
pub fn char_from_i32(i: i32) -> char {
    unsafe { char::from_u32_unchecked(i as u32) }
}

#[inline]
pub fn deref_value(v: &GosValue, stack: &Stack, objs: &VMObjects) -> GosValue {
    v.as_pointer().deref(stack, &objs.packages)
}

#[inline(always)]
pub fn load_index(val: &GosValue, ind: &GosValue) -> RuntimeResult<GosValue> {
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
pub fn load_index_int(val: &GosValue, i: usize) -> RuntimeResult<GosValue> {
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
            let target_cell = &s.0.borrow()[*key.as_int() as usize];
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
) -> RuntimeResult<()> {
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
                let target_cell = &s.0.borrow()[i];
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
