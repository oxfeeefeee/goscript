#![allow(dead_code)]
use super::opcode::OpIndex;
use super::types::GosValue;
use super::types::Objects;
use std::mem::transmute;

#[derive(Copy, Clone, Debug)]
#[repr(i16)]
pub enum PrimOps {
    Add = 0,
    Sub = 1,
}

impl From<PrimOps> for OpIndex {
    fn from(c: PrimOps) -> Self {
        c as OpIndex
    }
}

impl From<OpIndex> for PrimOps {
    fn from(i: OpIndex) -> Self {
        unsafe { transmute(i as i16) }
    }
}

impl PrimOps {
    pub fn call(&self, stack: &mut Vec<GosValue>, objs: &Objects) {
        match self {
            PrimOps::Add => add(stack),
            PrimOps::Sub => sub(stack),
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
