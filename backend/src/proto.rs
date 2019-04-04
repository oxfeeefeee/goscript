#![allow(dead_code)]
use super::types::GosValue;
use super::instruction::Instruction,


pub struct UpValueDesc {
    name: String, // debug info
    is_local: bool, // local or in outer functions
    index: usize,
}

pub struct LocalVar {
    name: String,
    start_pc: usize,
    end_pc: usize,
}

pub struct Proto {
    regs_needed: usize,
    is_var_arg: bool,
    up_values: Vec<UpValueDesc>,
    constants: Vec<GosValue>,
    code: Vec<Instruction>,
    sub_protos: Vec<Proto>,
    local_vars: Vec<LocalVar>, // debug info
}
