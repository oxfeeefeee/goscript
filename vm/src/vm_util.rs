#![allow(dead_code)]
//use super::opcode::OpIndex;
use super::instruction::*;
use super::value::{VMObjects, GosValue, ClosureVal, MetadataObjs};
use std::cmp::Ordering;
use std::cell::RefCell;
use std::rc::Rc;
use super::stack::Stack;

macro_rules! offset_uint {
    ($uint:expr, $offset:expr) => {
        ($uint as isize + $offset as isize) as usize
    };
}

macro_rules! index {
    ($stack:ident, $i:expr) => {
        $stack.get($i)
        //$stack.get($i).unwrap()
    };
}


macro_rules! stack_set {
    ($stack:ident, $i:expr, $v:expr) => {
        $stack.set($i, $v);
    };
}

macro_rules! upframe {
    ($iter:expr, $f:ident) => {
        $iter.find(|x| x.callable.func() == *$f).unwrap();
    };
}

macro_rules! bind_method {
    ($sval:ident, $val:ident, $index:ident, $objs:ident) => {
        GosValue::Closure(
            Rc::new(RefCell::new(ClosureVal {
                func: *$objs.metas[*$sval.borrow().meta.as_meta()].typ()
                    .get_struct_member($index)
                    .as_function(),
                receiver: Some($val.clone()),
                upvalues: vec![],
            })),
        )
    };
}

macro_rules! pack_variadic {
    ($stack:ident, $index:ident, $objs:ident) => {
        if $index < $stack.len() {
            let mut v = Vec::new();
            v.append(&mut $stack.split_off($index));
            $stack.push(GosValue::with_slice_val(v, &mut $objs.slices))
        }
    };
}

/// Duplicates the GosValue, primitive types and read-only types are simply cloned
macro_rules! duplicate {
    ($val:expr, $objs:ident) => {
        match &$val {
            GosValue::Nil
            | GosValue::Bool(_)
            | GosValue::Int(_)
            | GosValue::Float64(_)
            | GosValue::Complex64(_, _)
            | GosValue::Str(_)
            | GosValue::Boxed(_)
            | GosValue::Closure(_) => $val.clone(),
            GosValue::Slice(s) => GosValue::Slice(Rc::new((**s).clone())),
            GosValue::Map(m) => GosValue::Map(Rc::new((**m).clone())),
            GosValue::Interface(_) => unimplemented!(),
            GosValue::Struct(s) => 
                GosValue::Struct(Rc::new((**s).clone())),
            GosValue::Channel(_) => unimplemented!(),
            GosValue::Function(_) => $val.clone(),
            GosValue::Package(_) => $val.clone(),
            GosValue::Metadata(_) => $val.clone(),
            //_ => unreachable!(),
        }
    };
}

macro_rules! int_float_binary_op {
    ($stack:ident, $op:tt) => {
        let (b, a) = ($stack.pop(), $stack.pop());
        $stack.push(match (a, b) {
            (GosValue::Int(ia), GosValue::Int(ib)) => GosValue::Int(ia $op ib),
            (GosValue::Float64(fa), GosValue::Float64(fb)) => GosValue::Float64(fa $op fb),
            _ => unreachable!(),
        });
    };
}

macro_rules! int_binary_op {
    ($stack:ident, $op:tt) => {
        let (b, a) = ($stack.pop(), $stack.pop());
        $stack.push(match (a, b) {
            (GosValue::Int(ia), GosValue::Int(ib)) => GosValue::Int(ia $op ib),
            _ => unreachable!(),
        });
    };
}

macro_rules! int_unary_op {
    ($stack:ident, $left:ident, $op:tt) => {
        let a = $stack.pop();
        $stack.push(match a {
            GosValue::Int(ia) => GosValue::Int($left $op ia),
            _ => unreachable!(),
        });
    };
}

macro_rules! bool_binary_op {
    ($stack:ident, $op:tt) => {
        let (b, a) = ($stack.pop(), $stack.pop());
        $stack.push(match (a, b) {
            (GosValue::Bool(ba), GosValue::Bool(bb)) => GosValue::Bool(ba $op bb),
            _ => unreachable!(),
        });
    };
}

macro_rules! int_float_store_xxx_op {
    ($target:ident, $op:tt, $operand:ident) => {
        match ($target, $operand) {
            (GosValue::Int(ia), GosValue::Int(ib)) => {*ia $op ib;}
            (GosValue::Float64(fa), GosValue::Float64(fb)) =>  {*fa $op fb;}
            _ => unreachable!(),
        };
    };
}

macro_rules! int_store_xxx_op {
    ($target:ident, $op:tt, $operand:ident) => {
        match ($target, $operand) {
            (GosValue::Int(ia), GosValue::Int(ib)) => {*ia $op ib;}
            _ => unreachable!(),
        };
    };
}

macro_rules! range_vars {
    ($m_ref:ident, $m_ptr:ident, $m_iter:ident, $l_ref:ident, $l_ptr:ident, $l_iter:ident,
        $s_ref:ident, $s_ptr:ident, $s_iter:ident) => {
        let mut $m_ref: Option<Rc<RefCell<GosHashMap>>> = None;
        let mut $m_ptr: Option<*mut Ref<GosHashMap>> = None;
        let mut $m_iter: Option<std::collections::hash_map::Iter<GosValue, RefCell<GosValue>>> = None;
        let mut $l_ref: Option<Rc<SliceVal>> = None;
        let mut $l_ptr: Option<*mut SliceRef> = None;
        let mut $l_iter: Option<SliceEnumIter> = None;
        let mut $s_ref: Option<Rc<StringVal>> = None;
        let mut $s_ptr: Option<*mut Rc<StringVal>> = None;
        let mut $s_iter: Option<StringEnumIter> = None;
    };
}

macro_rules! range_init {
    ($objs:ident, $target:ident, $map_ref:ident, $map_ptr:ident, $map_iter:ident,
        $slice_ref:ident, $slice_ptr:ident, $slice_iter:ident, 
        $str_ref:ident, $str_ptr:ident, $str_iter:ident) => {{
        match &$target {
            GosValue::Map(m) => {
                let map = m.clone_inner();
                $map_ref.replace(map);
                let r = $map_ref.as_ref().unwrap().borrow();
                let p = Box::into_raw(Box::new(r));
                $map_ptr = Some(p);
                let mapref = unsafe { p.as_ref().unwrap() };
                $map_iter = Some(mapref.iter());
            }
            GosValue::Slice(sl) => {
                let slice = sl.clone();
                $slice_ref.replace(slice);
                let r = $slice_ref.as_ref().unwrap().borrow();
                let p = Box::into_raw(Box::new(r));
                $slice_ptr = Some(p);
                let sliceref = unsafe { p.as_ref().unwrap() };
                $slice_iter = Some(sliceref.iter().enumerate());
            }
            GosValue::Str(s) => {
                let string = s.clone();
                $str_ref.replace(string);
                let r = $str_ref.as_ref().unwrap().clone();
                let p = Box::into_raw(Box::new(r));
                $str_ptr = Some(p);
                let strref = unsafe { p.as_ref().unwrap() };
                $str_iter = Some(strref.iter().enumerate());
            }
            _ => unreachable!(),
        }
    }};
}

macro_rules! range_body {
    ($target:expr, $stack:ident, 
        $map_ptr:ident, $map_iter:ident,
        $slice_ptr:ident, $slice_iter:ident,
        $str_ptr:ident, $str_iter:ident) => {{
        match &$target {
            GosValue::Map(_) => {
                let v = $map_iter.as_mut().unwrap().next();
                if let Some((k, v)) = v {
                    $stack.push(k.clone());
                    $stack.push(v.clone().into_inner());
                    false
                } else {
                    $map_iter.take();
                    // release the pointer
                    if let Some(p) = $map_ptr {
                        unsafe {
                            drop(Box::<Ref<GosHashMap>>::from_raw(p));
                        }
                        $map_ptr = None
                    }
                    $stack.pop();
                    $stack.pop();
                    true
                }
            }
            GosValue::Slice(_) => {
                let v = $slice_iter.as_mut().unwrap().next();
                if let Some((k, v)) = v {
                    $stack.push(GosValue::Int(k as isize));
                    $stack.push(v.clone().into_inner());
                    false
                } else {
                    $slice_iter.take();
                    // release the pointer
                    if let Some(p) = $slice_ptr {
                        unsafe {
                            drop(Box::<SliceRef>::from_raw(p));
                        }
                        $slice_ptr = None
                    }
                    $stack.pop();
                    $stack.pop();
                    true
                }
            }
            GosValue::Str(_) => {
                let v = $str_iter.as_mut().unwrap().next();
                if let Some((k, v)) = v {
                    $stack.push(GosValue::Int(k as isize));
                    $stack.push(GosValue::Int(v as isize));
                    false
                } else {
                    $str_iter.take();
                    // release the pointer
                    if let Some(p) = $str_ptr {
                        unsafe {
                            drop(Box::<Rc<StringVal>>::from_raw(p));
                        }
                        $str_ptr = None
                    }
                    $stack.pop();
                    $stack.pop();
                    true
                }
            }
            _ => unreachable!(),
        }
    }};
}

#[inline]
pub fn add(stack: &mut Stack) {
    let (b, a) = (stack.pop(), stack.pop());
    let c = match (a, b) {
        (GosValue::Int(ia), GosValue::Int(ib)) => GosValue::Int(ia + ib),
        (GosValue::Float64(fa), GosValue::Float64(fb)) => GosValue::Float64(fa + fb),
        (GosValue::Str(s0), GosValue::Str(s1)) => {
            let mut s = s0.as_str().to_string();
            s.push_str(&s1.as_str());
            GosValue::new_str(s)
        }
        _ => unreachable!(),
    };
    stack.push(c);
}

#[inline]
pub fn sub(stack: &mut Stack) {
    int_float_binary_op!(stack, -);
}

#[inline]
pub fn mul(stack: &mut Stack) {
    int_float_binary_op!(stack, *);
}

#[inline]
pub fn quo(stack: &mut Stack) {
    int_float_binary_op!(stack, /);
}

#[inline]
pub fn rem(stack: &mut Stack) {
    int_binary_op!(stack, %);
}

#[inline]
pub fn and(stack: &mut Stack) {
    int_binary_op!(stack, &);
}

#[inline]
pub fn or(stack: &mut Stack) {
    int_binary_op!(stack, |);
}

#[inline]
pub fn xor(stack: &mut Stack) {
    int_binary_op!(stack, ^);
}

#[inline]
pub fn and_not(stack: &mut Stack) {
    let (b, a) = (stack.pop(), stack.pop());
    stack.push(match (a, b) {
        (GosValue::Int(ia), GosValue::Int(ib)) => GosValue::Int(ia & !ib),
        _ => unreachable!(),
    });
}

#[inline]
pub fn shl(stack: &mut Stack) {
    int_binary_op!(stack, <<);
}

#[inline]
pub fn shr(stack: &mut Stack) {
    int_binary_op!(stack, >>);
}

#[inline]
pub fn unary_and(stack: &mut Stack) {
    let i: isize = 0;
    int_unary_op!(stack, i, +);
}

#[inline]
pub fn unary_sub(stack: &mut Stack) {
    let i: isize = 0;
    int_unary_op!(stack, i, -);
}

#[inline]
pub fn unary_xor(stack: &mut Stack) {
    let i: isize = -1;
    int_unary_op!(stack, i, ^);
}

#[inline]
pub fn unary_ref(stack: &mut Stack) {
    let val = stack.pop();
    stack.push(GosValue::new_boxed(val));
}

#[inline]
pub fn unary_deref(stack: &mut Stack) {
    let val = stack.pop();
    stack.push(val.as_boxed().borrow().clone());
}

#[inline]
pub fn logical_and(stack: &mut Stack) {
    bool_binary_op!(stack, &&);
}

#[inline]
pub fn logical_or(stack: &mut Stack) {
    bool_binary_op!(stack, ||);
}

#[inline]
pub fn logical_not(stack: &mut Stack) {
    let len = stack.len();
    stack_set!(stack, len - 1, GosValue::Bool(!index![stack, len - 1].as_bool()));
}

#[inline]
pub fn compare_eql(stack: &mut Stack) {
    let (b, a) = (stack.pop(), stack.pop());
    stack.push(GosValue::Bool(a.eq(&b)));
}

#[inline]
pub fn compare_lss(stack: &mut Stack) {
    let (b, a) = (stack.pop(), stack.pop());
    stack.push(GosValue::Bool(a.cmp(&b) == Ordering::Less));
}

#[inline]
pub fn compare_gtr(stack: &mut Stack) {
    let (b, a) = (stack.pop(), stack.pop());
    stack.push(GosValue::Bool(a.cmp(&b) == Ordering::Greater));
}

#[inline]
pub fn compare_neq(stack: &mut Stack) {
    let (b, a) = (stack.pop(), stack.pop());
    stack.push(GosValue::Bool(a.cmp(&b) != Ordering::Equal));
}

#[inline]
pub fn compare_leq(stack: &mut Stack) {
    let (b, a) = (stack.pop(), stack.pop());
    stack.push(GosValue::Bool(a.cmp(&b) != Ordering::Greater));
}

#[inline]
pub fn compare_geq(stack: &mut Stack) {
    let (b, a) = (stack.pop(), stack.pop());
    stack.push(GosValue::Bool(a.cmp(&b) != Ordering::Less));
}

#[inline]
pub fn store_xxx_op(target: &mut GosValue, code: Opcode, operand: &GosValue) {
    match code {
        Opcode::ADD => int_float_store_xxx_op!(target, +=, operand),
        Opcode::SUB => int_float_store_xxx_op!(target, -=, operand),
        Opcode::MUL => int_float_store_xxx_op!(target, *=, operand),
        Opcode::QUO => int_float_store_xxx_op!(target, /=, operand),
        Opcode::REM => int_store_xxx_op!(target, %=, operand),
        Opcode::AND => int_store_xxx_op!(target, &=, operand),
        Opcode::OR => int_store_xxx_op!(target, |=, operand),
        Opcode::XOR => int_store_xxx_op!(target, ^=, operand),
        Opcode::SHL => int_store_xxx_op!(target, <<=, operand),
        Opcode::SHR => int_store_xxx_op!(target, >>=, operand),
        Opcode::AND_NOT => {
            match (target, operand) {
                (GosValue::Int(ia), GosValue::Int(ib)) => {
                    *ia = *ia & !ib;
                }
                _ => unreachable!(),
            };
        }
        _ => unreachable!(),
    };
}

#[inline]
pub fn set_store_op_val(target: &mut GosValue, op: Option<Opcode>, val: GosValue, is_op_set: bool) {
    if is_op_set {
        store_xxx_op(target, op.unwrap(), &val);
    } else {
        *target = val;
    }
}

#[inline]
pub fn set_ref_store_op_val(target: &RefCell<GosValue>, op: Option<Opcode>, val: GosValue, is_op_set: bool) {
    if is_op_set {
        let mut old_val = target.borrow_mut();
        store_xxx_op(&mut old_val, op.unwrap(), &val);
    } else {
        target.replace(val);
    }
}

#[inline]
pub fn load_field(val: &GosValue, ind: &GosValue, objs: &VMObjects) -> GosValue {
    match val {
        GosValue::Slice(slice) => {
            let index = *ind.as_int() as usize;
            if let Some(v) = slice.get(index) {
                v
            } else {
                unimplemented!();
            }
        }
        GosValue::Map(map) => map.get(&ind).clone(),
        GosValue::Str(s) => {
            GosValue::Int(s.get_byte(*ind.as_int() as usize) as isize)
        }
        GosValue::Struct(sval) => {
            match &ind {
                GosValue::Int(i) => sval.borrow().fields[*i as usize].clone(),
                GosValue::Str(s) => {
                    let struct_ref = sval.borrow();
                    let index = struct_ref.field_method_index(s.as_str(), &objs.metas);
                    if index < struct_ref.fields.len() as OpIndex {
                        struct_ref.fields[index as usize].clone()
                    } else {
                        bind_method!(sval, val, index, objs)
                    }
                }
                _ => unreachable!(),
            }
        }
        GosValue::Package(pkey) => {
            let pkg = &objs.packages[*pkey];
            pkg.member(*ind.as_int() as OpIndex).clone()
        }
        _ => unreachable!(),
    }
}

#[inline]
pub fn store_field(target: &GosValue, key: &GosValue, op: Option<Opcode>, val: GosValue, is_op_set: bool, metas: &MetadataObjs) {
    match target {
        GosValue::Slice(s) => {
            let target_cell = &s.borrow_data()[*key.as_int() as usize];
            set_ref_store_op_val(target_cell, op, val, is_op_set);
        }
        GosValue::Map(map) => {
            map.touch_key(&key);
            let borrowed = map.borrow_data();
            let target_cell = borrowed.get(&key).unwrap();
            set_ref_store_op_val(target_cell, op, val, is_op_set);
        }
        GosValue::Struct(s) => {
            match key {
                GosValue::Int(i) => {
                    let target = &mut s.borrow_mut().fields[*i as usize];
                    set_store_op_val(target, op, val, is_op_set);
                }
                GosValue::Str(sval) => {
                    let i = s.borrow().field_method_index(sval.as_str(), metas);
                    let target = &mut s.borrow_mut().fields[i as usize];
                    set_store_op_val(target, op, val, is_op_set);
                }
                _ => unreachable!(),
            };
        }
        _ => unreachable!(),
    }
}