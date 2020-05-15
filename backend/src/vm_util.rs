#![allow(dead_code)]
//use super::opcode::OpIndex;
use super::opcode::*;
use super::types::{BoxedObjs, GosValue, StringObjs};
use std::cmp::Ordering;

macro_rules! offset_uint {
    ($uint:expr, $offset:expr) => {
        ($uint as isize + $offset as isize) as usize
    };
}

macro_rules! read_index {
    ($code:ident, $frame:ident) => {{
        let index = $code[$frame.pc].unwrap_data();
        $frame.pc += 1;
        *index
    }};
}

macro_rules! upframe {
    ($iter:expr, $objs:ident, $f:ident) => {
        $iter.find(|x| x.callable.func($objs) == *$f).unwrap();
    };
}

macro_rules! try_unbox {
    ($val:expr, $box_objs:expr) => {
        if let GosValue::Boxed(bkey) = $val {
            &$box_objs[*bkey]
        } else {
            $val
        }
    };
}

macro_rules! bind_method {
    ($sval:ident, $val:ident, $index:ident, $objs:ident) => {
        GosValue::Closure(
            $objs.closures.insert(ClosureVal {
                func: *$objs.types[$sval.typ]
                    .get_struct_member($index)
                    .as_function(),
                receiver: Some($val.clone()),
                upvalues: vec![],
            }),
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
            GosValue::Slice(k) => GosValue::Slice($objs.slices.insert($objs.slices[*k].clone())),
            GosValue::Map(k) => GosValue::Map($objs.maps.insert($objs.maps[*k].clone())),
            GosValue::Interface(_) => unimplemented!(),
            GosValue::Struct(k) => {
                GosValue::Struct($objs.structs.insert($objs.structs[*k].clone()))
            }
            GosValue::Channel(_) => unimplemented!(),
            GosValue::Function(_) => $val.clone(),
            GosValue::Package(_) => $val.clone(),
            GosValue::Type(_) => $val.clone(),
        }
    };
}

macro_rules! get_store_op_val {
    ($stack:ident, $code:ident, $frame:ident, $consts:ident, $objs:ident, $code_offset:expr) => {
        match $code_offset {
            x if x == 0 => (None, duplicate!($stack[$stack.len() - 1], $objs)),
            x if x == 1 => {
                let i = offset_uint!($stack.len(), read_index!($code, $frame));
                (None, duplicate!($stack[i], $objs))
            }
            x if x == 2 => {
                let op = read_index!($code, $frame);
                let operand = &$stack[$stack.len() - 1];
                (Some(op), operand.clone())
            }
            _ => unreachable!(),
        }
    };
}

macro_rules! set_store_op_val {
    ($target:expr, $op:ident, $val:ident, $is_xxx_op_set:expr) => {
        if $is_xxx_op_set {
            vm_util::store_xxx_op($target, $op.unwrap(), &$val);
        } else {
            *$target = $val;
        }
    };
}

macro_rules! int_float_binary_op {
    ($stack:ident, $op:tt) => {
        let (b, a) = ($stack.pop().unwrap(), $stack.pop().unwrap());
        $stack.push(match (a, b) {
            (GosValue::Int(ia), GosValue::Int(ib)) => GosValue::Int(ia $op ib),
            (GosValue::Float64(fa), GosValue::Float64(fb)) => GosValue::Float64(fa $op fb),
            _ => unreachable!(),
        });
    };
}

macro_rules! int_binary_op {
    ($stack:ident, $op:tt) => {
        let (b, a) = ($stack.pop().unwrap(), $stack.pop().unwrap());
        $stack.push(match (a, b) {
            (GosValue::Int(ia), GosValue::Int(ib)) => GosValue::Int(ia $op ib),
            _ => unreachable!(),
        });
    };
}

macro_rules! int_unary_op {
    ($stack:ident, $left:ident, $op:tt) => {
        let a = $stack.pop().unwrap();
        $stack.push(match a {
            GosValue::Int(ia) => GosValue::Int($left $op ia),
            _ => unreachable!(),
        });
    };
}

macro_rules! bool_binary_op {
    ($stack:ident, $op:tt) => {
        let (b, a) = ($stack.pop().unwrap(), $stack.pop().unwrap());
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

pub fn add(stack: &mut Vec<GosValue>, strings: &mut StringObjs) {
    let (b, a) = (stack.pop().unwrap(), stack.pop().unwrap());
    let c = match (a, b) {
        (GosValue::Int(ia), GosValue::Int(ib)) => GosValue::Int(ia + ib),
        (GosValue::Float64(fa), GosValue::Float64(fb)) => GosValue::Float64(fa + fb),
        (GosValue::Str(s0), GosValue::Str(s1)) => {
            let mut s = strings[s0].data_as_ref().clone();
            s.push_str(&strings[s1].data_as_ref());
            GosValue::new_str(s, strings)
        }
        _ => unreachable!(),
    };
    stack.push(c);
}

pub fn sub(stack: &mut Vec<GosValue>) {
    int_float_binary_op!(stack, -);
}

pub fn mul(stack: &mut Vec<GosValue>) {
    int_float_binary_op!(stack, *);
}

pub fn quo(stack: &mut Vec<GosValue>) {
    int_float_binary_op!(stack, /);
}

pub fn rem(stack: &mut Vec<GosValue>) {
    int_binary_op!(stack, %);
}

pub fn and(stack: &mut Vec<GosValue>) {
    int_binary_op!(stack, &);
}

pub fn or(stack: &mut Vec<GosValue>) {
    int_binary_op!(stack, |);
}

pub fn xor(stack: &mut Vec<GosValue>) {
    int_binary_op!(stack, ^);
}

pub fn and_not(stack: &mut Vec<GosValue>) {
    let (b, a) = (stack.pop().unwrap(), stack.pop().unwrap());
    stack.push(match (a, b) {
        (GosValue::Int(ia), GosValue::Int(ib)) => GosValue::Int(ia & !ib),
        _ => unreachable!(),
    });
}

pub fn shl(stack: &mut Vec<GosValue>) {
    int_binary_op!(stack, <<);
}

pub fn shr(stack: &mut Vec<GosValue>) {
    int_binary_op!(stack, >>);
}

pub fn unary_and(stack: &mut Vec<GosValue>) {
    let i: isize = 0;
    int_unary_op!(stack, i, +);
}

pub fn unary_sub(stack: &mut Vec<GosValue>) {
    let i: isize = 0;
    int_unary_op!(stack, i, -);
}

pub fn unary_xor(stack: &mut Vec<GosValue>) {
    let i: isize = -1;
    int_unary_op!(stack, i, ^);
}

pub fn unary_ref(stack: &mut Vec<GosValue>, boxeds: &mut BoxedObjs) {
    let val = stack.pop().unwrap();
    stack.push(GosValue::Boxed(boxeds.insert(val)));
}

pub fn unary_deref(stack: &mut Vec<GosValue>, boxeds: &BoxedObjs) {
    let val = stack.pop().unwrap();
    stack.push(boxeds[*val.as_boxed()]);
}

pub fn logical_and(stack: &mut Vec<GosValue>) {
    bool_binary_op!(stack, &&);
}

pub fn logical_or(stack: &mut Vec<GosValue>) {
    bool_binary_op!(stack, ||);
}

pub fn logical_not(stack: &mut Vec<GosValue>) {
    let len = stack.len();
    stack[len - 1] = GosValue::Bool(!stack[len - 1].as_bool());
}

pub fn compare_eql(stack: &mut Vec<GosValue>) {
    let (b, a) = (stack.pop().unwrap(), stack.pop().unwrap());
    stack.push(GosValue::Bool(a.cmp(&b) == Ordering::Equal));
}

pub fn compare_lss(stack: &mut Vec<GosValue>) {
    let (b, a) = (stack.pop().unwrap(), stack.pop().unwrap());
    stack.push(GosValue::Bool(a.cmp(&b) == Ordering::Less));
}

pub fn compare_gtr(stack: &mut Vec<GosValue>) {
    let (b, a) = (stack.pop().unwrap(), stack.pop().unwrap());
    stack.push(GosValue::Bool(a.cmp(&b) == Ordering::Greater));
}

pub fn compare_neq(stack: &mut Vec<GosValue>) {
    let (b, a) = (stack.pop().unwrap(), stack.pop().unwrap());
    stack.push(GosValue::Bool(a.cmp(&b) != Ordering::Equal));
}

pub fn compare_leq(stack: &mut Vec<GosValue>) {
    let (b, a) = (stack.pop().unwrap(), stack.pop().unwrap());
    stack.push(GosValue::Bool(a.cmp(&b) != Ordering::Greater));
}

pub fn compare_geq(stack: &mut Vec<GosValue>) {
    let (b, a) = (stack.pop().unwrap(), stack.pop().unwrap());
    stack.push(GosValue::Bool(a.cmp(&b) != Ordering::Less));
}

pub fn store_xxx_op(target: &mut GosValue, code: OpIndex, operand: &GosValue) {
    match code {
        x if x == Opcode::ADD as OpIndex => int_float_store_xxx_op!(target, +=, operand),
        x if x == Opcode::SUB as OpIndex => int_float_store_xxx_op!(target, -=, operand),
        x if x == Opcode::MUL as OpIndex => int_float_store_xxx_op!(target, *=, operand),
        x if x == Opcode::QUO as OpIndex => int_float_store_xxx_op!(target, /=, operand),
        x if x == Opcode::REM as OpIndex => int_store_xxx_op!(target, %=, operand),
        x if x == Opcode::AND as OpIndex => int_store_xxx_op!(target, &=, operand),
        x if x == Opcode::OR as OpIndex => int_store_xxx_op!(target, |=, operand),
        x if x == Opcode::XOR as OpIndex => int_store_xxx_op!(target, ^=, operand),
        x if x == Opcode::SHL as OpIndex => int_store_xxx_op!(target, <<=, operand),
        x if x == Opcode::SHR as OpIndex => int_store_xxx_op!(target, >>=, operand),
        x if x == Opcode::AND_NOT as OpIndex => {
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
