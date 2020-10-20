#![allow(dead_code)]

use super::types::TypeLookup;
use goscript_types::TypeKey as TCTypeKey;
use goscript_vm::instruction::*;
use goscript_vm::objects::{FunctionKey, MetadataType, VMObjects};
use goscript_vm::value::GosValue;
use std::collections::HashMap;
use std::rc::Rc;

pub struct IfaceMapping {
    pub ifaces: Vec<(GosValue, Rc<Vec<FunctionKey>>)>,
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
    ) -> (GosValue, Rc<Vec<FunctionKey>>) {
        let i = lookup.type_from_tc(i_s.0, objs);
        let s = lookup.type_from_tc(i_s.1, objs);
        let imember = match objs.metas[*i.as_meta()].typ() {
            MetadataType::Named(_, iface) => match objs.metas[*iface.as_meta()].typ() {
                MetadataType::Interface(m) => m,
                _ => unreachable!(),
            },
            MetadataType::Interface(m) => m,
            _ => unreachable!(),
        };
        let smember = match objs.metas[*s.as_meta()].typ() {
            MetadataType::Boxed(b) => match objs.metas[*b.as_meta()].typ() {
                MetadataType::Named(m, _) => Some(m),
                _ => unreachable!(),
            },
            MetadataType::Named(m, _) => Some(m),
            // primitive types
            MetadataType::None => None,
            _ => unreachable!(),
        };
        (
            i,
            Rc::new(smember.map_or(vec![], |x| imember.iface_mapping(x))),
        )
    }
}
