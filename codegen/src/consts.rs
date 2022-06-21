// Copyright 2022 The Goscript Authors. All rights reserved.
// Use of this source code is governed by a BSD-style
// license that can be found in the LICENSE file.

use goscript_vm::gc::GcoVec;
use goscript_vm::instruction::*;
use goscript_vm::metadata::*;
use goscript_vm::value::*;
use std::cell::RefCell;
use std::collections::HashMap;

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub enum Const {
    Var(GosValue),
    ZeroValue(Meta),
    Method(Meta, usize), // deferred resolve
}

pub struct Consts {
    consts: RefCell<Vec<Const>>,
}

impl Consts {
    pub fn new() -> Consts {
        Consts {
            consts: RefCell::new(vec![
                Const::Var(GosValue::new_nil(ValueType::Void)),
                Const::Var(GosValue::new_bool(true)),
                Const::Var(GosValue::new_bool(false)),
            ]),
        }
    }

    pub fn add_const(&self, v: GosValue) -> usize {
        self.add(Const::Var(v))
    }

    pub fn add_zero(&self, typ: Meta) -> usize {
        self.add(Const::ZeroValue(typ))
    }

    pub fn add_method(&self, obj_type: Meta, index: usize) -> usize {
        self.add(Const::Method(obj_type, index))
    }

    fn add(&self, c: Const) -> usize {
        let mut borrow = self.consts.borrow_mut();
        let index = borrow.len();
        borrow.push(c);
        index
    }

    // todo: remove redundancy
    pub fn get_runtime_consts(
        &self,
        mobjs: &MetadataObjs,
        gcv: &GcoVec,
    ) -> (Vec<GosValue>, HashMap<usize, usize>) {
        let mut map = HashMap::new();
        let consts = self
            .consts
            .borrow()
            .iter()
            .enumerate()
            .map(|(i, val)| {
                map.insert(i, i);
                match val {
                    Const::Var(v) => v.clone(),
                    Const::Method(m, index) => GosValue::new_function(
                        m.get_method(*index as OpIndex, mobjs)
                            .borrow()
                            .func
                            .unwrap(),
                    ),
                    Const::ZeroValue(m) => m.zero(mobjs, gcv),
                }
            })
            .collect();
        (consts, map)
    }
}
