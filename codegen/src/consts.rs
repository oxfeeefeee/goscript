// Copyright 2022 The Goscript Authors. All rights reserved.
// Use of this source code is governed by a BSD-style
// license that can be found in the LICENSE file.

use goscript_parser::objects::*;
use goscript_vm::instruction::*;
use goscript_vm::metadata::*;
use goscript_vm::value::*;
use std::collections::HashMap;

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub enum Const {
    Var(GosValue),
    Function(Meta, OpIndex),
    PkgVar(PackageKey, IdentKey),
}

pub struct Consts {
    consts: Vec<Const>,
    const_indices: HashMap<Const, OpIndex>,
}

impl Consts {
    pub fn new() -> Consts {
        Consts {
            consts: vec![
                Const::Var(GosValue::new_nil(ValueType::Void)),
                Const::Var(GosValue::new_bool(true)),
                Const::Var(GosValue::new_bool(false)),
            ],
            const_indices: HashMap::new(),
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

    pub fn add_const(&mut self, v: GosValue) -> OpIndex {
        self.add(Const::Var(v))
    }

    pub fn add_metadata(&mut self, meta: Meta) -> OpIndex {
        self.add_const(GosValue::new_metadata(meta))
    }

    pub fn add_function(&mut self, obj_meta: Meta, index: OpIndex) -> OpIndex {
        self.add(Const::Function(obj_meta, index))
    }

    pub fn add_pkg_var(&mut self, pkg: PackageKey, ident: IdentKey) -> OpIndex {
        self.add(Const::PkgVar(pkg, ident))
    }

    fn add(&mut self, c: Const) -> OpIndex {
        *self.const_indices.entry(c).or_insert_with(|| {
            self.consts.push(c);
            self.consts.len() as OpIndex - 1
        })
    }
}
