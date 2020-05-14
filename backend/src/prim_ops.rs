#![allow(dead_code)]
//use super::opcode::OpIndex;
use super::types::{BoxedObjs, GosValue, StringObjs};
use std::cmp::Ordering;

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
    int_binary_op!(stack, /);
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
