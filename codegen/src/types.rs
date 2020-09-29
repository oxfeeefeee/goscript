#![allow(dead_code)]
use goscript_parser::ast::Node;
use goscript_parser::ast::{Expr, NodeId};
use goscript_parser::objects::IdentKey;
use goscript_types::{
    BasicType, ConstValue, ObjKey, TCObjects, Type, TypeInfo, TypeKey as TCTypeKey,
};
use goscript_vm::instruction::{OpIndex, ValueType};
use goscript_vm::objects::OrderedMembers;
use goscript_vm::value::*;
use std::collections::HashMap;

pub struct TypeLookup<'a> {
    tc_objs: &'a TCObjects,
    ti: &'a TypeInfo,
    types: HashMap<TCTypeKey, GosValue>,
}

impl<'a> TypeLookup<'a> {
    pub fn new(tc_objs: &'a TCObjects, ti: &'a TypeInfo) -> TypeLookup<'a> {
        TypeLookup {
            tc_objs: tc_objs,
            ti: ti,
            types: HashMap::new(),
        }
    }

    pub fn get_tc_const_value(&mut self, id: NodeId) -> Option<&ConstValue> {
        let typ_val = self.ti.types.get(&id).unwrap();
        typ_val.get_const_val()
    }

    pub fn get_const_value(&mut self, id: NodeId, objects: &mut VMObjects) -> Result<GosValue, ()> {
        let typ_val = self.ti.types.get(&id).unwrap();
        let const_val = typ_val.get_const_val().unwrap();
        Ok(self.const_value(typ_val.typ, const_val, objects))
    }

    pub fn gen_type_meta_by_node_id(&mut self, id: NodeId, objects: &mut VMObjects) -> GosValue {
        let typ = self.ti.types.get(&id).unwrap().typ;
        self.type_from_tc(typ, objects)
    }

    pub fn gen_def_type_meta(&mut self, ikey: IdentKey, objects: &mut VMObjects) -> GosValue {
        let obj = &self.tc_objs.lobjs[self.ti.defs[&ikey].unwrap()];
        let typ = obj.typ().unwrap();
        self.type_from_tc(typ, objects)
    }

    pub fn get_expr_value_type(&mut self, e: &Expr) -> ValueType {
        let typ = self.ti.types.get(&e.id()).unwrap().typ;
        self.value_type_from_tc(typ)
    }

    pub fn get_use_value_type(&self, ikey: IdentKey) -> ValueType {
        let typ = &self.tc_objs.lobjs[self.ti.uses[&ikey]].typ().unwrap();
        self.value_type_from_tc(*typ)
    }

    pub fn get_use_tc_type(&self, ikey: IdentKey) -> TCTypeKey {
        let obj = &self.tc_objs.lobjs[self.ti.uses[&ikey]];
        obj.typ().unwrap()
    }

    pub fn get_def_value_type(&mut self, ikey: IdentKey) -> ValueType {
        let typ = &self.tc_objs.lobjs[self.ti.defs[&ikey].unwrap()]
            .typ()
            .unwrap();
        self.value_type_from_tc(*typ)
    }

    pub fn get_range_value_types(&mut self, e: &Expr) -> [ValueType; 3] {
        let typ = self.ti.types.get(&e.id()).unwrap().typ;
        self.range_value_types(typ)
    }

    pub fn get_return_value_types(&mut self, e: &Expr) -> Vec<ValueType> {
        let typ = self.ti.types.get(&e.id()).unwrap().typ;
        self.return_value_types(typ)
    }

    pub fn get_selection_value_types(&mut self, id: NodeId) -> (ValueType, ValueType) {
        let sel = &self.ti.selections[&id];
        let t0 = self.value_type_from_tc(sel.recv().unwrap());
        let t1 = self.value_type_from_tc(self.tc_objs.lobjs[sel.obj()].typ().unwrap());
        (t0, t1)
    }

    pub fn type_from_tc(&mut self, typ: TCTypeKey, vm_objs: &mut VMObjects) -> GosValue {
        if !self.types.contains_key(&typ) {
            let val = self.type_from_tc_impl(typ, vm_objs);
            self.types.insert(typ, val);
        }
        self.types.get(&typ).unwrap().clone()
    }

    // returns const value if val is_some, otherwise returns vm_type for the tc_type
    fn const_value_or_type_from_tc(
        &self,
        tkey: TCTypeKey,
        val: Option<&ConstValue>,
        vm_objs: &mut VMObjects,
    ) -> GosValue {
        let typ = self.tc_objs.types[tkey].try_as_basic().unwrap().typ();
        match typ {
            //todo: fix: dont new MetadataVal
            BasicType::Bool | BasicType::UntypedBool => val.map_or(vm_objs.metadata_bool(), |x| {
                GosValue::Bool(x.bool_as_bool())
            }),
            BasicType::Int
            | BasicType::Int8
            | BasicType::Int16
            | BasicType::Int32
            | BasicType::Rune
            | BasicType::Int64
            | BasicType::Uint
            | BasicType::Uint8
            | BasicType::Byte
            | BasicType::Uint16
            | BasicType::Uint32
            | BasicType::Uint64
            | BasicType::Uintptr
            | BasicType::UnsafePointer
            | BasicType::UntypedInt
            | BasicType::UntypedRune => val.map_or(vm_objs.metadata_int(), |x| {
                let (i, _) = x.to_int().int_as_i64();
                GosValue::Int(i as isize)
            }),
            BasicType::Float32 | BasicType::Float64 | BasicType::UntypedFloat => {
                val.map_or(vm_objs.metadata_float64(), |x| {
                    let (f, _) = x.num_as_f64();
                    GosValue::Float64(f.into())
                })
            }
            BasicType::Str | BasicType::UntypedString => val
                .map_or(vm_objs.metadata_string(), |x| {
                    GosValue::new_str(x.str_as_string())
                }),
            _ => unreachable!(),
            //Complex64,  todo
            //Complex128, todo
        }
    }

    // get GosValue from type checker's Obj
    fn const_value(&self, tkey: TCTypeKey, val: &ConstValue, vm_objs: &mut VMObjects) -> GosValue {
        self.const_value_or_type_from_tc(tkey, Some(val), vm_objs)
    }

    // get vm_type from tc_type
    fn type_from_tc_impl(&mut self, typ: TCTypeKey, vm_objs: &mut VMObjects) -> GosValue {
        match &self.tc_objs.types[typ] {
            Type::Basic(_) => self.const_value_or_type_from_tc(typ, None, vm_objs),
            Type::Slice(detail) => {
                let el_type = self.type_from_tc(detail.elem(), vm_objs);
                MetadataVal::new_slice(el_type, vm_objs)
            }
            Type::Map(detail) => {
                let ktype = self.type_from_tc(detail.key(), vm_objs);
                let vtype = self.type_from_tc(detail.elem(), vm_objs);
                MetadataVal::new_map(ktype, vtype, vm_objs)
            }
            Type::Struct(detail) => {
                let mut vec = Vec::new();
                let mut map = HashMap::<String, OpIndex>::new();
                for (i, f) in detail.fields().iter().enumerate() {
                    let field = &self.tc_objs.lobjs[*f];
                    dbg!(&field);
                    let f_type = self.type_from_tc(field.typ().unwrap(), vm_objs);
                    vec.push(f_type);
                    map.insert(field.name().clone(), i as OpIndex);
                }
                MetadataVal::new_struct(OrderedMembers::new(vec, map), vm_objs)
            }
            Type::Interface(detail) => {
                let methods = detail.all_methods();
                let fields = self.get_ordered_members(methods.as_ref().unwrap(), vm_objs);
                MetadataVal::new_interface(fields, vm_objs)
            }
            Type::Signature(detail) => {
                let mut convert = |tuple_key| {
                    self.tc_objs.types[tuple_key]
                        .try_as_tuple()
                        .unwrap()
                        .vars()
                        .iter()
                        .map(|&x| self.type_from_tc(self.tc_objs.lobjs[x].typ().unwrap(), vm_objs))
                        .collect()
                };
                let params = convert(detail.params());
                let results = convert(detail.results());
                let mut recv = None;
                if let Some(r) = detail.recv() {
                    let recv_tc_type = self.tc_objs.lobjs[*r].typ().unwrap();
                    // to avoid infinite recursion
                    if !self.tc_objs.types[recv_tc_type].is_interface(self.tc_objs) {
                        recv = Some(self.type_from_tc(recv_tc_type, vm_objs));
                    }
                }
                let variadic = if detail.variadic() {
                    let slice = self.tc_objs.types[detail.params()]
                        .try_as_tuple()
                        .unwrap()
                        .vars()
                        .last()
                        .unwrap();
                    let sval = self.tc_objs.lobjs[*slice].typ().unwrap();
                    let elem = self.tc_objs.types[sval].try_as_slice().unwrap().elem();
                    let typ = self.type_from_tc(elem, vm_objs);
                    Some(vm_objs.metas[*typ.as_meta()].zero_val().get_type())
                } else {
                    None
                };
                MetadataVal::new_sig(recv, params, results, variadic, vm_objs)
            }
            Type::Pointer(detail) => {
                let inner = self.type_from_tc(detail.base(), vm_objs);
                MetadataVal::new_boxed(inner, vm_objs)
            }
            Type::Named(detail) => {
                let underlying = self.type_from_tc(detail.underlying(), vm_objs);
                MetadataVal::new_named(underlying, vm_objs)
            }
            _ => {
                dbg!(&self.tc_objs.types[typ]);
                unimplemented!()
            }
        }
    }

    fn value_type_from_tc(&self, typ: TCTypeKey) -> ValueType {
        match &self.tc_objs.types[typ] {
            Type::Basic(detail) => match detail.typ() {
                BasicType::Bool | BasicType::UntypedBool => ValueType::Bool,
                BasicType::Int
                | BasicType::Int8
                | BasicType::Int16
                | BasicType::Int32
                | BasicType::Rune
                | BasicType::Int64
                | BasicType::Uint
                | BasicType::Uint8
                | BasicType::Byte
                | BasicType::Uint16
                | BasicType::Uint32
                | BasicType::Uint64
                | BasicType::Uintptr
                | BasicType::UnsafePointer
                | BasicType::UntypedInt
                | BasicType::UntypedRune => ValueType::Int,
                BasicType::Float32 | BasicType::Float64 | BasicType::UntypedFloat => {
                    ValueType::Float64
                }
                BasicType::Str | BasicType::UntypedString => ValueType::Str,
                BasicType::UntypedNil => ValueType::Nil,
                _ => {
                    dbg!(detail.typ());
                    unreachable!()
                } //Complex64,  todo
                  //Complex128, todo
            },
            Type::Slice(_) => ValueType::Slice,
            Type::Map(_) => ValueType::Map,
            Type::Struct(_) => ValueType::Struct,
            Type::Interface(_) => ValueType::Interface,
            Type::Signature(_) => ValueType::Closure,
            Type::Pointer(_) => ValueType::Boxed,
            Type::Named(detail) => self.value_type_from_tc(detail.underlying()),
            _ => {
                dbg!(&self.tc_objs.types[typ]);
                unimplemented!()
            }
        }
    }

    fn range_value_types(&self, typ: TCTypeKey) -> [ValueType; 3] {
        match &self.tc_objs.types[typ] {
            Type::Basic(detail) => match detail.typ() {
                BasicType::Str | BasicType::UntypedString => {
                    [ValueType::Str, ValueType::Int, ValueType::Int]
                }
                _ => unreachable!(),
            },
            Type::Slice(detail) => {
                let elem = self.value_type_from_tc(detail.elem());
                [ValueType::Slice, ValueType::Int, elem]
            }
            Type::Map(detail) => {
                let key = self.value_type_from_tc(detail.key());
                let elem = self.value_type_from_tc(detail.elem());
                [ValueType::Map, key, elem]
            }
            _ => {
                dbg!(&self.tc_objs.types[typ]);
                unreachable!()
            }
        }
    }

    fn return_value_types(&self, typ: TCTypeKey) -> Vec<ValueType> {
        match &self.tc_objs.types[typ] {
            Type::Tuple(detail) => detail
                .vars()
                .iter()
                .map(|x| {
                    let typ = self.tc_objs.lobjs[*x].typ().unwrap();
                    self.value_type_from_tc(typ)
                })
                .collect(),
            _ => unreachable!(),
        }
    }

    fn get_ordered_members(
        &mut self,
        fields: &Vec<ObjKey>,
        vm_objs: &mut VMObjects,
    ) -> OrderedMembers {
        let mut vec = Vec::new();
        let mut map = HashMap::<String, OpIndex>::new();
        for (i, f) in fields.iter().enumerate() {
            let field = &self.tc_objs.lobjs[*f];
            let f_type = self.type_from_tc(field.typ().unwrap(), vm_objs);
            vec.push(f_type);
            map.insert(field.name().clone(), i as OpIndex);
        }
        OrderedMembers::new(vec, map)
    }
}
