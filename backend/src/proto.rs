#![allow(dead_code)]
use std::rc::{Rc};
use super::types::GosValue;
use super::instruction::Instruction;

#[derive(Clone, Debug)]
pub struct UpValueDesc {
    name: String, // debug info
    is_local: bool, // local or in outer functions
    index: usize,
}

#[derive(Clone, Debug)]
pub struct LocalVar {
    name: String,
    start_pc: usize,
    end_pc: usize,
}

#[derive(Clone, Debug)]
pub struct Proto {
    pub regs_needed: usize,
    pub is_var_arg: bool,
    pub up_values: Vec<UpValueDesc>,
    pub constants: Vec<GosValue>,
    pub code: Vec<Instruction>,
    pub sub_protos: Vec<Proto>,
    pub local_vars: Vec<LocalVar>, // debug info
}

#[derive(Clone, Debug)]
pub enum UpValue {
    Open(u32),
    Closed(GosValue),
}

#[derive(Clone, Debug)]
pub struct Closure {
    pub proto: Rc<Proto>,
    pub up_values: Vec<UpValue>,
}

impl UpValue {
    pub fn value<'a>(&'a self, reg: &'a Vec<GosValue>) -> &'a GosValue {
        match self {
            UpValue::Open(i) => &reg[*i as usize],
            UpValue::Closed(v) => v,
        }
    }

    pub fn set_value(&mut self, v: GosValue) {
        match self {
            UpValue::Open(_) => unreachable!(),
            UpValue::Closed(oldv) => {*oldv = v;},
        }
    }
}