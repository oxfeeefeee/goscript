#![allow(dead_code)]

use goscript_vm::instruction::*;
use goscript_vm::metadata::*;
use goscript_vm::objects::EntIndex;
use std::collections::HashMap;

/// Built-in functions are not called like normal function for performance reasons
pub struct BuiltInFunc {
    pub name: &'static str,
    pub opcode: Opcode,
    pub params_count: isize,
    pub variadic: bool,
}

impl BuiltInFunc {
    pub fn new(name: &'static str, op: Opcode, params: isize, variadic: bool) -> BuiltInFunc {
        BuiltInFunc {
            name: name,
            opcode: op,
            params_count: params,
            variadic: variadic,
        }
    }
}

pub struct Builtins {
    funcs: Vec<BuiltInFunc>,
    vals: HashMap<&'static str, Opcode>,
    types: HashMap<&'static str, GosMetadata>,
}

impl Builtins {
    pub fn new(md: &Metadata) -> Builtins {
        let funcs = vec![
            BuiltInFunc::new("new", Opcode::NEW, 1, false),
            BuiltInFunc::new("make", Opcode::MAKE, 2, true),
            BuiltInFunc::new("len", Opcode::LEN, 1, false),
            BuiltInFunc::new("cap", Opcode::CAP, 1, false),
            BuiltInFunc::new("append", Opcode::APPEND, 2, true),
            BuiltInFunc::new("assert", Opcode::ASSERT, 1, false),
            BuiltInFunc::new("ffi", Opcode::FFI, 2, false),
        ];
        let mut vals = HashMap::new();
        vals.insert("true", Opcode::PUSH_TRUE);
        vals.insert("false", Opcode::PUSH_FALSE);
        vals.insert("nil", Opcode::PUSH_NIL);
        let mut types = HashMap::new();
        types.insert("bool", md.mbool);
        types.insert("int", md.mint);
        types.insert("float64", md.mfloat64);
        types.insert("complex64", md.mcomplex64);
        types.insert("string", md.mstr);
        Builtins {
            funcs: funcs,
            vals: vals,
            types: types,
        }
    }

    pub fn func_index(&self, name: &str) -> Option<OpIndex> {
        self.funcs.iter().enumerate().find_map(|(i, x)| {
            if x.name == name {
                Some(i as OpIndex)
            } else {
                None
            }
        })
    }

    pub fn get_func_by_index(&self, index: usize) -> &BuiltInFunc {
        &self.funcs[index]
    }

    pub fn val_type_index(&self, name: &str) -> EntIndex {
        if let Some(op) = self.vals.get(name) {
            return EntIndex::BuiltInVal(*op);
        } else if let Some(m) = self.types.get(name) {
            return EntIndex::BuiltInType(*m);
        } else {
            unreachable!();
        }
    }
}
