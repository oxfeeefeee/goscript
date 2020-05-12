#![allow(dead_code)]
//use super::opcode::OpIndex;
use super::types::{BoxedObjs, GosValue, StringObjs};

pub fn add(stack: &mut Vec<GosValue>, strings: &mut StringObjs) {
    let len = stack.len();
    let a = &stack[len - 2];
    let b = &stack[len - 1];
    let c = match (a, b) {
        (GosValue::Int(ia), GosValue::Int(ib)) => GosValue::Int(ia + ib),
        (GosValue::Float64(fa), GosValue::Float64(fb)) => GosValue::Float64(fa + fb),
        (GosValue::Str(s0), GosValue::Str(s1)) => {
            let mut s = strings[*s0].data.clone();
            s.push_str(&strings[*s1].data);
            GosValue::new_str(s, strings)
        }
        _ => GosValue::Nil,
    };
    stack[len - 2] = c;
    stack.pop();
}

pub fn sub(stack: &mut Vec<GosValue>) {
    let len = stack.len();
    let a = &stack[len - 2];
    let b = &stack[len - 1];
    let c = match (a, b) {
        (GosValue::Int(ia), GosValue::Int(ib)) => GosValue::Int(ia - ib),
        (GosValue::Float64(fa), GosValue::Float64(fb)) => GosValue::Float64(fa - fb),
        _ => GosValue::Nil,
    };
    stack[len - 2] = c;
    stack.pop();
}

pub fn reference(stack: &mut Vec<GosValue>, boxeds: &mut BoxedObjs) {
    let val = stack.pop().unwrap();
    stack.push(GosValue::Boxed(boxeds.insert(val)));
}

pub fn deref(stack: &mut Vec<GosValue>, boxeds: &BoxedObjs) {
    let val = stack.pop().unwrap();
    stack.push(boxeds[*val.as_boxed()]);
}
