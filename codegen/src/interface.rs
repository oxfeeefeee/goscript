use super::types::TypeLookup;
use goscript_types::TypeKey as TCTypeKey;
use goscript_vm::gc::GcoVec;
use goscript_vm::instruction::*;
use goscript_vm::metadata::*;
use goscript_vm::objects::{IfaceBinding, VMObjects};
use std::collections::HashMap;

pub struct IfaceMapping {
    ifaces: Vec<(GosMetadata, Option<Vec<IfaceBinding>>)>,
    iface_indices: HashMap<(TCTypeKey, Option<TCTypeKey>), OpIndex>,
}

impl IfaceMapping {
    pub fn new() -> IfaceMapping {
        IfaceMapping {
            ifaces: vec![],
            iface_indices: HashMap::new(),
        }
    }

    pub fn result(self) -> Vec<(GosMetadata, Option<Vec<IfaceBinding>>)> {
        self.ifaces
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
        let mapping = IfaceMapping::get_binding_info(i_s, lookup, objs, dummy_gcv);
        let index = self.ifaces.len() as OpIndex;
        self.ifaces.push(mapping);
        self.iface_indices.insert(*i_s, index);
        index
    }

    fn get_binding_info(
        i_s: &(TCTypeKey, Option<TCTypeKey>),
        lookup: &mut TypeLookup,
        objs: &mut VMObjects,
        dummy_gcv: &mut GcoVec,
    ) -> (GosMetadata, Option<Vec<IfaceBinding>>) {
        let i = lookup.meta_from_tc(i_s.0, objs, dummy_gcv);
        if i_s.1.is_none() {
            return (i, None);
        }
        let struct_ = lookup.meta_from_tc(i_s.1.unwrap(), objs, dummy_gcv);
        let fields: Vec<&String> = match &objs.metas[i.underlying(&objs.metas).as_non_ptr()] {
            MetadataType::Interface(m) => m.fields.iter().map(|x| &x.1).collect(),
            _ => unreachable!(),
        };
        (
            i,
            Some(
                fields
                    .iter()
                    .map(|x| struct_.get_iface_binding(x, &objs.metas).unwrap())
                    .collect(),
            ),
        )
    }
}
