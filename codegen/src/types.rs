// Copyright 2022 The Goscript Authors. All rights reserved.
// Use of this source code is governed by a BSD-style
// license that can be found in the LICENSE file.

use goscript_parser::ast::Node;
use goscript_parser::ast::{Expr, NodeId};
use goscript_parser::objects::IdentKey;
use goscript_parser::Map;
use goscript_types::{
    BasicType, ChanDir, ConstValue, EntityType, ObjKey as TCObjKey, OperandMode,
    PackageKey as TCPackageKey, SelectionKind as TCSelectionKind, TCObjects, Type, TypeInfo,
    TypeKey as TCTypeKey,
};
use goscript_vm::ffi::*;
use goscript_vm::value::*;
use std::vec;

pub type TypeCache = Map<TCTypeKey, Meta>;

#[derive(PartialEq)]
pub enum SelectionType {
    NonMethod,
    MethodNonPtrRecv,
    MethodPtrRecv,
}

pub(crate) struct TypeLookup<'a> {
    tc_objs: &'a TCObjects,
    ti: &'a TypeInfo,
    types_cache: &'a mut TypeCache,
}

impl<'a> TypeLookup<'a> {
    pub fn new(
        tc_objs: &'a TCObjects,
        ti: &'a TypeInfo,
        types_cache: &'a mut TypeCache,
    ) -> TypeLookup<'a> {
        TypeLookup {
            tc_objs,
            ti,
            types_cache,
        }
    }

    #[inline]
    pub fn type_info(&self) -> &TypeInfo {
        self.ti
    }

    #[inline]
    pub fn try_tc_const_value(&self, id: NodeId) -> Option<&ConstValue> {
        let typ_val = self.ti.types.get(&id).unwrap();
        typ_val.get_const_val()
    }

    #[inline]
    pub fn const_type_value(&self, id: NodeId) -> (TCTypeKey, GosValue) {
        let typ_val = self.ti.types.get(&id).unwrap();
        let const_val = typ_val.get_const_val().unwrap();
        let (v, _) = self.const_value_type(typ_val.typ, const_val);
        (typ_val.typ, v)
    }

    #[inline]
    pub fn ident_const_value_type(&self, id: &IdentKey) -> (GosValue, ValueType) {
        let lobj_key = self.ti.defs[id].unwrap();
        let lobj = &self.tc_objs.lobjs[lobj_key];
        let tkey = lobj.typ().unwrap();
        self.const_value_type(tkey, lobj.const_val())
    }

    #[inline]
    pub fn expr_tc_type(&self, e: &Expr) -> TCTypeKey {
        self.node_tc_type(e.id())
    }

    #[inline]
    pub fn node_tc_type(&self, id: NodeId) -> TCTypeKey {
        self.ti.types.get(&id).unwrap().typ
    }

    #[inline]
    pub fn try_expr_mode(&self, e: &Expr) -> Option<&OperandMode> {
        self.ti.types.get(&e.id()).map(|x| &x.mode)
    }

    #[inline]
    pub fn expr_mode(&self, e: &Expr) -> &OperandMode {
        &self.ti.types.get(&e.id()).unwrap().mode
    }

    // some of the built in funcs are not recorded
    pub fn try_expr_tc_type(&self, e: &Expr) -> Option<TCTypeKey> {
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

    pub fn try_pkg_key(&self, e: &Expr) -> Option<TCPackageKey> {
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

    pub fn expr_value_type(&self, e: &Expr) -> ValueType {
        let tv = self.ti.types.get(&e.id()).unwrap();
        if tv.mode == OperandMode::TypeExpr {
            ValueType::Metadata
        } else {
            let t = self.expr_tc_type(e);
            self.tc_type_to_value_type(t)
        }
    }

    pub fn sliceable_expr_value_types(
        &mut self,
        e: &Expr,
        vmctx: &mut CodeGenVMCtx,
    ) -> (ValueType, TCTypeKey) {
        let tc_type = self.expr_tc_type(&e);
        let typ = self.tc_objs.types[tc_type].underlying().unwrap_or(tc_type);
        let meta = self.tc_type_to_meta(typ, vmctx);
        let metas = &vmctx.metas();
        match &metas[meta.key] {
            MetadataType::Array(_, _) => (
                ValueType::Array,
                self.tc_objs.types[typ].try_as_array().unwrap().elem(),
            ),
            MetadataType::Slice(_) => (
                ValueType::Slice,
                self.tc_objs.types[typ].try_as_slice().unwrap().elem(),
            ),
            MetadataType::Str(_) => (
                ValueType::String,
                self.tc_objs.universe().types()[&BasicType::Uint8],
            ),
            _ => unreachable!(),
        }
    }

    pub fn node_meta(&mut self, id: NodeId, vmctx: &mut CodeGenVMCtx) -> Meta {
        let tv = self.ti.types.get(&id).unwrap();
        let md = self.tc_type_to_meta(tv.typ, vmctx);
        if tv.mode == OperandMode::TypeExpr {
            md.into_type_category()
        } else {
            md
        }
    }

    #[inline]
    pub fn object_use(&self, ikey: IdentKey) -> TCObjKey {
        self.ti.uses[&ikey]
    }

    #[inline]
    pub fn object_def(&self, ikey: IdentKey) -> TCObjKey {
        self.ti.defs[&ikey].unwrap()
    }

    #[inline]
    pub fn object_implicit(&self, id: &NodeId) -> TCObjKey {
        self.ti.implicits[id]
    }

    #[inline]
    pub fn obj_use_tc_type(&self, ikey: IdentKey) -> TCTypeKey {
        self.obj_tc_type(self.object_use(ikey))
    }

    #[inline]
    pub fn obj_use_value_type(&self, ikey: IdentKey) -> ValueType {
        self.tc_type_to_value_type(self.obj_use_tc_type(ikey))
    }

    #[inline]
    pub fn obj_def_tc_type(&self, ikey: IdentKey) -> TCTypeKey {
        self.obj_tc_type(self.object_def(ikey))
    }

    #[inline]
    pub fn obj_tc_type(&self, okey: TCObjKey) -> TCTypeKey {
        let obj = &self.tc_objs.lobjs[okey];
        obj.typ().unwrap()
    }

    #[inline]
    pub fn ident_is_def(&self, ikey: &IdentKey) -> bool {
        self.ti.defs.contains_key(ikey)
    }

    #[inline]
    pub fn obj_def_meta(&mut self, ikey: IdentKey, vmctx: &mut CodeGenVMCtx) -> Meta {
        self.tc_type_to_meta(self.obj_def_tc_type(ikey), vmctx)
    }

    #[inline]
    pub fn expr_range_tc_types(&self, e: &Expr) -> [TCTypeKey; 3] {
        let typ = self.ti.types.get(&e.id()).unwrap().typ;
        self.range_tc_types(typ)
    }

    #[inline]
    pub fn expr_tuple_tc_types(&self, e: &Expr) -> Vec<TCTypeKey> {
        let typ = self.ti.types.get(&e.id()).unwrap().typ;
        self.tuple_tc_types(typ)
    }

    pub fn selection_vtypes_indices_sel_typ(
        &self,
        id: NodeId,
    ) -> (TCTypeKey, TCTypeKey, &Vec<usize>, SelectionType) {
        let sel = &self.ti.selections[&id];
        let recv_type = sel.recv().unwrap();
        let obj = &self.tc_objs.lobjs[sel.obj()];
        let expr_type = obj.typ().unwrap();
        let sel_typ = if self.tc_objs.types[expr_type].try_as_signature().is_some() {
            match obj.entity_type() {
                EntityType::Func(ptr_recv) => match ptr_recv {
                    true => SelectionType::MethodPtrRecv,
                    false => SelectionType::MethodNonPtrRecv,
                },
                _ => SelectionType::NonMethod,
            }
        } else {
            SelectionType::NonMethod
        };
        (recv_type, expr_type, &sel.indices(), sel_typ)
    }

    pub fn tc_type_to_meta(&mut self, typ: TCTypeKey, vmctx: &mut CodeGenVMCtx) -> Meta {
        if !self.types_cache.contains_key(&typ) {
            let val = self.tc_type_to_meta_impl(typ, vmctx);
            self.types_cache.insert(typ, val);
        }
        self.types_cache.get(&typ).unwrap().clone()
    }

    pub fn sig_params_tc_types(&self, func: TCTypeKey) -> (Vec<TCTypeKey>, Option<TCTypeKey>) {
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

    pub fn sig_returns_tc_types(&self, func: TCTypeKey) -> Vec<TCTypeKey> {
        let typ = &self.tc_objs.types[func].underlying_val(self.tc_objs);
        let sig = typ.try_as_signature().unwrap();
        self.tuple_tc_types(sig.results())
    }

    pub fn is_method(&self, expr: &Expr) -> bool {
        match expr {
            Expr::Selector(sel_expr) => match self.ti.selections.get(&sel_expr.id()) {
                Some(sel) => match sel.kind() {
                    TCSelectionKind::MethodVal => true,
                    _ => false,
                },
                None => false,
            },
            _ => false,
        }
    }

    // returns vm_type(metadata) for the tc_type
    pub fn basic_type_meta(&self, tkey: TCTypeKey, s_meta: &StaticMeta) -> Option<Meta> {
        self.tc_objs.types[tkey].try_as_basic().map(|x| {
            let typ = x.typ();
            match typ {
                BasicType::Bool | BasicType::UntypedBool => s_meta.mbool,
                BasicType::Int | BasicType::UntypedInt => s_meta.mint,
                BasicType::Int8 => s_meta.mint8,
                BasicType::Int16 => s_meta.mint16,
                BasicType::Int32 | BasicType::Rune | BasicType::UntypedRune => s_meta.mint32,
                BasicType::Int64 => s_meta.mint64,
                BasicType::Uint | BasicType::Uintptr => s_meta.muint,
                BasicType::Uint8 | BasicType::Byte => s_meta.muint8,
                BasicType::Uint16 => s_meta.muint16,
                BasicType::Uint32 => s_meta.muint32,
                BasicType::Uint64 => s_meta.muint64,
                BasicType::Float32 => s_meta.mfloat32,
                BasicType::Float64 | BasicType::UntypedFloat => s_meta.mfloat64,
                BasicType::Complex64 => s_meta.mcomplex64,
                BasicType::Complex128 => s_meta.mcomplex128,
                BasicType::Str | BasicType::UntypedString => s_meta.mstr,
                BasicType::UnsafePointer => s_meta.unsafe_ptr,
                BasicType::UntypedNil => s_meta.none,
                _ => {
                    dbg!(typ);
                    unreachable!()
                }
            }
        })
    }

    // get GosValue from type checker's Obj
    fn const_value_type(&self, tkey: TCTypeKey, val: &ConstValue) -> (GosValue, ValueType) {
        let typ = self.tc_objs.types[tkey]
            .underlying_val(self.tc_objs)
            .try_as_basic()
            .unwrap()
            .typ();
        match typ {
            BasicType::Bool | BasicType::UntypedBool => {
                (val.bool_as_bool().into(), ValueType::Bool)
            }
            BasicType::Int | BasicType::UntypedInt => {
                let (i, _) = val.to_int().int_as_i64();
                ((i as isize).into(), ValueType::Int)
            }
            BasicType::Int8 => {
                let (i, _) = val.to_int().int_as_i64();
                ((i as i8).into(), ValueType::Int8)
            }
            BasicType::Int16 => {
                let (i, _) = val.to_int().int_as_i64();
                ((i as i16).into(), ValueType::Int16)
            }
            BasicType::Int32 | BasicType::Rune | BasicType::UntypedRune => {
                let (i, _) = val.to_int().int_as_i64();
                ((i as i32).into(), ValueType::Int32)
            }
            BasicType::Int64 => {
                let (i, _) = val.to_int().int_as_i64();
                (i.into(), ValueType::Int64)
            }
            BasicType::Uint => {
                let (i, _) = val.to_int().int_as_u64();
                ((i as usize).into(), ValueType::Uint)
            }
            BasicType::Uintptr => {
                let (i, _) = val.to_int().int_as_u64();
                (FfiCtx::new_uint_ptr(i as usize), ValueType::UintPtr)
            }
            BasicType::Uint8 | BasicType::Byte => {
                let (i, _) = val.to_int().int_as_u64();
                ((i as u8).into(), ValueType::Uint8)
            }
            BasicType::Uint16 => {
                let (i, _) = val.to_int().int_as_u64();
                ((i as u16).into(), ValueType::Uint16)
            }
            BasicType::Uint32 => {
                let (i, _) = val.to_int().int_as_u64();
                ((i as u32).into(), ValueType::Uint32)
            }
            BasicType::Uint64 => {
                let (i, _) = val.to_int().int_as_u64();
                (i.into(), ValueType::Uint64)
            }
            BasicType::Float32 => {
                let (f, _) = val.num_as_f32();
                (f.into_inner().into(), ValueType::Float32)
            }
            BasicType::Float64 | BasicType::UntypedFloat => {
                let (f, _) = val.num_as_f64();
                (f.into_inner().into(), ValueType::Float64)
            }
            BasicType::Complex64 => {
                let (cr, ci, _) = val.to_complex().complex_as_complex64();
                (FfiCtx::new_complex64(cr.0, ci.0), ValueType::Complex64)
            }
            BasicType::Complex128 => {
                let (cr, ci, _) = val.to_complex().complex_as_complex128();
                (FfiCtx::new_complex128(cr.0, ci.0), ValueType::Complex128)
            }
            BasicType::Str | BasicType::UntypedString => {
                (FfiCtx::new_string(&val.str_as_string()), ValueType::String)
            }
            BasicType::UnsafePointer => {
                (FfiCtx::new_nil(ValueType::UnsafePtr), ValueType::UnsafePtr)
            }
            _ => {
                dbg!(typ);
                unreachable!();
            }
        }
    }

    // get vm_type from tc_type
    fn tc_type_to_meta_impl(&mut self, typ: TCTypeKey, vmctx: &mut CodeGenVMCtx) -> Meta {
        match &self.tc_objs.types[typ] {
            Type::Basic(_) => self.basic_type_meta(typ, vmctx.s_meta()).unwrap(),
            Type::Array(detail) => {
                let elem = self.tc_type_to_meta_impl(detail.elem(), vmctx);
                Meta::new_array(elem, detail.len().unwrap() as usize, vmctx.metas_mut())
            }
            Type::Slice(detail) => {
                let el_type = self.tc_type_to_meta(detail.elem(), vmctx);
                Meta::new_slice(el_type, vmctx.metas_mut())
            }
            Type::Map(detail) => {
                let ktype = self.tc_type_to_meta(detail.key(), vmctx);
                let vtype = self.tc_type_to_meta(detail.elem(), vmctx);
                Meta::new_map(ktype, vtype, vmctx.metas_mut())
            }
            Type::Struct(detail) => {
                let fields = self.build_fields(detail.fields(), vmctx);
                vmctx.new_struct_meta(fields)
            }
            Type::Interface(detail) => {
                let methods = detail.all_methods();
                let fields = self.build_fields(methods.as_ref().unwrap(), vmctx);
                Meta::new_interface(fields, vmctx.metas_mut())
            }
            Type::Chan(detail) => {
                let typ = match detail.dir() {
                    ChanDir::RecvOnly => ChannelType::Recv,
                    ChanDir::SendOnly => ChannelType::Send,
                    ChanDir::SendRecv => ChannelType::SendRecv,
                };
                let vmeta = self.tc_type_to_meta(detail.elem(), vmctx);
                Meta::new_channel(typ, vmeta, vmctx.metas_mut())
            }
            Type::Signature(detail) => {
                let mut convert = |tuple_key| -> Vec<Meta> {
                    self.tc_objs.types[tuple_key]
                        .try_as_tuple()
                        .unwrap()
                        .vars()
                        .iter()
                        .map(|&x| self.tc_type_to_meta(self.tc_objs.lobjs[x].typ().unwrap(), vmctx))
                        .collect()
                };
                let params = convert(detail.params());
                let results = convert(detail.results());
                let mut recv = None;
                if let Some(r) = detail.recv() {
                    let recv_tc_type = self.tc_objs.lobjs[*r].typ().unwrap();
                    // to avoid infinite recursion
                    if !self.tc_objs.types[recv_tc_type].is_interface(self.tc_objs) {
                        recv = Some(self.tc_type_to_meta(recv_tc_type, vmctx));
                    }
                }
                let variadic = if detail.variadic() {
                    let slice = params.last().unwrap();
                    match &vmctx.metas()[slice.key] {
                        MetadataType::Slice(elem) => Some((*slice, elem.clone())),
                        _ => unreachable!(),
                    }
                } else {
                    None
                };
                Meta::new_sig(recv, params, results, variadic, vmctx.metas_mut())
            }
            Type::Pointer(detail) => {
                let inner = self.tc_type_to_meta(detail.base(), vmctx);
                inner.ptr_to()
            }
            Type::Named(detail) => {
                // generate a Named with dummy underlying to avoid recursion
                let md = Meta::new_named(vmctx.s_meta().mint, vmctx.metas_mut());
                for key in detail.methods().iter() {
                    let mobj = &self.tc_objs.lobjs[*key];
                    md.add_method(
                        mobj.name().clone(),
                        mobj.entity_type().func_has_ptr_recv(),
                        vmctx.metas_mut(),
                    )
                }
                self.types_cache.insert(typ, md);
                let underlying = self.tc_type_to_meta(detail.underlying(), vmctx);
                let (_, underlying_mut) = vmctx.metas_mut()[md.key].as_named_mut();
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

    pub fn obj_underlying_value_type(&self, typ: TCTypeKey) -> ValueType {
        self.tc_type_to_value_type(self.underlying_tc(typ))
    }

    pub fn tc_type_to_value_type(&self, typ: TCTypeKey) -> ValueType {
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
                BasicType::Str | BasicType::UntypedString => ValueType::String,
                BasicType::UnsafePointer => ValueType::UnsafePtr,
                BasicType::UntypedNil => ValueType::Void,
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
            Type::Named(n) => self.tc_type_to_value_type(n.underlying()),
            _ => {
                dbg!(&self.tc_objs.types[typ]);
                unimplemented!()
            }
        }
    }

    pub fn slice_elem_type(&self, typ: TCTypeKey) -> ValueType {
        match &self.tc_objs.types[typ] {
            Type::Slice(s) => self.tc_type_to_value_type(s.elem()),
            _ => unreachable!(),
        }
    }

    pub fn bool_tc_type(&self) -> TCTypeKey {
        self.tc_objs.universe().types()[&BasicType::Bool]
    }

    pub fn should_cast_to_iface(&self, lhs: TCTypeKey, rhs: TCTypeKey) -> bool {
        let vt1 = self.obj_underlying_value_type(rhs);
        self.obj_underlying_value_type(lhs) == ValueType::Interface
            && vt1 != ValueType::Interface
            && vt1 != ValueType::Void
    }

    fn range_tc_types(&self, typ: TCTypeKey) -> [TCTypeKey; 3] {
        let t_int = self.tc_objs.universe().types()[&BasicType::Int];
        let typ = self.tc_objs.types[typ].underlying().unwrap_or(typ);
        match &self.tc_objs.types[typ] {
            Type::Basic(detail) => match detail.typ() {
                BasicType::Str | BasicType::UntypedString => [typ, t_int, t_int],
                _ => unreachable!(),
            },
            Type::Slice(detail) => [typ, t_int, detail.elem()],
            Type::Array(detail) => [typ, t_int, detail.elem()],
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

    pub fn iface_binding_info(
        &mut self,
        i_s: (TCTypeKey, TCTypeKey),
        vmctx: &mut CodeGenVMCtx,
    ) -> (Meta, Vec<IfaceBinding>) {
        let iface = self.tc_type_to_meta(i_s.0, vmctx);
        let named = self.tc_type_to_meta(i_s.1, vmctx);
        let metas = vmctx.metas_mut();
        let fields: Vec<&String> = match &metas[iface.underlying(metas).key] {
            MetadataType::Interface(m) => m.all().iter().map(|x| &x.name).collect(),
            _ => unreachable!(),
        };
        (
            named,
            fields
                .iter()
                .map(|x| named.get_iface_binding(x, metas).unwrap())
                .collect(),
        )
    }

    fn build_fields(&mut self, fields: &Vec<TCObjKey>, vmctx: &mut CodeGenVMCtx) -> Fields {
        let mut infos = Vec::new();
        let mut map = Map::<String, Vec<usize>>::new();
        for (i, f) in fields.iter().enumerate() {
            let field = &self.tc_objs.lobjs[*f];
            let f_type = self.tc_type_to_meta(field.typ().unwrap(), vmctx);
            let is_exported = field.name().chars().next().unwrap().is_uppercase();
            let is_embedded =
                field.entity_type().is_var() && field.entity_type().var_property().embedded;
            infos.push(FieldInfo {
                meta: f_type,
                name: field.name().clone(),
                exported: is_exported,
                embedded: is_embedded,
            });
            map.insert(field.name().clone(), vec![i]);
            if is_embedded {
                match f_type.mtype_unwraped(&vmctx.metas()) {
                    MetadataType::Struct(fields, _) => {
                        for (k, v) in fields.mapping() {
                            let mut indices = vec![i];
                            indices.append(&mut v.clone());
                            map.insert(k.clone(), indices);
                        }
                    }
                    _ => {}
                }
            }
        }

        Fields::new(infos, map)
    }
}
