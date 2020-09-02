#![allow(dead_code)]
use goscript_types::{BasicType, ConstValue, TCObjects, Type, TypeKey as TCTypeKey};
use goscript_vm::instruction::{OpIndex, Value32Type};
use goscript_vm::value::*;
use std::collections::HashMap;

// returns const value if val is_some, otherwise returns vm_type for the tc_type
fn const_value_or_type_from_tc(
    tkey: TCTypeKey,
    val: Option<&ConstValue>,
    tc_objs: &TCObjects,
    vm_objs: &mut VMObjects,
) -> GosValue {
    let typ = tc_objs.types[tkey].try_as_basic().unwrap().typ();
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
                GosValue::Float64(*f)
            })
        }
        BasicType::Str | BasicType::UntypedString => val.map_or(vm_objs.metadata_string(), |x| {
            GosValue::new_str(x.str_as_string())
        }),
        _ => unreachable!(),
        //Complex64,  todo
        //Complex128, todo
    }
}

// get GosValue from type checker's Obj
pub fn get_const_value(
    tkey: TCTypeKey,
    val: &ConstValue,
    tc_objs: &TCObjects,
    vm_objs: &mut VMObjects,
) -> GosValue {
    const_value_or_type_from_tc(tkey, Some(val), tc_objs, vm_objs)
}

// get vm_type from tc_type
// todo: cache result
pub fn type_from_tc(typ: TCTypeKey, tc_objs: &TCObjects, vm_objs: &mut VMObjects) -> GosValue {
    match &tc_objs.types[typ] {
        Type::Basic(_) => const_value_or_type_from_tc(typ, None, tc_objs, vm_objs),
        Type::Slice(detail) => {
            let el_type = type_from_tc(detail.elem(), tc_objs, vm_objs);
            MetadataVal::new_slice(el_type, vm_objs)
        }
        Type::Struct(detail) => {
            let mut fields = Vec::new();
            let mut map = HashMap::<String, OpIndex>::new();
            for (i, f) in detail.fields().iter().enumerate() {
                let field = &tc_objs.lobjs[*f];
                let f_type = type_from_tc(field.typ().unwrap(), tc_objs, vm_objs);
                fields.push(f_type);
                map.insert(field.name().clone(), i as OpIndex);
            }
            MetadataVal::new_struct(fields, map, vm_objs)
        }
        Type::Signature(detail) => {
            let mut convert = |tuple_key| {
                tc_objs.types[tuple_key]
                    .try_as_tuple()
                    .unwrap()
                    .vars()
                    .iter()
                    .map(|&x| type_from_tc(tc_objs.lobjs[x].typ().unwrap(), tc_objs, vm_objs))
                    .collect()
            };
            let params = convert(detail.params());
            let results = convert(detail.results());
            let recv = detail.recv().map(|x| {
                let recv_tc_type = tc_objs.lobjs[x].typ().unwrap();
                type_from_tc(recv_tc_type, tc_objs, vm_objs)
            });
            //dbg!(&params, &results, &recv, detail);
            MetadataVal::new_sig(recv, params, results, detail.variadic(), vm_objs)
        }
        Type::Pointer(detail) => {
            let inner = type_from_tc(detail.base(), tc_objs, vm_objs);
            MetadataVal::new_boxed(inner, vm_objs)
        }
        Type::Named(detail) => {
            // this is incorrect
            type_from_tc(detail.underlying(), tc_objs, vm_objs)
        }
        _ => {
            dbg!(&tc_objs.types[typ]);
            unimplemented!()
        }
    }
}

pub fn value32_type_from_tc(typ: TCTypeKey, tc_objs: &TCObjects) -> Value32Type {
    match &tc_objs.types[typ] {
        Type::Basic(detail) => match detail.typ() {
            BasicType::Bool | BasicType::UntypedBool => Value32Type::Bool,
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
            | BasicType::UntypedRune => Value32Type::Int,
            BasicType::Float32 | BasicType::Float64 | BasicType::UntypedFloat => {
                Value32Type::Float64
            }
            BasicType::Str | BasicType::UntypedString => Value32Type::Str,
            _ => unreachable!(),
            //Complex64,  todo
            //Complex128, todo
        },
        Type::Slice(_) => Value32Type::Slice,
        Type::Map(_) => Value32Type::Map,
        Type::Struct(_) => Value32Type::Struct,
        Type::Signature(_) => Value32Type::Function,
        Type::Pointer(_) => Value32Type::Boxed,
        Type::Named(detail) => value32_type_from_tc(detail.underlying(), tc_objs),
        _ => {
            dbg!(&tc_objs.types[typ]);
            unimplemented!()
        }
    }
}

pub fn range_value32_types(typ: TCTypeKey, tc_objs: &TCObjects) -> Vec<Value32Type> {
    match &tc_objs.types[typ] {
        Type::Basic(detail) => match detail.typ() {
            BasicType::Str | BasicType::UntypedString => vec![Value32Type::Int, Value32Type::Int],
            _ => unreachable!(),
        },
        Type::Slice(detail) => {
            let elem = value32_type_from_tc(detail.elem(), tc_objs);
            vec![Value32Type::Int, elem]
        }
        Type::Map(detail) => {
            let key = value32_type_from_tc(detail.key(), tc_objs);
            let elem = value32_type_from_tc(detail.elem(), tc_objs);
            vec![key, elem]
        }
        _ => {
            dbg!(&tc_objs.types[typ]);
            unreachable!()
        }
    }
}

pub fn return_value32_types(typ: TCTypeKey, tc_objs: &TCObjects) -> Vec<Value32Type> {
    match &tc_objs.types[typ] {
        Type::Tuple(detail) => detail
            .vars()
            .iter()
            .map(|x| {
                let typ = tc_objs.lobjs[*x].typ().unwrap();
                value32_type_from_tc(typ, tc_objs)
            })
            .collect(),
        _ => unreachable!(),
    }
}
