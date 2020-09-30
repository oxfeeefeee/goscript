#![allow(dead_code)]

use super::types::TypeLookup;
use goscript_types::TypeKey as TCTypeKey;
use goscript_vm::instruction::*;
use goscript_vm::objects::{MetadataType, VMObjects};
use std::collections::HashMap;
use std::rc::Rc;

pub struct IfaceMapping {
    mappings: Vec<Rc<Vec<OpIndex>>>,
    indices: HashMap<(TCTypeKey, TCTypeKey), OpIndex>,
}

impl IfaceMapping {
    pub fn new() -> IfaceMapping {
        IfaceMapping {
            mappings: vec![],
            indices: HashMap::new(),
        }
    }

    pub fn get_index(
        &mut self,
        i_s: &(TCTypeKey, TCTypeKey),
        lookup: &mut TypeLookup,
        objs: &mut VMObjects,
    ) -> OpIndex {
        if let Some(i) = self.indices.get(i_s) {
            return *i;
        }
        let mapping = IfaceMapping::get_index_impl(i_s, lookup, objs);
        let index = self.mappings.len() as OpIndex;
        self.mappings.push(mapping);
        self.indices.insert(*i_s, index);
        index
    }

    fn get_index_impl(
        i_s: &(TCTypeKey, TCTypeKey),
        lookup: &mut TypeLookup,
        objs: &mut VMObjects,
    ) -> Rc<Vec<OpIndex>> {
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
                MetadataType::Named(m, _) => m,
                _ => unreachable!(),
            },
            MetadataType::Named(m, _) => m,
            _ => unreachable!(),
        };
        Rc::new(imember.iface_mapping(smember))
    }
}
