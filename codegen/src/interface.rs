#![allow(dead_code)]

use super::types::TypeLookup;
use goscript_types::TypeKey as TCTypeKey;
use goscript_vm::instruction::*;
use goscript_vm::metadata::*;
use goscript_vm::objects::{FunctionKey, VMObjects};
use std::collections::HashMap;
use std::rc::Rc;

pub struct IfaceMapping {
    pub ifaces: Vec<(GosMetadata, Rc<Vec<FunctionKey>>)>,
    iface_indices: HashMap<(TCTypeKey, TCTypeKey), OpIndex>,
}

impl IfaceMapping {
    pub fn new() -> IfaceMapping {
        IfaceMapping {
            ifaces: vec![],
            iface_indices: HashMap::new(),
        }
    }

    pub fn get_index(
        &mut self,
        i_s: &(TCTypeKey, TCTypeKey),
        lookup: &mut TypeLookup,
        objs: &mut VMObjects,
    ) -> OpIndex {
        if let Some(i) = self.iface_indices.get(i_s) {
            return *i;
        }
        let mapping = IfaceMapping::get_iface_info(i_s, lookup, objs);
        let index = self.ifaces.len() as OpIndex;
        self.ifaces.push(mapping);
        self.iface_indices.insert(*i_s, index);
        index
    }

    fn get_iface_info(
        i_s: &(TCTypeKey, TCTypeKey),
        lookup: &mut TypeLookup,
        objs: &mut VMObjects,
    ) -> (GosMetadata, Rc<Vec<FunctionKey>>) {
        let i = lookup.meta_from_tc(i_s.0, objs);
        let s = lookup.meta_from_tc(i_s.1, objs);
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
        (
            i,
            Rc::new(methods.map_or(vec![], |x| ifields.iface_named_mapping(x))),
        )
    }
}
