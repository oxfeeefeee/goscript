// Copyright 2022 The Goscript Authors. All rights reserved.
// Use of this source code is governed by a BSD-style
// license that can be found in the LICENSE file.

use super::types::TypeLookup;
use goscript_types::TypeKey as TCTypeKey;
use goscript_vm::gc::GcoVec;
use goscript_vm::instruction::*;
use goscript_vm::metadata::*;
use goscript_vm::objects::{IfaceBinding, VMObjects};
use std::collections::HashMap;
use std::hash::Hash;

pub type IfaceSelector = Selector<(TCTypeKey, TCTypeKey), (Meta, Vec<IfaceBinding>)>;

pub type StructSelector = Selector<Vec<OpIndex>, Vec<OpIndex>>;

pub struct Selector<K: Eq + Hash, V> {
    vec: Vec<V>,
    mapping: HashMap<K, OpIndex>,
}

impl<K: Eq + Hash, V> Selector<K, V> {
    pub fn new() -> Selector<K, V> {
        Selector {
            vec: vec![],
            mapping: HashMap::new(),
        }
    }

    pub fn result(self) -> Vec<V> {
        self.vec
    }

    pub fn add<K2V>(&mut self, key: K, k2v: K2V) -> OpIndex
    where
        K2V: FnMut(K) -> V,
    {
        *self.mapping.entry(key).or_insert_with(|| {
            let info = k2v(key);
            self.vec.push(info);
            self.vec.len() as OpIndex - 1
        })
    }
}

impl Selector<Vec<OpIndex>, Vec<OpIndex>> {
    pub fn get_index(&mut self, indices: Vec<OpIndex>) -> OpIndex {
        let k2v = |k| k;
        self.add(indices, k2v)
    }
}

impl Selector<(TCTypeKey, TCTypeKey), (Meta, Vec<IfaceBinding>)> {
    pub fn get_index(
        &mut self,
        i_s: (TCTypeKey, TCTypeKey),
        lookup: &mut TypeLookup,
        objs: &mut VMObjects,
        dummy_gcv: &mut GcoVec,
    ) -> OpIndex {
        let k2v = |k| Self::get_binding_info(k, lookup, objs, dummy_gcv);
        self.add(i_s, k2v)
    }

    fn get_binding_info(
        i_s: (TCTypeKey, TCTypeKey),
        lookup: &mut TypeLookup,
        objs: &mut VMObjects,
        dummy_gcv: &mut GcoVec,
    ) -> (Meta, Vec<IfaceBinding>) {
        let iface = lookup.tc_type_to_meta(i_s.0, objs, dummy_gcv);
        let struct_ = lookup.tc_type_to_meta(i_s.1, objs, dummy_gcv);
        let fields: Vec<&String> = match &objs.metas[iface.underlying(&objs.metas).key] {
            MetadataType::Interface(m) => m.all().iter().map(|x| &x.name).collect(),
            _ => unreachable!(),
        };
        (
            struct_,
            fields
                .iter()
                .map(|x| struct_.get_iface_binding(x, &objs.metas).unwrap())
                .collect(),
        )
    }
}

// pub struct IfaceSelector {
//     ifaces: Vec<(Meta, Vec<IfaceBinding>)>,
//     mapping: HashMap<(TCTypeKey, TCTypeKey), OpIndex>,
// }

// impl IfaceSelector {
//     pub fn new() -> IfaceSelector {
//         IfaceSelector {
//             ifaces: vec![],
//             mapping: HashMap::new(),
//         }
//     }

//     pub fn result(self) -> Vec<(Meta, Vec<IfaceBinding>)> {
//         self.ifaces
//     }

//     pub fn get_index(
//         &mut self,
//         i_s: (TCTypeKey, TCTypeKey),
//         lookup: &mut TypeLookup,
//         objs: &mut VMObjects,
//         dummy_gcv: &mut GcoVec,
//     ) -> OpIndex {
//         *self.mapping.entry(i_s).or_insert_with(|| {
//             let info = IfaceSelector::get_binding_info(&i_s, lookup, objs, dummy_gcv);
//             self.ifaces.push(info);
//             self.ifaces.len() as OpIndex - 1
//         })
//     }

//     fn get_binding_info(
//         i_s: &(TCTypeKey, TCTypeKey),
//         lookup: &mut TypeLookup,
//         objs: &mut VMObjects,
//         dummy_gcv: &mut GcoVec,
//     ) -> (Meta, Vec<IfaceBinding>) {
//         let iface = lookup.tc_type_to_meta(i_s.0, objs, dummy_gcv);
//         let struct_ = lookup.tc_type_to_meta(i_s.1, objs, dummy_gcv);
//         let fields: Vec<&String> = match &objs.metas[iface.underlying(&objs.metas).key] {
//             MetadataType::Interface(m) => m.all().iter().map(|x| &x.name).collect(),
//             _ => unreachable!(),
//         };
//         (
//             struct_,
//             fields
//                 .iter()
//                 .map(|x| struct_.get_iface_binding(x, &objs.metas).unwrap())
//                 .collect(),
//         )
//     }
// }
