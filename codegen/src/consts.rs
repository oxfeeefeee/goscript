// Copyright 2022 The Goscript Authors. All rights reserved.
// Use of this source code is governed by a BSD-style
// license that can be found in the LICENSE file.

use goscript_vm::instruction::*;
use goscript_vm::metadata::*;
use goscript_vm::value::*;
use std::cell::RefCell;
use std::collections::HashMap;

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub enum Const {
    Var(GosValue),
    Function(Meta, OpIndex), // deferred resolve
}

pub struct Consts {
    consts: RefCell<Vec<Const>>,
    const_indices: RefCell<HashMap<Const, OpIndex>>,
}

impl Consts {
    pub fn new() -> Consts {
        Consts {
            consts: RefCell::new(vec![
                Const::Var(GosValue::new_nil(ValueType::Void)),
                Const::Var(GosValue::new_bool(true)),
                Const::Var(GosValue::new_bool(false)),
            ]),
            const_indices: RefCell::new(HashMap::new()),
        }
    }

    pub fn nil() -> OpIndex {
        0
    }

    pub fn true_() -> OpIndex {
        1
    }

    pub fn false_() -> OpIndex {
        2
    }

    pub fn add_const(&self, v: GosValue) -> OpIndex {
        self.add(Const::Var(v))
    }

    pub fn add_metadata(&self, meta: Meta) -> OpIndex {
        self.add_const(GosValue::new_metadata(meta))
    }

    pub fn add_package(&self, pkg: PackageKey) -> OpIndex {
        self.add_const(GosValue::new_package(pkg))
    }

    pub fn add_function(&self, obj_meta: Meta, index: OpIndex) -> OpIndex {
        self.add(Const::Function(obj_meta, index))
    }

    pub fn into_runtime_consts(self, vmo: &mut VMObjects) -> Vec<GosValue> {
        self.consts
            .into_inner()
            .into_iter()
            .map(|x| match x {
                Const::Var(v) => v,
                Const::Function(meta, i) => {
                    let method = meta.get_method(i, &vmo.metas);
                    let key = method.borrow().func.unwrap();
                    GosValue::new_function(key)
                }
            })
            .collect()
    }

    fn add(&self, c: Const) -> OpIndex {
        match self.const_indices.borrow_mut().get_mut(&c) {
            Some(v) => *v,
            None => {
                self.consts.borrow_mut().push(c);
                self.consts.borrow().len() as OpIndex - 1
            }
        }
    }
}
