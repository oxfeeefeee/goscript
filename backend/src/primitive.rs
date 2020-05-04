#![allow(dead_code)]
use super::opcode::OpIndex;
use super::types::GosValue;
use super::types::Objects;
use std::mem::transmute;

#[derive(Copy, Clone, Debug)]
#[repr(i16)]
pub enum Primitive {
    Add = 0,
    Sub = 1,
    Index = 10,
}

impl From<Primitive> for OpIndex {
    fn from(c: Primitive) -> Self {
        c as OpIndex
    }
}

impl From<OpIndex> for Primitive {
    fn from(i: OpIndex) -> Self {
        unsafe { transmute(i as i16) }
    }
}

impl Primitive {
    pub fn call(&self, stack: &mut Vec<GosValue>, objs: &Objects) {
        match self {
            Primitive::Add => add(stack),
            Primitive::Sub => sub(stack),
            Primitive::Index => index(stack, objs),
        }
    }
}

fn add(stack: &mut Vec<GosValue>) {
    let len = stack.len();
    let a = &stack[len - 2];
    let b = &stack[len - 1];
    let c = match (a, b) {
        (GosValue::Int(ia), GosValue::Int(ib)) => GosValue::Int(ia + ib),
        (GosValue::Float64(fa), GosValue::Float64(fb)) => GosValue::Float64(fa + fb),
        _ => GosValue::Nil,
    };
    stack[len - 2] = c;
    stack.pop();
}

fn sub(stack: &mut Vec<GosValue>) {
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

fn index(stack: &mut Vec<GosValue>, objs: &Objects) {
    let len = stack.len();
    let val = &stack[len - 2];
    let ind = &stack[len - 1];
    let c = match val {
        GosValue::Slice(skey) => {
            let slice = &objs.slices[*skey];
            if let Some(v) = slice.get(ind.get_int() as usize) {
                v
            } else {
                // todo: runtime error
                unimplemented!();
            }
        }
        GosValue::Map(mkey) => (&objs.maps[*mkey]).get(ind).clone(),
        _ => unreachable!(),
    };
    stack[len - 2] = c;
    stack.pop();
}
