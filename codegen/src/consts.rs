// Copyright 2022 The Goscript Authors. All rights reserved.
// Use of this source code is governed by a BSD-style
// license that can be found in the LICENSE file.

use goscript_vm::ffi::*;
use goscript_vm::value::*;
use std::cell::RefCell;
use std::collections::HashMap;
use std::hash::Hash;
use std::hash::Hasher;

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
            consts: RefCell::new(vec![]),
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

    pub fn get_runtime_consts(
        &self,
        vmctx: &mut CodeGenVMCtx,
    ) -> (Vec<GosValue>, HashMap<usize, usize>) {
        #[derive(Debug)]
        enum ConstType {
            Nil,
            Copyable,
            Other,
        }

        // Runtime never compare two GosValues with different types,
        // so GosValue::Eq and GosValue::Hash cannot be used here.
        struct CopyableVal {
            val: GosValue,
        }

        impl Eq for CopyableVal {}

        impl PartialEq for CopyableVal {
            fn eq(&self, b: &CopyableVal) -> bool {
                self.val.typ() == b.val.typ() && self.val == b.val
            }
        }

        impl Hash for CopyableVal {
            fn hash<H: Hasher>(&self, state: &mut H) {
                self.val.typ().hash(state);
                self.val.hash(state);
            }
        }

        let mut nils = vec![];
        let mut nil_map = HashMap::new();
        let mut copyables = vec![];
        let mut copyables_map = HashMap::new();
        let mut others = vec![];
        let consts_indices: Vec<(ConstType, usize, usize)> = self
            .consts
            .borrow()
            .iter()
            .enumerate()
            .map(|(i, c)| {
                let val = match c {
                    Const::Var(v) => v.clone(),
                    Const::Method(m, index) => GosValue::new_function(
                        m.get_method(*index as OpIndex, vmctx.metas())
                            .borrow()
                            .func
                            .unwrap(),
                    ),
                    Const::ZeroValue(m) => vmctx.ffi_ctx().zero_val(m),
                };

                if val.is_nil() {
                    (
                        ConstType::Nil,
                        i,
                        *nil_map.entry(val.typ()).or_insert_with(|| {
                            nils.push(val);
                            nils.len() - 1
                        }),
                    )
                } else if val.typ().copyable() {
                    (
                        ConstType::Copyable,
                        i,
                        *copyables_map
                            .entry(CopyableVal { val: val.clone() })
                            .or_insert_with(|| {
                                copyables.push(val);
                                copyables.len() - 1
                            }),
                    )
                } else {
                    others.push(val);
                    (ConstType::Other, i, others.len() - 1)
                }
            })
            .collect();

        let mut map = HashMap::new();
        for (t, i, j) in consts_indices {
            let offset = match t {
                ConstType::Nil => 0,
                ConstType::Copyable => nils.len(),
                ConstType::Other => nils.len() + copyables.len(),
            };
            map.insert(i, j + offset);
        }
        let mut consts = vec![];
        consts.append(&mut nils);
        consts.append(&mut copyables);
        consts.append(&mut others);
        (consts, map)
    }
}
