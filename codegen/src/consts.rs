// Copyright 2022 The Goscript Authors. All rights reserved.
// Use of this source code is governed by a BSD-style
// license that can be found in the LICENSE file.

use goscript_parser::Map;
use goscript_vm::value::*;
use goscript_vm::*;
use std::cell::RefCell;
use std::hash::Hash;
#[cfg(not(feature = "btree_map"))]
use std::hash::Hasher;

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub enum Const {
    Nil(GosValue),
    Comparable(GosValue),
    ZeroVal(GosValue, Meta),
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

    pub fn add_nil(&self, v: GosValue) -> usize {
        assert!(v.is_nil());
        self.add(Const::Nil(v))
    }

    pub fn add_comparable(&self, v: GosValue) -> usize {
        assert!(v.comparable());
        self.add(Const::Comparable(v))
    }

    pub fn add_zero_val(&self, v: GosValue, m: Meta) -> usize {
        if v.nilable() {
            self.add_nil(v)
        } else if v.typ() == ValueType::Struct || v.typ() == ValueType::Array {
            // Structs and Arrays
            self.add(Const::ZeroVal(v, m))
        } else {
            // Not all Structs and Arrays are comparable
            self.add_comparable(v)
        }
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
    ) -> (Vec<GosValue>, Map<usize, usize>) {
        // First, resolve methods
        for c in self.consts.borrow_mut().iter_mut() {
            match c {
                Const::Method(meta, index) => {
                    *c = Const::Comparable(FfiCtx::new_function(
                        meta.get_method(*index as OpIndex, vmctx.metas())
                            .borrow()
                            .func
                            .unwrap(),
                    ));
                }
                _ => {}
            }
        }

        #[derive(Debug)]
        enum ConstType {
            Nil,
            Comparable,
            Other,
        }

        // Runtime never compare two GosValues with different types,
        // so GosValue::Eq, GosValue::Hash and GosValue::Ord cannot be used here.
        struct ComparableVal {
            val: GosValue,
        }

        impl Eq for ComparableVal {}

        impl PartialEq for ComparableVal {
            fn eq(&self, b: &ComparableVal) -> bool {
                self.val.typ() == b.val.typ() && self.val == b.val
            }
        }

        #[cfg(not(feature = "btree_map"))]
        impl Hash for ComparableVal {
            fn hash<H: Hasher>(&self, state: &mut H) {
                self.val.typ().hash(state);
                self.val.hash(state);
            }
        }

        #[cfg(feature = "btree_map")]
        impl PartialOrd for ComparableVal {
            #[inline]
            fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
                Some(self.cmp(other))
            }
        }

        #[cfg(feature = "btree_map")]
        impl Ord for ComparableVal {
            fn cmp(&self, b: &Self) -> std::cmp::Ordering {
                if self.val.typ() == b.val.typ() {
                    self.val.cmp(&b.val)
                } else {
                    self.val.typ().cmp(&b.val.typ())
                }
            }
        }

        let mut nils = vec![];
        let mut nil_map = Map::new();
        let mut comparables = vec![];
        let mut comparables_map = Map::new();
        let mut others = vec![];
        let mut others_map = Map::new();
        let consts_indices: Vec<(ConstType, usize, usize)> = self
            .consts
            .borrow()
            .iter()
            .enumerate()
            .map(|(i, c)| match c {
                Const::Nil(val) => (
                    ConstType::Nil,
                    i,
                    *nil_map.entry((val.typ(), val.t_elem())).or_insert_with(|| {
                        nils.push(val.clone());
                        nils.len() - 1
                    }),
                ),
                Const::Comparable(val) => (
                    ConstType::Comparable,
                    i,
                    *comparables_map
                        .entry(ComparableVal { val: val.clone() })
                        .or_insert_with(|| {
                            comparables.push(val.clone());
                            comparables.len() - 1
                        }),
                ),
                Const::ZeroVal(val, meta) => (
                    ConstType::Other,
                    i,
                    *others_map.entry(meta).or_insert_with(|| {
                        others.push(val.clone());
                        others.len() - 1
                    }),
                ),
                Const::Method(_, _) => unreachable!(),
            })
            .collect();

        let mut map = Map::new();
        for (t, i, j) in consts_indices {
            let offset = match t {
                ConstType::Nil => 0,
                ConstType::Comparable => nils.len(),
                ConstType::Other => nils.len() + comparables.len(),
            };
            map.insert(i, j + offset);
        }
        let mut consts = vec![];
        consts.append(&mut nils);
        consts.append(&mut comparables);
        consts.append(&mut others);
        (consts, map)
    }
}
