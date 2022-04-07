#![allow(dead_code)]
use goscript_parser::ast::Node;
use goscript_parser::ast::{Expr, NodeId};
use goscript_parser::objects::IdentKey;
use goscript_types::{
    BasicType, ChanDir, ConstValue, EntityType, ObjKey as TCObjKey, OperandMode,
    PackageKey as TCPackageKey, TCObjects, Type, TypeInfo, TypeKey as TCTypeKey,
};
use goscript_vm::gc::GcoVec;
use goscript_vm::instruction::ValueType;
use goscript_vm::metadata::*;
use goscript_vm::value::*;
use std::collections::HashMap;

pub type TypeCache = HashMap<TCTypeKey, Meta>;

#[derive(PartialEq)]
pub enum SelectionType {
    NonMethod,
    MethodNonPtrRecv,
    MethodPtrRecv,
}

pub struct TypeLookup<'a> {
    tc_objs: &'a TCObjects,
    ti: &'a TypeInfo,
    types_cache: &'a mut TypeCache,
    unsafe_ptr_meta: Meta,
}

impl<'a> TypeLookup<'a> {
    pub fn new(
        tc_objs: &'a TCObjects,
        ti: &'a TypeInfo,
        cache: &'a mut TypeCache,
        u_p_meta: Meta,
    ) -> TypeLookup<'a> {
        TypeLookup {
            tc_objs: tc_objs,
            ti: ti,
            types_cache: cache,
            unsafe_ptr_meta: u_p_meta,
        }
    }

    #[inline]
    pub fn type_info(&self) -> &TypeInfo {
        self.ti
    }

    #[inline]
    pub fn get_tc_const_value(&mut self, id: NodeId) -> Option<&ConstValue> {
        let typ_val = self.ti.types.get(&id).unwrap();
        typ_val.get_const_val()
    }

    #[inline]
    pub fn get_const_value(&mut self, id: NodeId) -> GosValue {
        let typ_val = self.ti.types.get(&id).unwrap();
        let const_val = typ_val.get_const_val().unwrap();
        self.const_value(typ_val.typ, const_val)
    }

    #[inline]
    pub fn get_const_value_by_ident(&mut self, id: &IdentKey) -> GosValue {
        let lobj_key = self.ti.defs[id].unwrap();
        let lobj = &self.tc_objs.lobjs[lobj_key];
        let tkey = lobj.typ().unwrap();
        self.const_value(tkey, lobj.const_val())
    }

    pub fn get_expr_value_types(&self, e: &Expr) -> Vec<ValueType> {
        let typ_val = self.ti.types.get(&e.id()).unwrap();
        let tcts = match typ_val.mode {
            OperandMode::Value => {
                let typ = self.tc_objs.types[typ_val.typ].underlying_val(self.tc_objs);
                match typ {
                    Type::Tuple(tp) => tp
                        .vars()
                        .iter()
                        .map(|o| self.tc_objs.lobjs[*o].typ().unwrap())
                        .collect(),
                    _ => vec![typ_val.typ],
                }
            }
            OperandMode::NoValue => vec![],
            OperandMode::Constant(_) => vec![typ_val.typ],
            _ => unreachable!(),
        };
        tcts.iter().map(|x| self.value_type_from_tc(*x)).collect()
    }

    #[inline]
    pub fn get_expr_tc_type(&self, e: &Expr) -> TCTypeKey {
        self.get_node_tc_type(e.id())
    }

    #[inline]
    pub fn get_node_tc_type(&self, id: NodeId) -> TCTypeKey {
        self.ti.types.get(&id).unwrap().typ
    }

    #[inline]
    pub fn try_get_expr_mode(&self, e: &Expr) -> Option<&OperandMode> {
        self.ti.types.get(&e.id()).map(|x| &x.mode)
    }

    #[inline]
    pub fn get_expr_mode(&self, e: &Expr) -> &OperandMode {
        &self.ti.types.get(&e.id()).unwrap().mode
    }

    // some of the built in funcs are not recorded
    pub fn try_get_expr_tc_type(&self, e: &Expr) -> Option<TCTypeKey> {
        self.ti
            .types
            .get(&e.id())
            .map(|x| {
                let typ = x.typ;
                if self.tc_objs.types[typ].is_invalid(self.tc_objs) {
                    None
                } else {
                    Some(typ)
                }
            })
            .flatten()
    }

    pub fn try_get_pkg_key(&self, e: &Expr) -> Option<TCPackageKey> {
        if let Expr::Ident(ikey) = e {
            self.ti
                .uses
                .get(ikey)
                .map(|x| {
                    if let EntityType::PkgName(pkg, _) = self.tc_objs.lobjs[*x].entity_type() {
                        Some(*pkg)
                    } else {
                        None
                    }
                })
                .flatten()
        } else {
            None
        }
    }

    pub fn get_expr_value_type(&mut self, e: &Expr) -> ValueType {
        let tv = self.ti.types.get(&e.id()).unwrap();
        if tv.mode == OperandMode::TypeExpr {
            ValueType::Metadata
        } else {
            let t = self.get_expr_tc_type(e);
            self.value_type_from_tc(t)
        }
    }

    pub fn get_meta_by_node_id(
        &mut self,
        id: NodeId,
        objects: &mut VMObjects,
        dummy_gcv: &mut GcoVec,
    ) -> Meta {
        let tv = self.ti.types.get(&id).unwrap();
        let md = self.meta_from_tc(tv.typ, objects, dummy_gcv);
        if tv.mode == OperandMode::TypeExpr {
            md.into_type_category()
        } else {
            md
        }
    }

    #[inline]
    pub fn get_use_object(&self, ikey: IdentKey) -> TCObjKey {
        self.ti.uses[&ikey]
    }

    #[inline]
    pub fn get_def_object(&self, ikey: IdentKey) -> TCObjKey {
        self.ti.defs[&ikey].unwrap()
    }

    #[inline]
    pub fn get_implicit_object(&self, id: &NodeId) -> TCObjKey {
        self.ti.implicits[id]
    }

    #[inline]
    pub fn get_use_tc_type(&self, ikey: IdentKey) -> TCTypeKey {
        let obj = &self.tc_objs.lobjs[self.get_use_object(ikey)];
        obj.typ().unwrap()
    }

    #[inline]
    pub fn get_use_value_type(&self, ikey: IdentKey) -> ValueType {
        self.value_type_from_tc(self.get_use_tc_type(ikey))
    }

    #[inline]
    pub fn get_def_tc_type(&self, ikey: IdentKey) -> TCTypeKey {
        let obj = &self.tc_objs.lobjs[self.get_def_object(ikey)];
        obj.typ().unwrap()
    }

    #[inline]
    pub fn get_def_value_type(&mut self, ikey: IdentKey) -> ValueType {
        self.value_type_from_tc(self.get_def_tc_type(ikey))
    }

    #[inline]
    pub fn ident_is_def(&self, ikey: &IdentKey) -> bool {
        self.ti.defs.contains_key(ikey)
    }

    #[inline]
    pub fn gen_def_type_meta(
        &mut self,
        ikey: IdentKey,
        objects: &mut VMObjects,
        dummy_gcv: &mut GcoVec,
    ) -> Meta {
        self.meta_from_tc(self.get_def_tc_type(ikey), objects, dummy_gcv)
    }

    #[inline]
    pub fn get_range_tc_types(&mut self, e: &Expr) -> [TCTypeKey; 3] {
        let typ = self.ti.types.get(&e.id()).unwrap().typ;
        self.range_tc_types(typ)
    }

    #[inline]
    pub fn get_tuple_tc_types(&mut self, e: &Expr) -> Vec<TCTypeKey> {
        let typ = self.ti.types.get(&e.id()).unwrap().typ;
        self.tuple_tc_types(typ)
    }

    pub fn get_selection_vtypes_indices_sel_typ(
        &mut self,
        id: NodeId,
    ) -> (ValueType, ValueType, &Vec<usize>, SelectionType) {
        let sel = &self.ti.selections[&id];
        let t0 = self.value_type_from_tc(sel.recv().unwrap());
        let obj = &self.tc_objs.lobjs[sel.obj()];
        let t1 = self.value_type_from_tc(obj.typ().unwrap());
        let sel_typ = match t1 {
            ValueType::Closure => match obj.entity_type() {
                EntityType::Func(ptr_recv) => match ptr_recv {
                    true => SelectionType::MethodPtrRecv,
                    false => SelectionType::MethodNonPtrRecv,
                },
                _ => SelectionType::NonMethod,
            },
            _ => SelectionType::NonMethod,
        };
        (t0, t1, &sel.indices(), sel_typ)
    }

    pub fn meta_from_tc(
        &mut self,
        typ: TCTypeKey,
        vm_objs: &mut VMObjects,
        dummy_gcv: &mut GcoVec,
    ) -> Meta {
        if !self.types_cache.contains_key(&typ) {
            let val = self.meta_from_tc_impl(typ, vm_objs, dummy_gcv);
            self.types_cache.insert(typ, val);
        }
        self.types_cache.get(&typ).unwrap().clone()
    }

    pub fn get_sig_params_tc_types(
        &mut self,
        func: TCTypeKey,
    ) -> (Vec<TCTypeKey>, Option<TCTypeKey>) {
        let typ = &self.tc_objs.types[func].underlying_val(self.tc_objs);
        let sig = typ.try_as_signature().unwrap();
        let params: Vec<TCTypeKey> = self.tc_objs.types[sig.params()]
            .try_as_tuple()
            .unwrap()
            .vars()
            .iter()
            .map(|&x| self.tc_objs.lobjs[x].typ().unwrap())
            .collect();
        let variadic = if sig.variadic() {
            let slice_key = *params.last().unwrap();
            match &self.tc_objs.types[slice_key] {
                Type::Slice(s) => Some(s.elem()),
                // spec: "As a special case, append also accepts a first argument assignable
                // to type []byte with a second argument of string type followed by ... .
                // This form appends the bytes of the string.
                Type::Basic(_) => Some(*self.tc_objs.universe().byte()),
                _ => unreachable!(),
            }
        } else {
            None
        };
        (params, variadic)
    }

    pub fn get_sig_returns_tc_types(&mut self, func: TCTypeKey) -> Vec<TCTypeKey> {
        let typ = &self.tc_objs.types[func].underlying_val(self.tc_objs);
        let sig = typ.try_as_signature().unwrap();
        self.tuple_tc_types(sig.results())
    }

    // returns vm_type(metadata) for the tc_type
    pub fn basic_type_from_tc(&self, tkey: TCTypeKey, vm_objs: &mut VMObjects) -> Option<Meta> {
        self.tc_objs.types[tkey].try_as_basic().map(|x| {
            let typ = x.typ();
            match typ {
                BasicType::Bool | BasicType::UntypedBool => vm_objs.s_meta.mbool,
                BasicType::Int | BasicType::UntypedInt => vm_objs.s_meta.mint,
                BasicType::Int8 => vm_objs.s_meta.mint8,
                BasicType::Int16 => vm_objs.s_meta.mint16,
                BasicType::Int32 | BasicType::Rune | BasicType::UntypedRune => {
                    vm_objs.s_meta.mint32
                }
                BasicType::Int64 => vm_objs.s_meta.mint64,
                BasicType::Uint | BasicType::Uintptr => vm_objs.s_meta.muint,
                BasicType::Uint8 | BasicType::Byte => vm_objs.s_meta.muint8,
                BasicType::Uint16 => vm_objs.s_meta.muint16,
                BasicType::Uint32 => vm_objs.s_meta.muint32,
                BasicType::Uint64 => vm_objs.s_meta.muint64,
                BasicType::Float32 => vm_objs.s_meta.mfloat32,
                BasicType::Float64 | BasicType::UntypedFloat => vm_objs.s_meta.mfloat64,
                BasicType::Complex64 => vm_objs.s_meta.mcomplex64,
                BasicType::Complex128 => vm_objs.s_meta.mcomplex128,
                BasicType::Str | BasicType::UntypedString => vm_objs.s_meta.mstr,
                BasicType::UnsafePointer => vm_objs.s_meta.unsafe_ptr,
                BasicType::UntypedNil => vm_objs.s_meta.none,
                _ => {
                    dbg!(typ);
                    unreachable!()
                }
            }
        })
    }

    // get GosValue from type checker's Obj
    fn const_value(&self, tkey: TCTypeKey, val: &ConstValue) -> GosValue {
        let typ = self.tc_objs.types[tkey]
            .underlying_val(self.tc_objs)
            .try_as_basic()
            .unwrap()
            .typ();
        match typ {
            BasicType::Bool | BasicType::UntypedBool => GosValue::new_bool(val.bool_as_bool()),
            BasicType::Int | BasicType::UntypedInt => {
                let (i, _) = val.to_int().int_as_i64();
                GosValue::new_int(i as isize)
            }
            BasicType::Int8 => {
                let (i, _) = val.to_int().int_as_i64();
                GosValue::new_int8(i as i8)
            }
            BasicType::Int16 => {
                let (i, _) = val.to_int().int_as_i64();
                GosValue::new_int16(i as i16)
            }
            BasicType::Int32 | BasicType::Rune | BasicType::UntypedRune => {
                let (i, _) = val.to_int().int_as_i64();
                GosValue::new_int32(i as i32)
            }
            BasicType::Int64 => {
                let (i, _) = val.to_int().int_as_i64();
                GosValue::new_int64(i)
            }
            BasicType::Uint => {
                let (i, _) = val.to_int().int_as_u64();
                GosValue::new_uint(i as usize)
            }
            BasicType::Uintptr => {
                let (i, _) = val.to_int().int_as_u64();
                GosValue::new_uint_ptr(i as usize)
            }
            BasicType::Uint8 | BasicType::Byte => {
                let (i, _) = val.to_int().int_as_u64();
                GosValue::new_uint8(i as u8)
            }
            BasicType::Uint16 => {
                let (i, _) = val.to_int().int_as_u64();
                GosValue::new_uint16(i as u16)
            }
            BasicType::Uint32 => {
                let (i, _) = val.to_int().int_as_u64();
                GosValue::new_uint32(i as u32)
            }
            BasicType::Uint64 => {
                let (i, _) = val.to_int().int_as_u64();
                GosValue::new_uint64(i)
            }
            BasicType::Float32 => {
                let (f, _) = val.num_as_f32();
                GosValue::new_float32(f.into())
            }
            BasicType::Float64 | BasicType::UntypedFloat => {
                let (f, _) = val.num_as_f64();
                GosValue::new_float64(f.into())
            }
            BasicType::Complex64 => {
                let (cr, ci, _) = val.complex_as_complex64();
                GosValue::new_complex64(cr, ci)
            }
            BasicType::Complex128 => {
                let (cr, ci, _) = val.complex_as_complex128();
                GosValue::Complex128(Box::new((cr, ci)))
            }
            BasicType::Str | BasicType::UntypedString => GosValue::new_str(val.str_as_string()),
            BasicType::UnsafePointer => GosValue::nil_with_meta(self.unsafe_ptr_meta),
            _ => {
                dbg!(typ);
                unreachable!();
            }
        }
    }

    // get vm_type from tc_type
    fn meta_from_tc_impl(
        &mut self,
        typ: TCTypeKey,
        vm_objs: &mut VMObjects,
        dummy_gcv: &mut GcoVec,
    ) -> Meta {
        match &self.tc_objs.types[typ] {
            Type::Basic(_) => self.basic_type_from_tc(typ, vm_objs).unwrap(),
            Type::Array(detail) => {
                let elem = self.meta_from_tc_impl(detail.elem(), vm_objs, dummy_gcv);
                Meta::new_array(elem, detail.len().unwrap() as usize, &mut vm_objs.metas)
            }
            Type::Slice(detail) => {
                let el_type = self.meta_from_tc(detail.elem(), vm_objs, dummy_gcv);
                Meta::new_slice(el_type, &mut vm_objs.metas)
            }
            Type::Map(detail) => {
                let ktype = self.meta_from_tc(detail.key(), vm_objs, dummy_gcv);
                let vtype = self.meta_from_tc(detail.elem(), vm_objs, dummy_gcv);
                Meta::new_map(ktype, vtype, &mut vm_objs.metas)
            }
            Type::Struct(detail) => {
                let fields = self.get_fields(detail.fields(), vm_objs, dummy_gcv);
                Meta::new_struct(fields, vm_objs, dummy_gcv)
            }
            Type::Interface(detail) => {
                let methods = detail.all_methods();
                let fields = self.get_fields(methods.as_ref().unwrap(), vm_objs, dummy_gcv);
                Meta::new_interface(fields, &mut vm_objs.metas)
            }
            Type::Chan(detail) => {
                let typ = match detail.dir() {
                    ChanDir::RecvOnly => ChannelType::Recv,
                    ChanDir::SendOnly => ChannelType::Send,
                    ChanDir::SendRecv => ChannelType::SendRecv,
                };
                let vmeta = self.meta_from_tc(detail.elem(), vm_objs, dummy_gcv);
                Meta::new_channel(typ, vmeta, &mut vm_objs.metas)
            }
            Type::Signature(detail) => {
                let mut convert = |tuple_key| -> Vec<Meta> {
                    self.tc_objs.types[tuple_key]
                        .try_as_tuple()
                        .unwrap()
                        .vars()
                        .iter()
                        .map(|&x| {
                            self.meta_from_tc(
                                self.tc_objs.lobjs[x].typ().unwrap(),
                                vm_objs,
                                dummy_gcv,
                            )
                        })
                        .collect()
                };
                let params = convert(detail.params());
                let results = convert(detail.results());
                let mut recv = None;
                if let Some(r) = detail.recv() {
                    let recv_tc_type = self.tc_objs.lobjs[*r].typ().unwrap();
                    // to avoid infinite recursion
                    if !self.tc_objs.types[recv_tc_type].is_interface(self.tc_objs) {
                        recv = Some(self.meta_from_tc(recv_tc_type, vm_objs, dummy_gcv));
                    }
                }
                let variadic = if detail.variadic() {
                    let slice = params.last().unwrap();
                    match &vm_objs.metas[slice.key] {
                        MetadataType::SliceOrArray(elem, _) => Some((*slice, elem.clone())),
                        _ => unreachable!(),
                    }
                } else {
                    None
                };
                Meta::new_sig(recv, params, results, variadic, &mut vm_objs.metas)
            }
            Type::Pointer(detail) => {
                let inner = self.meta_from_tc(detail.base(), vm_objs, dummy_gcv);
                inner.ptr_to()
            }
            Type::Named(detail) => {
                // generate a Named with dummy underlying to avoid recursion
                let md = Meta::new_named(vm_objs.s_meta.mint, &mut vm_objs.metas);
                for key in detail.methods().iter() {
                    let mobj = &self.tc_objs.lobjs[*key];
                    md.add_method(
                        mobj.name().clone(),
                        mobj.entity_type().func_has_ptr_recv(),
                        &mut vm_objs.metas,
                    )
                }
                self.types_cache.insert(typ, md);
                let underlying = self.meta_from_tc(detail.underlying(), vm_objs, dummy_gcv);
                let (_, underlying_mut) = vm_objs.metas[md.key].as_named_mut();
                *underlying_mut = underlying;
                md
            }
            _ => {
                dbg!(&self.tc_objs.types[typ]);
                unimplemented!()
            }
        }
    }

    pub fn underlying_tc(&self, typ: TCTypeKey) -> TCTypeKey {
        match &self.tc_objs.types[typ] {
            Type::Named(n) => n.underlying(),
            _ => typ,
        }
    }

    pub fn underlying_value_type_from_tc(&self, typ: TCTypeKey) -> ValueType {
        self.value_type_from_tc(self.underlying_tc(typ))
    }

    pub fn value_type_from_tc(&self, typ: TCTypeKey) -> ValueType {
        match &self.tc_objs.types[typ] {
            Type::Basic(detail) => match detail.typ() {
                BasicType::Bool | BasicType::UntypedBool => ValueType::Bool,
                BasicType::Int | BasicType::UntypedInt => ValueType::Int,
                BasicType::Int8 => ValueType::Int8,
                BasicType::Int16 => ValueType::Int16,
                BasicType::Int32 | BasicType::Rune | BasicType::UntypedRune => ValueType::Int32,
                BasicType::Int64 => ValueType::Int64,
                BasicType::Uint => ValueType::Uint,
                BasicType::Uintptr => ValueType::UintPtr,
                BasicType::Uint8 | BasicType::Byte => ValueType::Uint8,
                BasicType::Uint16 => ValueType::Uint16,
                BasicType::Uint32 => ValueType::Uint32,
                BasicType::Uint64 => ValueType::Uint64,
                BasicType::Float32 => ValueType::Float32,
                BasicType::Float64 | BasicType::UntypedFloat => ValueType::Float64,
                BasicType::Complex64 => ValueType::Complex64,
                BasicType::Complex128 => ValueType::Complex128,
                BasicType::Str | BasicType::UntypedString => ValueType::Str,
                BasicType::UnsafePointer => ValueType::UnsafePtr,
                BasicType::UntypedNil => ValueType::Nil,
                _ => {
                    dbg!(detail.typ());
                    unreachable!()
                }
            },
            Type::Array(_) => ValueType::Array,
            Type::Slice(_) => ValueType::Slice,
            Type::Map(_) => ValueType::Map,
            Type::Struct(_) => ValueType::Struct,
            Type::Interface(_) => ValueType::Interface,
            Type::Chan(_) => ValueType::Channel,
            Type::Signature(_) => ValueType::Closure,
            Type::Pointer(_) => ValueType::Pointer,
            Type::Named(n) => self.value_type_from_tc(n.underlying()),
            _ => {
                dbg!(&self.tc_objs.types[typ]);
                unimplemented!()
            }
        }
    }

    fn range_tc_types(&self, typ: TCTypeKey) -> [TCTypeKey; 3] {
        let t_int = self.tc_objs.universe().types()[&BasicType::Int];
        match &self.tc_objs.types[typ] {
            Type::Basic(detail) => match detail.typ() {
                BasicType::Str | BasicType::UntypedString => [typ, t_int, t_int],
                _ => unreachable!(),
            },
            Type::Slice(detail) => [typ, t_int, detail.elem()],
            Type::Map(detail) => [typ, detail.key(), detail.elem()],
            _ => {
                dbg!(&self.tc_objs.types[typ]);
                unreachable!()
            }
        }
    }

    fn tuple_tc_types(&self, typ: TCTypeKey) -> Vec<TCTypeKey> {
        match &self.tc_objs.types[typ] {
            Type::Tuple(detail) => detail
                .vars()
                .iter()
                .map(|x| self.tc_objs.lobjs[*x].typ().unwrap())
                .collect(),
            _ => unreachable!(),
        }
    }

    fn get_fields(
        &mut self,
        fields: &Vec<TCObjKey>,
        vm_objs: &mut VMObjects,
        dummy_gcv: &mut GcoVec,
    ) -> Fields {
        let mut vec = Vec::new();
        let mut map = HashMap::<String, usize>::new();
        for (i, f) in fields.iter().enumerate() {
            let field = &self.tc_objs.lobjs[*f];
            let f_type = self.meta_from_tc(field.typ().unwrap(), vm_objs, dummy_gcv);
            let exported = field.name().chars().next().unwrap().is_uppercase();
            vec.push((f_type, field.name().clone(), exported));
            map.insert(field.name().clone(), i);
        }
        Fields::new(vec, map)
    }
}
