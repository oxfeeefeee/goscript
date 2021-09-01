#![allow(dead_code)]

use super::types::TypeLookup;
use goscript_types::TypeKey as TCTypeKey;
use goscript_vm::gc::GcoVec;
use goscript_vm::instruction::*;
use goscript_vm::metadata::*;
use goscript_vm::objects::{FunctionKey, VMObjects};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

pub struct IfaceMapping {
    ifaces: Vec<(GosMetadata, Option<Vec<Rc<RefCell<MethodDesc>>>>)>,
    iface_indices: HashMap<(TCTypeKey, Option<TCTypeKey>), OpIndex>,
}

impl IfaceMapping {
    pub fn new() -> IfaceMapping {
        IfaceMapping {
            ifaces: vec![],
            iface_indices: HashMap::new(),
        }
    }

    pub fn into_result(self) -> Vec<(GosMetadata, Option<Rc<Vec<FunctionKey>>>)> {
        self.ifaces
            .into_iter()
            .map(|(meta, method)| {
                (
                    meta,
                    method.map(|m| Rc::new(m.iter().map(|x| x.borrow().func.unwrap()).collect())),
                )
            })
            .collect()
    }

    pub fn get_index(
        &mut self,
        i_s: &(TCTypeKey, Option<TCTypeKey>),
        lookup: &mut TypeLookup,
        objs: &mut VMObjects,
        dummy_gcv: &mut GcoVec,
    ) -> OpIndex {
        if let Some(i) = self.iface_indices.get(i_s) {
            return *i;
        }
        let mapping = IfaceMapping::get_iface_info(i_s, lookup, objs, dummy_gcv);
        let index = self.ifaces.len() as OpIndex;
        self.ifaces.push(mapping);
        self.iface_indices.insert(*i_s, index);
        index
    }

    fn get_iface_info(
        i_s: &(TCTypeKey, Option<TCTypeKey>),
        lookup: &mut TypeLookup,
        objs: &mut VMObjects,
        dummy_gcv: &mut GcoVec,
    ) -> (GosMetadata, Option<Vec<Rc<RefCell<MethodDesc>>>>) {
        let i = lookup.meta_from_tc(i_s.0, objs, dummy_gcv);
        if i_s.1.is_none() {
            return (i, None);
        }
        let s = lookup.meta_from_tc(i_s.1.unwrap(), objs, dummy_gcv);
        let ifields = match &objs.metas[i.as_non_ptr()] {
            MetadataType::Named(_, iface) => match &objs.metas[iface.as_non_ptr()] {
                MetadataType::Interface(m) => m,
                _ => unreachable!(),
            },
            MetadataType::Interface(m) => m,
            _ => unreachable!(),
        };
        let named = match s {
            GosMetadata::NonPtr(k, _) => k,
            GosMetadata::Ptr1(k, _) => k,
            _ => unreachable!(),
        };
        let methods = match &objs.metas[named] {
            MetadataType::Named(m, _) => Some(m),
            // primitive types
            _ => None,
        };
        (i, methods.map(|x| ifields.iface_named_mapping(x)))
    }
}
