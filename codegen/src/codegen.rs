// Copyright 2022 The Goscript Authors. All rights reserved.
// Use of this source code is governed by a BSD-style
// license that can be found in the LICENSE file.

use slotmap::{Key, KeyData};
use std::convert::TryFrom;
use std::iter::FromIterator;

use super::branch::*;
use super::call::CallHelper;
use super::emit::*;
use super::package::PkgHelper;
use super::selector::*;
use super::types::{SelectionType, TypeCache, TypeLookup};

use goscript_vm::gc::GcoVec;
use goscript_vm::instruction::*;
use goscript_vm::metadata::*;
use goscript_vm::objects::EntIndex;
use goscript_vm::value::*;

use goscript_parser::ast::*;
use goscript_parser::objects::Objects as AstObjects;
use goscript_parser::objects::*;
use goscript_parser::position::Pos;
use goscript_parser::token::Token;
use goscript_parser::visitor::{walk_decl, walk_expr, walk_stmt, ExprVisitor, StmtVisitor};
use goscript_types::{
    identical_ignore_tags, Builtin, ObjKey as TCObjKey, OperandMode, PackageKey as TCPackageKey,
    TCObjects, Type, TypeInfo, TypeKey as TCTypeKey,
};

macro_rules! current_func_mut {
    ($owner:ident) => {
        &mut $owner.objects.functions[*$owner.func_stack.last().unwrap()]
    };
}

macro_rules! current_func {
    ($owner:ident) => {
        &$owner.objects.functions[*$owner.func_stack.last().unwrap()]
    };
}

macro_rules! current_func_emitter {
    ($owner:ident) => {
        Emitter::new(current_func_mut!($owner))
    };
}

macro_rules! use_ident_unique_key {
    ($owner:ident, $var:expr) => {
        $owner.t.object_use($var).data()
    };
}

#[allow(unused_macros)]
macro_rules! dbg_tc {
    ($obj:expr, $self_:expr) => {
        let d = goscript_types::Displayer::new($obj, Some($self_.ast_objs), Some($self_.tc_objs));
        println!("{}", d);
    };
}

/// CodeGen implements the code generation logic.
pub struct CodeGen<'a> {
    objects: &'a mut VMObjects,
    ast_objs: &'a AstObjects,
    tc_objs: &'a TCObjects,
    dummy_gcv: &'a mut GcoVec,
    t: TypeLookup<'a>,
    iface_selector: &'a mut IfaceSelector,
    struct_selector: &'a mut StructSelector,
    call_helper: &'a mut CallHelper,
    branch_helper: &'a mut BranchHelper,
    pkg_helper: &'a mut PkgHelper<'a>,

    pkg_key: PackageKey,
    func_stack: Vec<FunctionKey>,
    func_t_stack: Vec<TCTypeKey>, // for casting return values to interfaces
    blank_ident: IdentKey,
}

impl<'a> CodeGen<'a> {
    pub fn new(
        vmo: &'a mut VMObjects,
        asto: &'a AstObjects,
        tco: &'a TCObjects,
        dummy_gcv: &'a mut GcoVec,
        ti: &'a TypeInfo,
        type_cache: &'a mut TypeCache,
        iface_selector: &'a mut IfaceSelector,
        struct_selector: &'a mut StructSelector,
        call_helper: &'a mut CallHelper,
        branch_helper: &'a mut BranchHelper,
        pkg_helper: &'a mut PkgHelper<'a>,
        pkg: PackageKey,
        bk: IdentKey,
    ) -> CodeGen<'a> {
        CodeGen {
            objects: vmo,
            ast_objs: asto,
            tc_objs: tco,
            dummy_gcv: dummy_gcv,
            t: TypeLookup::new(tco, ti, type_cache),
            iface_selector: iface_selector,
            struct_selector: struct_selector,
            call_helper: call_helper,
            branch_helper: branch_helper,
            pkg_helper: pkg_helper,
            pkg_key: pkg,
            func_stack: Vec::new(),
            func_t_stack: Vec::new(),
            blank_ident: bk,
        }
    }

    pub fn pkg_helper(&mut self) -> &mut PkgHelper<'a> {
        &mut self.pkg_helper
    }

    fn resolve_any_ident(&mut self, ident: &IdentKey, expr: Option<&Expr>) -> EntIndex {
        let mode = expr.map_or(&OperandMode::Value, |x| self.t.expr_mode(x));
        match mode {
            OperandMode::TypeExpr => {
                let tctype = self.t.underlying_tc(self.t.obj_use_tc_type(*ident));
                match self.t.basic_type_meta(tctype, self.objects) {
                    Some(meta) => EntIndex::TypeMeta(meta),
                    None => {
                        let id = &self.ast_objs.idents[*ident];
                        if id.name == "error" {
                            EntIndex::TypeMeta(self.t.tc_type_to_meta(
                                tctype,
                                self.objects,
                                self.dummy_gcv,
                            ))
                        } else {
                            self.resolve_var_ident(ident)
                        }
                    }
                }
            }
            OperandMode::Value => {
                let id = &self.ast_objs.idents[*ident];
                match &*id.name {
                    "true" => EntIndex::BuiltInVal(Opcode::PUSH_TRUE),
                    "false" => EntIndex::BuiltInVal(Opcode::PUSH_FALSE),
                    "nil" => EntIndex::BuiltInVal(Opcode::PUSH_NIL),
                    _ => self.resolve_var_ident(ident),
                }
            }
            _ => self.resolve_var_ident(ident),
        }
    }

    fn resolve_var_ident(&mut self, ident: &IdentKey) -> EntIndex {
        let okey = self.t.object_use(*ident);
        let entity_key: KeyData = okey.data();
        // 1. try local first
        if let Some(index) = current_func!(self).entity_index(&entity_key).map(|x| *x) {
            return index;
        }
        // 2. try upvalue
        let upvalue = self
            .func_stack
            .clone()
            .iter()
            .skip(1) // skip package constructor
            .rev()
            .skip(1) // skip itself
            .find_map(|ifunc| {
                let f = &mut self.objects.functions[*ifunc];
                let index = f.entity_index(&entity_key).map(|x| *x);
                if let Some(ind) = index {
                    let desc =
                        ValueDesc::new(*ifunc, ind.into(), self.t.obj_use_value_type(*ident), true);
                    Some(desc)
                } else {
                    None
                }
            });
        if let Some(uv) = upvalue {
            let func = current_func_mut!(self);
            let index = func.try_add_upvalue(&entity_key, uv);
            return index;
        }
        // 3. must be package member
        self.pkg_helper.get_member_index(okey, *ident)
    }

    fn add_local_or_resolve_ident(
        &mut self,
        ikey: &IdentKey,
        is_def: bool,
    ) -> (EntIndex, Option<TCTypeKey>, usize) {
        let ident = &self.ast_objs.idents[*ikey];
        let pos = ident.pos;
        if ident.is_blank() {
            return (EntIndex::Blank, None, pos);
        }
        if is_def {
            let tc_obj = self.t.object_def(*ikey);
            let (index, tc_type, _) = self.add_local_var(tc_obj);
            let func = current_func_mut!(self);
            if func.is_ctor() {
                let pkg_key = func.package;
                let pkg = &mut self.objects.packages[pkg_key];
                pkg.add_var_mapping(ident.name.clone(), index.into());
            }
            (index, Some(tc_type), pos)
        } else {
            let index = self.resolve_var_ident(ikey);
            let t = self.t.obj_use_tc_type(*ikey);
            (index, Some(t), pos)
        }
    }

    fn add_local_var(&mut self, okey: TCObjKey) -> (EntIndex, TCTypeKey, Meta) {
        let tc_type = self.t.obj_tc_type(okey);
        let meta = self
            .t
            .tc_type_to_meta(tc_type, self.objects, self.dummy_gcv);
        let zero_val = meta.zero(&self.objects.metas, self.dummy_gcv);
        let func = current_func_mut!(self);
        let ident_key = Some(okey.data());
        let index = func.add_local(ident_key);
        func.add_local_zero(zero_val, meta.value_type(&self.objects.metas));
        (index, tc_type, meta)
    }

    fn gen_def_var(&mut self, vs: &ValueSpec) {
        let lhs = vs
            .names
            .iter()
            .map(|n| -> (LeftHandSide, Option<TCTypeKey>, usize) {
                let (index, t, pos) = self.add_local_or_resolve_ident(n, true);
                (LeftHandSide::Primitive(index), t, pos)
            })
            .collect::<Vec<(LeftHandSide, Option<TCTypeKey>, usize)>>();
        let rhs = if vs.values.is_empty() {
            RightHandSide::Nothing
        } else {
            RightHandSide::Values(&vs.values)
        };
        self.gen_assign_def_var(&lhs, &vs.typ, &rhs);
    }

    fn gen_def_const(&mut self, names: &Vec<IdentKey>) {
        for name in names.iter() {
            let (val, typ) = self.t.ident_const_value_type(name);
            self.current_func_add_const_def(name, val, typ);
        }
    }

    /// entrance for all assign related stmts
    /// var x
    /// x := 0
    /// x += 1
    /// x++
    /// for x := range xxx
    /// recv clause of select stmt
    fn gen_assign(
        &mut self,
        token: &Token,
        lhs_exprs: &Vec<&Expr>,
        rhs: RightHandSide,
    ) -> Option<usize> {
        let lhs = lhs_exprs
            .iter()
            .map(|expr| {
                match expr {
                    Expr::Ident(ident) => {
                        let is_def = self.t.ident_is_def(ident);
                        let (idx, t, p) = self.add_local_or_resolve_ident(ident, is_def);
                        (LeftHandSide::Primitive(idx), t, p)
                    }
                    Expr::Index(ind_expr) => {
                        let obj = &ind_expr.as_ref().expr;
                        self.visit_expr(obj);
                        let obj_typ = self.t.expr_value_type(obj);
                        let ind = &ind_expr.as_ref().index;
                        let pos = ind_expr.as_ref().l_brack;

                        let mut index_const = None;
                        let mut index_typ = None;
                        if let Some(const_val) = self.t.try_tc_const_value(ind.id()) {
                            let (ival, _) = const_val.to_int().int_as_i64();
                            if let Ok(i) = OpIndex::try_from(ival) {
                                index_const = Some(i);
                            }
                        }
                        if index_const.is_none() {
                            self.visit_expr(ind);
                            index_typ = Some(self.t.expr_value_type(ind));
                        }
                        (
                            LeftHandSide::IndexExpr(IndexLhsInfo::new(
                                0,
                                index_const,
                                obj_typ,
                                index_typ,
                            )), // the true index will be calculated later
                            Some(self.t.expr_tc_type(expr)),
                            pos,
                        )
                    }
                    Expr::Selector(sexpr) => {
                        let pos = self.ast_objs.idents[sexpr.sel].pos;
                        match self.t.try_pkg_key(&sexpr.expr) {
                            Some(key) => {
                                let pkg = self.pkg_helper.get_runtime_key(key);
                                (
                                    // the true index will be calculated later
                                    LeftHandSide::Primitive(EntIndex::PackageMember(
                                        pkg,
                                        sexpr.sel.data(),
                                    )),
                                    Some(self.t.expr_tc_type(expr)),
                                    pos,
                                )
                            }
                            None => {
                                let t =
                                    self.t
                                        .node_meta(sexpr.expr.id(), self.objects, self.dummy_gcv);
                                let name = &self.ast_objs.idents[sexpr.sel].name;
                                let indices = t.field_indices(name, &self.objects.metas).to_vec();
                                self.visit_expr(&sexpr.expr);
                                let obj_typ = self.t.expr_value_type(&sexpr.expr);
                                (
                                    // the true index will be calculated later
                                    LeftHandSide::SelExpr(SelLhsInfo::new(0, indices, obj_typ)),
                                    Some(self.t.expr_tc_type(expr)),
                                    pos,
                                )
                            }
                        }
                    }
                    Expr::Star(sexpr) => {
                        self.visit_expr(&sexpr.expr);
                        (
                            LeftHandSide::Deref(0), // the true index will be calculated later
                            Some(self.t.expr_tc_type(expr)),
                            sexpr.star,
                        )
                    }
                    _ => unreachable!(),
                }
            })
            .collect::<Vec<(LeftHandSide, Option<TCTypeKey>, usize)>>();

        match rhs {
            RightHandSide::Nothing => {
                let code = match token {
                    Token::INC => Opcode::UNARY_ADD,
                    Token::DEC => Opcode::UNARY_SUB,
                    _ => unreachable!(),
                };
                let typ = self.t.expr_value_type(&lhs_exprs[0]);
                self.gen_op_assign(&lhs[0].0, (code, None), None, typ, lhs[0].2);
                None
            }
            RightHandSide::Values(rhs_exprs) => {
                let simple_op = match token {
                    Token::ADD_ASSIGN => Some(Opcode::ADD),         // +=
                    Token::SUB_ASSIGN => Some(Opcode::SUB),         // -=
                    Token::MUL_ASSIGN => Some(Opcode::MUL),         // *=
                    Token::QUO_ASSIGN => Some(Opcode::QUO),         // /=
                    Token::REM_ASSIGN => Some(Opcode::REM),         // %=
                    Token::AND_ASSIGN => Some(Opcode::AND),         // &=
                    Token::OR_ASSIGN => Some(Opcode::OR),           // |=
                    Token::XOR_ASSIGN => Some(Opcode::XOR),         // ^=
                    Token::SHL_ASSIGN => Some(Opcode::SHL),         // <<=
                    Token::SHR_ASSIGN => Some(Opcode::SHR),         // >>=
                    Token::AND_NOT_ASSIGN => Some(Opcode::AND_NOT), // &^=
                    Token::ASSIGN | Token::DEFINE => None,
                    _ => unreachable!(),
                };
                if let Some(code) = simple_op {
                    assert_eq!(lhs_exprs.len(), 1);
                    assert_eq!(rhs_exprs.len(), 1);
                    let ltyp = self.t.expr_value_type(&lhs_exprs[0]);
                    let rtyp = match code {
                        Opcode::SHL | Opcode::SHR => {
                            let t = self.t.expr_value_type(&rhs_exprs[0]);
                            Some(t)
                        }
                        _ => None,
                    };
                    self.gen_op_assign(
                        &lhs[0].0,
                        (code, rtyp),
                        Some(&rhs_exprs[0]),
                        ltyp,
                        lhs[0].2,
                    );
                    None
                } else {
                    self.gen_assign_def_var(&lhs, &None, &rhs)
                }
            }
            _ => self.gen_assign_def_var(&lhs, &None, &rhs),
        }
    }

    fn gen_assign_def_var(
        &mut self,
        lhs: &Vec<(LeftHandSide, Option<TCTypeKey>, usize)>,
        typ: &Option<Expr>,
        rhs: &RightHandSide,
    ) -> Option<usize> {
        let mut range_marker = None;
        let mut lhs_on_stack_top = false;
        // handle the right hand side
        let types = match rhs {
            RightHandSide::Nothing => {
                // define without values
                let t = self.t.expr_tc_type(&typ.as_ref().unwrap());
                let meta = self.t.tc_type_to_meta(t, self.objects, self.dummy_gcv);
                let mut types = Vec::with_capacity(lhs.len());
                for (_, _, pos) in lhs.iter() {
                    let mut emitter = current_func_emitter!(self);
                    let i = emitter.add_const(None, GosValue::new_metadata(meta));
                    emitter.emit_push_zero_val(i.into(), Some(*pos));
                    types.push(t);
                }
                types
            }
            RightHandSide::Values(values) => {
                let val0 = &values[0];
                let val0_mode = self.t.expr_mode(val0);
                if values.len() == 1
                    && (val0_mode == &OperandMode::CommaOk || val0_mode == &OperandMode::MapIndex)
                {
                    let comma_ok = lhs.len() == 2;
                    let types = if comma_ok {
                        self.t.expr_tuple_tc_types(val0)
                    } else {
                        vec![self.t.expr_tc_type(val0)]
                    };
                    match val0 {
                        Expr::TypeAssert(tae) => {
                            self.gen_type_assert(&tae.expr, &tae.typ, comma_ok);
                        }
                        Expr::Index(ie) => {
                            let t = self.t.tc_type_to_value_type(types[0]);
                            self.gen_index(&ie.expr, &ie.index, t, comma_ok);
                        }
                        Expr::Unary(recv_expr) => {
                            assert_eq!(recv_expr.op, Token::ARROW);
                            self.visit_expr(&recv_expr.expr);
                            let t = self.t.expr_value_type(&recv_expr.expr);
                            assert_eq!(t, ValueType::Channel);
                            let comma_ok_flag = comma_ok.then(|| ValueType::FlagA);
                            current_func_mut!(self).emit_code_with_type2(
                                Opcode::RECV,
                                t,
                                comma_ok_flag,
                                Some(recv_expr.op_pos),
                            );
                        }
                        _ => {
                            unreachable!()
                        }
                    }
                    types
                } else if values.len() == lhs.len() {
                    // define or assign with values
                    let mut types = Vec::with_capacity(values.len());
                    for val in values.iter() {
                        self.visit_expr(val);
                        let rhs_type = self.t.expr_tc_type(val);
                        types.push(rhs_type);
                    }
                    types
                } else if values.len() == 1 {
                    let expr = val0;
                    // define or assign with function call on the right
                    if let Expr::Call(_) = expr {
                        self.visit_expr(expr);
                    } else {
                        unreachable!()
                    }
                    self.t.expr_tuple_tc_types(expr)
                } else {
                    unreachable!();
                }
            }
            RightHandSide::Range(r) => {
                // the range statement
                self.visit_expr(r);
                let tkv = self.t.expr_range_tc_types(r);
                let types = [
                    Some(self.t.tc_type_to_value_type(tkv[0])),
                    Some(self.t.tc_type_to_value_type(tkv[1])),
                    Some(self.t.tc_type_to_value_type(tkv[2])),
                ];
                let pos = Some(r.pos(&self.ast_objs));
                let func = current_func_mut!(self);
                func.emit_inst(Opcode::RANGE_INIT, types, None, pos);
                range_marker = Some(func.next_code_index());
                // the block_end address to be set
                func.emit_inst(Opcode::RANGE, types, None, pos);
                tkv[1..].to_vec()
            }
            RightHandSide::SelectRecv(rhs) => {
                // only when in select stmt, lhs in stack is on top of the rhs
                lhs_on_stack_top = true;
                let comma_ok = lhs.len() == 2 && self.t.expr_mode(rhs) == &OperandMode::CommaOk;
                if comma_ok {
                    self.t.expr_tuple_tc_types(rhs)
                } else {
                    vec![self.t.expr_tc_type(rhs)]
                }
            }
        };

        let mut on_stack_types = vec![];
        // lhs types
        for (i, _, _) in lhs.iter().rev() {
            on_stack_types.append(&mut CodeGen::lhs_on_stack_value_types(i));
        }
        let lhs_stack_space = on_stack_types.len() as OpIndex;

        assert_eq!(lhs.len(), types.len());
        let total_val = types.len() as OpIndex;
        let total_stack_space = lhs_stack_space + total_val;
        let (mut current_indexing_deref_index, rhs_begin_index) = match lhs_on_stack_top {
            true => (-lhs_stack_space, -total_stack_space),
            false => (-total_stack_space, -total_val),
        };
        let mut rhs_types = lhs
            .iter()
            .enumerate()
            .map(|(i, (l, _, p))| {
                let rhs_index = i as OpIndex + rhs_begin_index;
                let typ = self.try_cast_to_iface(lhs[i].1, types[i], rhs_index, *p);
                let pos = Some(*p);
                match l {
                    LeftHandSide::Primitive(_) => {
                        let fkey = self.func_stack.last().unwrap();
                        current_func_emitter!(self).emit_store(
                            l,
                            rhs_index,
                            None,
                            Some((self.pkg_helper.pairs_mut(), *fkey)),
                            typ,
                            pos,
                        );
                    }
                    LeftHandSide::IndexExpr(info) => {
                        current_func_emitter!(self).emit_store(
                            &LeftHandSide::IndexExpr(
                                info.clone_with_index(current_indexing_deref_index),
                            ),
                            rhs_index,
                            None,
                            None,
                            typ,
                            pos,
                        );
                        // the lhs of IndexExpr takes two spots
                        current_indexing_deref_index += 2;
                    }
                    LeftHandSide::SelExpr(info) => {
                        current_func_emitter!(self).emit_store(
                            &LeftHandSide::SelExpr(
                                info.clone_with_index(current_indexing_deref_index),
                            ),
                            rhs_index,
                            None,
                            None,
                            typ,
                            pos,
                        );
                        // the lhs of SelExpr takes two spots
                        current_indexing_deref_index += 1;
                    }
                    LeftHandSide::Deref(_) => {
                        current_func_emitter!(self).emit_store(
                            &LeftHandSide::Deref(current_indexing_deref_index),
                            rhs_index,
                            None,
                            None,
                            typ,
                            pos,
                        );
                        current_indexing_deref_index += 1;
                    }
                }
                typ
            })
            .collect();

        let pos = Some(lhs[0].2);
        if !lhs_on_stack_top {
            on_stack_types.append(&mut rhs_types);
        } else {
            on_stack_types.splice(0..0, rhs_types);
        }
        current_func_emitter!(self).emit_pop(&on_stack_types, pos);
        range_marker
    }

    fn gen_op_assign(
        &mut self,
        left: &LeftHandSide,
        op: (Opcode, Option<ValueType>),
        right: Option<&Expr>,
        typ: ValueType,
        p: usize,
    ) {
        let pos = Some(p);
        let mut on_stack_types = CodeGen::lhs_on_stack_value_types(left);

        match right {
            Some(e) => {
                self.visit_expr(e);
                on_stack_types.push(self.t.expr_value_type(e));
            }
            None => {}
        };

        // If this is SHL/SHR,  cast the rhs to uint32
        if let Some(t) = op.1 {
            if t != ValueType::Uint32 {
                current_func_emitter!(self).emit_cast(ValueType::Uint32, t, None, -1, 0, pos);
                *on_stack_types.last_mut().unwrap() = ValueType::Uint32;
            }
        }

        match left {
            LeftHandSide::Primitive(_) => {
                // why no magic number?
                // local index is resolved in gen_assign
                let mut emitter = current_func_emitter!(self);
                let fkey = self.func_stack.last().unwrap();
                emitter.emit_store(
                    left,
                    -1,
                    Some(op),
                    Some((self.pkg_helper.pairs_mut(), *fkey)),
                    typ,
                    pos,
                );
            }
            LeftHandSide::IndexExpr(info) => {
                // stack looks like this(bottom to top) :
                //  [... target, index, value] or [... target, value]
                current_func_emitter!(self).emit_store(
                    &LeftHandSide::IndexExpr(
                        info.clone_with_index(-(on_stack_types.len() as OpIndex)),
                    ),
                    -1,
                    Some(op),
                    None,
                    typ,
                    pos,
                );
            }
            LeftHandSide::SelExpr(info) => {
                // stack looks like this(bottom to top) :
                //  [... target, index, value] or [... target, value]
                current_func_emitter!(self).emit_store(
                    &LeftHandSide::SelExpr(
                        info.clone_with_index(-(on_stack_types.len() as OpIndex)),
                    ),
                    -1,
                    Some(op),
                    None,
                    typ,
                    pos,
                );
            }
            LeftHandSide::Deref(_) => {
                // stack looks like this(bottom to top) :
                //  [... target, value]
                let mut emitter = current_func_emitter!(self);
                emitter.emit_store(
                    &LeftHandSide::Deref(-(on_stack_types.len() as OpIndex)),
                    -1,
                    Some(op),
                    None,
                    typ,
                    pos,
                );
            }
        }
        current_func_emitter!(self).emit_pop(&on_stack_types, pos);
    }

    fn gen_switch_body(&mut self, body: &BlockStmt, tag_type: ValueType) {
        let mut helper = SwitchHelper::new();
        let mut has_default = false;
        for (i, stmt) in body.list.iter().enumerate() {
            helper.add_case_clause();
            let cc = SwitchHelper::to_case_clause(stmt);
            match &cc.list {
                Some(l) => {
                    for c in l.iter() {
                        let pos = Some(stmt.pos(&self.ast_objs));
                        self.visit_expr(c);
                        let func = current_func_mut!(self);
                        helper.tags.add_case(i, func.next_code_index());
                        func.emit_code_with_type(Opcode::SWITCH, tag_type, pos);
                    }
                }
                None => has_default = true,
            }
        }

        // pop the tag
        current_func_emitter!(self).emit_pop(&vec![tag_type], None);

        let func = current_func_mut!(self);
        helper.tags.add_default(func.next_code_index());
        func.emit_code(Opcode::JUMP, None);

        for (i, stmt) in body.list.iter().enumerate() {
            let cc = SwitchHelper::to_case_clause(stmt);
            let func = current_func_mut!(self);
            let default = cc.list.is_none();
            if default {
                helper.tags.patch_default(func, func.next_code_index());
            } else {
                helper.tags.patch_case(func, i, func.next_code_index());
            }
            for s in cc.body.iter() {
                self.visit_stmt(s);
            }
            if !SwitchHelper::has_fall_through(stmt) {
                let func = current_func_mut!(self);
                if default {
                    helper.ends.add_default(func.next_code_index());
                } else {
                    helper.ends.add_case(i, func.next_code_index());
                }
                func.emit_code(Opcode::JUMP, None);
            }
        }
        let end = current_func!(self).next_code_index();
        helper.patch_ends(current_func_mut!(self), end);
        // jump to the end if there is no default code
        if !has_default {
            let func = current_func_mut!(self);
            helper.tags.patch_default(func, end);
        }
    }

    fn gen_func_def(
        &mut self,
        tc_type: TCTypeKey, // Meta,
        fkey: FuncTypeKey,
        recv: Option<FieldList>,
        body: &BlockStmt,
    ) -> FunctionKey {
        let typ = &self.ast_objs.ftypes[fkey];
        let fmeta = self
            .t
            .tc_type_to_meta(tc_type, &mut self.objects, self.dummy_gcv);
        let f = GosValue::function_with_meta(
            self.pkg_key,
            fmeta,
            self.objects,
            self.dummy_gcv,
            FuncFlag::Default,
        );
        let fkey = *f.as_function();
        let mut emitter = Emitter::new(&mut self.objects.functions[fkey]);
        if let Some(fl) = &typ.results {
            emitter.add_params(&fl, self.ast_objs, &self.t);
        }
        match recv {
            Some(recv) => {
                let mut fields = recv;
                fields.list.append(&mut typ.params.list.clone());
                emitter.add_params(&fields, self.ast_objs, &self.t)
            }
            None => emitter.add_params(&typ.params, self.ast_objs, &self.t),
        };
        self.func_stack.push(fkey);
        self.func_t_stack.push(tc_type);
        // process function body
        self.visit_stmt_block(body);

        let func = &mut self.objects.functions[fkey];
        // it will not be executed if it's redundant
        Emitter::new(func).emit_return(None, Some(body.r_brace));

        self.func_stack.pop();
        self.func_t_stack.pop();
        fkey
    }

    fn gen_call(&mut self, func_expr: &Expr, params: &Vec<Expr>, ellipsis: bool, style: CallStyle) {
        let pos = Some(func_expr.pos(&self.ast_objs));
        match *self.t.expr_mode(func_expr) {
            // built in function
            OperandMode::Builtin(builtin) => {
                let opcode = match builtin {
                    Builtin::New => Opcode::NEW,
                    Builtin::Make => Opcode::MAKE,
                    Builtin::Complex => Opcode::COMPLEX,
                    Builtin::Real => Opcode::REAL,
                    Builtin::Imag => Opcode::IMAG,
                    Builtin::Len => Opcode::LEN,
                    Builtin::Cap => Opcode::CAP,
                    Builtin::Append => Opcode::APPEND,
                    Builtin::Copy => Opcode::COPY,
                    Builtin::Delete => Opcode::DELETE,
                    Builtin::Close => Opcode::CLOSE,
                    Builtin::Panic => Opcode::PANIC,
                    Builtin::Recover => Opcode::RECOVER,
                    Builtin::Assert => Opcode::ASSERT,
                    Builtin::Ffi => Opcode::FFI,
                    _ => unimplemented!(),
                };
                for e in params.iter() {
                    self.visit_expr(e);
                }
                // some of the built in funcs are not recorded
                if let Some(t) = self.t.try_expr_tc_type(func_expr) {
                    self.try_cast_params_to_iface(t, params, ellipsis);
                    if opcode == Opcode::FFI {
                        // FFI needs the signature of the call
                        let meta = self.t.tc_type_to_meta(t, self.objects, self.dummy_gcv);
                        let mut emitter = current_func_emitter!(self);
                        emitter.emit_load(EntIndex::TypeMeta(meta), None, ValueType::Metadata, pos);
                    }
                }
                let (param0t, param_last_t) = match params.len() > 0 {
                    true => (
                        Some(self.t.expr_value_type(&params[0])),
                        Some(self.t.expr_value_type(params.last().unwrap())),
                    ),
                    false => (None, None),
                };
                let bf = self.tc_objs.universe().builtins()[&builtin];
                let param_count = params.len() as OpIndex;
                let special_case = (opcode == Opcode::APPEND || opcode == Opcode::COPY)
                    && param_last_t.map_or(false, |x| x == ValueType::String);
                let (t_variadic, count) = match special_case {
                    true => (Some(ValueType::FlagC), Some(0)), // special case,
                    false => match bf.variadic {
                        true => match ellipsis {
                            true => (Some(ValueType::FlagB), Some(0)), // do not pack params if there is ellipsis
                            false => (
                                param_last_t,
                                Some(bf.arg_count as OpIndex - param_count + 1),
                            ),
                        },
                        false => (Some(ValueType::FlagA), Some(param_count as OpIndex)),
                    },
                };
                let (t0, t1) = match opcode {
                    Opcode::DELETE => (param0t, param_last_t),
                    Opcode::MAKE | Opcode::NEW => {
                        // provide the type of param0 too instead of only ValueType::Metadata
                        let t = self.t.expr_tc_type(&params[0]);
                        (param0t, Some(self.t.tc_type_to_value_type(t)))
                    }
                    Opcode::PANIC => (Some(ValueType::Interface), None),
                    Opcode::APPEND | Opcode::COPY => {
                        let (_, t_elem) = self.t.sliceable_expr_value_types(
                            &params[0],
                            self.objects,
                            self.dummy_gcv,
                        );
                        (Some(t_elem), None)
                    }
                    _ => (param0t, None),
                };
                let func = current_func_mut!(self);
                func.emit_inst(opcode, [t0, t1, t_variadic], count, pos);
            }
            // conversion
            // from the specs:
            /*
            A non-constant value x can be converted to type T in any of these cases:
                x is assignable to T.
                +3 [struct] ignoring struct tags (see below), x's type and T have identical underlying types.
                +4 [pointer] ignoring struct tags (see below), x's type and T are pointer types that are not defined types, and their pointer base types have identical underlying types.
                +5 [number] x's type and T are both integer or floating point types.
                +6 [number] x's type and T are both complex types.
                +7 [string] x is an integer or a slice of bytes or runes and T is a string type.
                +8 [slice] x is a string and T is a slice of bytes or runes.
            A value x is assignable to a variable of type T ("x is assignable to T") if one of the following conditions applies:
                - x's type is identical to T.
                - x's type V and T have identical underlying types and at least one of V or T is not a defined type.
                +1 [interface] T is an interface type and x implements T.
                +2 [channel] x is a bidirectional channel value, T is a channel type, x's type V and T have identical element types, and at least one of V or T is not a defined type.
                - x is the predeclared identifier nil and T is a pointer, function, slice, map, channel, or interface type.
                - x is an untyped constant representable by a value of type T.
            */
            OperandMode::TypeExpr => {
                assert!(params.len() == 1);
                self.visit_expr(&params[0]);
                let tc_to = self.t.underlying_tc(self.t.expr_tc_type(func_expr));
                let typ_to = self.t.tc_type_to_value_type(tc_to);
                let tc_from = self.t.underlying_tc(self.t.expr_tc_type(&params[0]));
                let typ_from = self.t.tc_type_to_value_type(tc_from);

                if typ_from == ValueType::Void
                    || identical_ignore_tags(tc_to, tc_from, self.tc_objs)
                {
                    // just ignore conversion if it's nil or types are identical
                    // or convert between Named type and underlying type,
                    // or both types are Named in case they are Structs
                } else {
                    match typ_to {
                        ValueType::Interface => {
                            if typ_from != ValueType::Void {
                                let iface_index = self.iface_selector.get_index(
                                    (tc_to, tc_from),
                                    &mut self.t,
                                    self.objects,
                                    self.dummy_gcv,
                                );
                                current_func_emitter!(self).emit_cast(
                                    typ_to,
                                    typ_from,
                                    None,
                                    -1,
                                    iface_index,
                                    pos,
                                );
                            }
                        }
                        ValueType::Int
                        | ValueType::Int8
                        | ValueType::Int16
                        | ValueType::Int32
                        | ValueType::Int64
                        | ValueType::Uint
                        | ValueType::UintPtr
                        | ValueType::Uint8
                        | ValueType::Uint16
                        | ValueType::Uint32
                        | ValueType::Uint64
                        | ValueType::Float32
                        | ValueType::Float64
                        | ValueType::Complex64
                        | ValueType::Complex128
                        | ValueType::String
                        | ValueType::Slice
                        | ValueType::UnsafePtr
                        | ValueType::Pointer => {
                            let t_extra = match typ_to {
                                ValueType::String => (typ_from == ValueType::Slice).then(|| {
                                    self.tc_objs.types[tc_from].try_as_slice().unwrap().elem()
                                }),
                                ValueType::Slice => {
                                    Some(self.tc_objs.types[tc_to].try_as_slice().unwrap().elem())
                                }
                                ValueType::Pointer => {
                                    Some(self.tc_objs.types[tc_to].try_as_pointer().unwrap().base())
                                }
                                ValueType::Channel => Some(tc_to),
                                _ => None,
                            };
                            let t2 = t_extra.map(|x| self.t.tc_type_to_value_type(x));

                            current_func_emitter!(self).emit_cast(typ_to, typ_from, t2, -1, 0, pos);
                        }
                        ValueType::Channel => { /* nothing to be done */ }
                        _ => {
                            dbg!(typ_to);
                            unreachable!()
                        }
                    }
                }
            }
            // normal goscript function
            _ => {
                self.visit_expr(func_expr);
                current_func_emitter!(self).emit_pre_call(pos);
                let _ = params.iter().map(|e| self.visit_expr(e)).count();
                let t = self.t.expr_tc_type(func_expr);
                self.try_cast_params_to_iface(t, params, ellipsis);

                // do not pack params if there is ellipsis
                let ftc = self.t.underlying_tc(self.t.expr_tc_type(func_expr));
                let variadic_typ = if !ellipsis {
                    let meta = self.t.tc_type_to_meta(ftc, self.objects, self.dummy_gcv);
                    let metas = &self.objects.metas;
                    let call_meta = metas[meta.key].as_signature();
                    call_meta.variadic.map(|x| x.1.value_type(metas))
                } else {
                    None
                };
                current_func_emitter!(self).emit_call(style, variadic_typ, pos);
            }
        }
    }

    fn gen_type_assert(&mut self, expr: &Expr, typ: &Option<Expr>, comma_ok: bool) {
        self.visit_expr(expr);
        let t = self.t.expr_tc_type(typ.as_ref().unwrap());
        let meta = self.t.tc_type_to_meta(t, self.objects, self.dummy_gcv);
        let func = current_func_mut!(self);
        let index = func.add_const(None, GosValue::new_metadata(meta));
        let pos = expr.pos(self.ast_objs);
        func.emit_code_with_flag_imm(Opcode::TYPE_ASSERT, comma_ok, index.into(), Some(pos));
    }

    fn gen_index(&mut self, container: &Expr, index: &Expr, t_result: ValueType, comma_ok: bool) {
        let t1 = self.t.expr_value_type(index);
        self.visit_expr(container);
        let pos = Some(container.pos(&self.ast_objs));
        if let Some(const_val) = self.t.try_tc_const_value(index.id()) {
            let (ival, _) = const_val.to_int().int_as_i64();
            if let Ok(i) = OpIndex::try_from(ival) {
                current_func_emitter!(self).emit_load_index_imm(i, t_result, comma_ok, pos);
                return;
            }
        }
        self.visit_expr(index);
        current_func_emitter!(self).emit_load_index(t_result, t1, comma_ok, pos);
    }

    fn try_cast_to_iface(
        &mut self,
        lhs: Option<TCTypeKey>,
        rhs: TCTypeKey,
        rhs_index: OpIndex,
        pos: usize,
    ) -> ValueType {
        let rhs_type = self.t.tc_type_to_value_type(rhs);
        match lhs {
            Some(t0) => match self.t.obj_underlying_value_type(t0) == ValueType::Interface {
                true => {
                    let vt1 = self.t.obj_underlying_value_type(rhs);
                    match vt1 != ValueType::Interface && vt1 != ValueType::Void {
                        true => {
                            let index = self.iface_selector.get_index(
                                (t0, rhs),
                                &mut self.t,
                                self.objects,
                                self.dummy_gcv,
                            );
                            current_func_emitter!(self).emit_cast(
                                ValueType::Interface,
                                rhs_type,
                                None,
                                rhs_index,
                                index,
                                Some(pos),
                            );
                            ValueType::Interface
                        }
                        false => rhs_type,
                    }
                }
                false => rhs_type,
            },
            None => rhs_type,
        }
    }

    fn try_cast_params_to_iface(&mut self, func: TCTypeKey, params: &Vec<Expr>, ellipsis: bool) {
        let (sig_params, variadic) = self.t.sig_params_tc_types(func);
        let non_variadic_count = variadic.map_or(sig_params.len(), |_| sig_params.len() - 1);
        let param_types = self.get_exprs_final_types(params);

        for (i, v) in sig_params[..non_variadic_count].iter().enumerate() {
            let rhs_index = i as OpIndex - params.len() as OpIndex;
            self.try_cast_to_iface(Some(*v), param_types[i].0, rhs_index, param_types[i].1);
        }
        if !ellipsis {
            if let Some(t) = variadic {
                if self.t.obj_underlying_value_type(t) == ValueType::Interface {
                    for (i, p) in param_types.iter().enumerate().skip(non_variadic_count) {
                        let rhs_index = i as OpIndex - params.len() as OpIndex;
                        self.try_cast_to_iface(Some(t), p.0, rhs_index, p.1);
                    }
                }
            }
        }
    }

    fn get_exprs_final_types(&self, params: &Vec<Expr>) -> Vec<(TCTypeKey, usize)> {
        params.iter().fold(vec![], |mut init, e| {
            let pos = e.pos(&self.ast_objs);
            match e {
                Expr::Call(call) => {
                    let typ = self.t.node_tc_type(call.id());
                    match &self.tc_objs.types[typ] {
                        Type::Tuple(tuple) => init.extend(
                            tuple
                                .vars()
                                .iter()
                                .map(|o| (self.tc_objs.lobjs[*o].typ().unwrap(), pos))
                                .collect::<Vec<(TCTypeKey, usize)>>()
                                .iter(),
                        ),
                        _ => init.push((typ, pos)),
                    }
                }
                _ => init.push((self.t.expr_tc_type(e), pos)),
            };
            init
        })
    }

    fn visit_composite_expr(&mut self, expr: &Expr, tctype: TCTypeKey) {
        match expr {
            Expr::CompositeLit(clit) => self.gen_composite_literal(clit, tctype),
            _ => self.visit_expr(expr),
        }
        let t = self.t.expr_tc_type(expr);
        self.try_cast_to_iface(Some(tctype), t, -1, expr.pos(self.ast_objs));
    }

    fn gen_composite_literal(&mut self, clit: &CompositeLit, tctype: TCTypeKey) {
        let meta = self
            .t
            .tc_type_to_meta(tctype, &mut self.objects, self.dummy_gcv);
        let vt = self.t.tc_type_to_value_type(tctype);
        let pos = Some(clit.l_brace);
        let typ = &self.tc_objs.types[tctype].underlying_val(&self.tc_objs);
        let meta = meta.underlying(&self.objects.metas);
        let mtype = &self.objects.metas[meta.key].clone();
        match mtype {
            MetadataType::Slice(_) | MetadataType::Array(_, _) => {
                let elem = match typ {
                    Type::Array(detail) => detail.elem(),
                    Type::Slice(detail) => detail.elem(),
                    _ => unreachable!(),
                };
                for expr in clit.elts.iter().rev() {
                    match expr {
                        Expr::KeyValue(kv) => {
                            self.visit_composite_expr(&kv.val, elem);
                            // the key is a constant
                            let key_const = self.t.try_tc_const_value(kv.key.id()).unwrap();
                            let (key_i64, ok) = key_const.int_as_i64();
                            debug_assert!(ok);
                            current_func_emitter!(self).emit_push_imm(
                                ValueType::Int,
                                key_i64 as i32,
                                None,
                            );
                        }
                        _ => {
                            self.visit_composite_expr(expr, elem);
                            // -1 as a placeholder for when the index is missing
                            current_func_emitter!(self).emit_push_imm(ValueType::Int, -1, None);
                        }
                    };
                }
            }
            MetadataType::Map(_, _) => {
                let map_type = typ.try_as_map().unwrap();
                for expr in clit.elts.iter() {
                    match expr {
                        Expr::KeyValue(kv) => {
                            self.visit_composite_expr(&kv.val, map_type.elem());
                            self.visit_composite_expr(&kv.key, map_type.key());
                        }
                        _ => unreachable!(),
                    }
                }
            }
            MetadataType::Struct(f, _) => {
                let fields = typ.try_as_struct().unwrap().fields();
                for (i, expr) in clit.elts.iter().enumerate() {
                    let (expr, index) = match expr {
                        Expr::KeyValue(kv) => {
                            let ident = kv.key.try_as_ident().unwrap();
                            let index = f.index_by_name(&self.ast_objs.idents[*ident].name);
                            (&kv.val, index)
                        }
                        _ => (expr, i),
                    };
                    let field_type = self.tc_objs.lobjs[fields[index]].typ().unwrap();
                    self.visit_composite_expr(expr, field_type);
                    current_func_emitter!(self).emit_push_imm(
                        ValueType::Uint,
                        index as OpIndex,
                        pos,
                    );
                }
            }
            _ => {
                dbg!(&mtype);
                unreachable!()
            }
        }
        current_func_emitter!(self).emit_push_imm(
            ValueType::Int32,
            clit.elts.len() as OpIndex,
            pos,
        );

        let mut emitter = current_func_emitter!(self);
        let i = emitter.add_const(None, GosValue::new_metadata(meta));
        emitter.emit_literal(ValueType::Metadata, Some(vt), i.into(), pos);
    }

    fn gen_ref_expr(&mut self, expr: &Expr, whole_expr: Option<&Expr>) {
        let pos = Some(expr.pos(&self.ast_objs));
        match expr {
            Expr::Ident(ikey) => {
                let index = self.resolve_any_ident(ikey, None);
                match index {
                    EntIndex::LocalVar(_) => {
                        let meta = self.t.node_meta(expr.id(), self.objects, self.dummy_gcv);
                        let t = meta.value_type(&self.objects.metas);
                        let entity_key = use_ident_unique_key!(self, *ikey);
                        let func = current_func_mut!(self);
                        let ind = *func.entity_index(&entity_key).unwrap();
                        let desc =
                            ValueDesc::new(*self.func_stack.last().unwrap(), ind.into(), t, false);
                        if !func.is_ctor() {
                            let index = func.try_add_upvalue(&entity_key, desc);
                            func.emit_inst(
                                Opcode::REF_UPVALUE,
                                [Some(t), None, None],
                                Some(index.into()),
                                pos,
                            );
                        } else {
                            // for package ctors, all locals are "closed"
                            let mut emitter = current_func_emitter!(self);
                            emitter.emit_load(ind, None, t, pos);
                            emitter
                                .f
                                .emit_inst(Opcode::REF, [Some(t), None, None], None, pos);
                        }
                    }
                    EntIndex::UpValue(i) => {
                        let t = self.t.expr_value_type(expr);
                        let func = current_func_mut!(self);
                        func.emit_inst(Opcode::REF_UPVALUE, [Some(t), None, None], Some(i), pos);
                    }
                    EntIndex::PackageMember(pkg, ident) => {
                        let func = current_func_mut!(self);
                        func.emit_inst(Opcode::REF_PKG_MEMBER, [None, None, None], Some(0), pos);
                        func.emit_raw_inst(key_to_u64(self.pkg_key), pos);
                        let fkey = self.func_stack.last().unwrap();
                        let i = current_func!(self).next_code_index() - 2;
                        self.pkg_helper.add_pair(pkg, ident.into(), *fkey, i, false);
                    }
                    _ => unreachable!(),
                }
            }
            Expr::Index(iexpr) => {
                let (t0, t2) =
                    self.t
                        .sliceable_expr_value_types(&iexpr.expr, self.objects, self.dummy_gcv);
                let t1 = self.t.expr_value_type(&iexpr.index);
                self.visit_expr(&iexpr.expr);
                self.visit_expr(&iexpr.index);
                let pos = Some(iexpr.index.pos(&self.ast_objs));
                current_func_mut!(self).emit_inst(
                    Opcode::REF_SLICE_MEMBER,
                    [Some(t0), Some(t1), Some(t2)],
                    None,
                    pos,
                );
            }
            Expr::Selector(sexpr) => match self.t.try_pkg_key(&sexpr.expr) {
                Some(key) => {
                    let pkey = self.pkg_helper.get_runtime_key(key);
                    let func = current_func_mut!(self);
                    func.emit_inst(Opcode::REF_PKG_MEMBER, [None, None, None], Some(0), pos);
                    func.emit_raw_inst(key_to_u64(pkey), pos);
                    let fkey = self.func_stack.last().unwrap();
                    let i = current_func!(self).next_code_index() - 2;
                    self.pkg_helper.add_pair(pkey, sexpr.sel, *fkey, i, false);
                }
                None => {
                    self.visit_expr(&sexpr.expr);
                    let (t0, _, indices, _) = self.t.selection_vtypes_indices_sel_typ(sexpr.id());
                    current_func_emitter!(self).emit_struct_field_op(
                        Opcode::REF_STRUCT_FIELD,
                        indices,
                        t0,
                        pos,
                    );
                }
            },
            Expr::CompositeLit(clit) => {
                self.visit_expr_composit_lit(whole_expr.unwrap(), clit);
                let typ = self.t.expr_value_type(expr);
                current_func_mut!(self).emit_inst(Opcode::REF, [Some(typ), None, None], None, pos);
            }
            _ => {
                dbg!(&expr);
                unimplemented!()
            }
        }
    }

    fn gen_type_meta(&mut self, typ: &Expr) {
        let m = self.t.node_meta(typ.id(), self.objects, self.dummy_gcv);
        let mut emitter = current_func_emitter!(self);
        let pos = Some(typ.pos(&self.ast_objs));
        emitter.emit_load(EntIndex::TypeMeta(m), None, ValueType::Metadata, pos);
    }

    fn gen_const(&mut self, node: NodeId, pos: Option<Pos>) {
        let val = self.t.const_value(node);
        let mut emitter = current_func_emitter!(self);
        let t = val.typ();
        let i = emitter.add_const(None, val);
        emitter.emit_load(i, None, t, pos);
    }

    fn gen_load_field(
        &mut self,
        indices: &[usize],
        mdata: Meta,
        typ: ValueType,
        pos: Option<usize>,
    ) -> (Meta, ValueType) {
        if indices.len() > 0 {
            current_func_emitter!(self).emit_struct_field_op(
                Opcode::LOAD_STRUCT_FIELD,
                indices,
                typ,
                pos,
            );
            let field_meta = self.get_field_meta(&mdata, indices);
            let field_type = field_meta.value_type(&self.objects.metas);
            (field_meta, field_type)
        } else {
            (mdata, typ)
        }
    }

    fn get_field_meta(&self, parent: &Meta, indices: &[usize]) -> Meta {
        match parent.mtype_unwraped(&self.objects.metas) {
            MetadataType::Struct(f, _) => f.get(indices, &self.objects.metas).meta,
            _ => unreachable!(),
        }
    }

    fn current_func_add_const_def(
        &mut self,
        ikey: &IdentKey,
        cst: GosValue,
        typ: ValueType,
    ) -> EntIndex {
        let func = current_func_mut!(self);
        let index = func.add_const(Some(self.t.object_def(*ikey).data()), cst.clone());
        if func.is_ctor() {
            let pkg_key = func.package;
            drop(func);
            let pkg = &mut self.objects.packages[pkg_key];
            let ident = &self.ast_objs.idents[*ikey];
            pkg.add_member(ident.name.clone(), cst, typ);
        }
        index
    }

    fn add_pkg_var_member(&mut self, pkey: PackageKey, names: &Vec<IdentKey>) {
        for n in names.iter() {
            let ident = &self.ast_objs.idents[*n];
            let meta = self.t.obj_def_meta(*n, self.objects, self.dummy_gcv);
            let val = meta.zero(&self.objects.metas, self.dummy_gcv);
            self.objects.packages[pkey].add_member(
                ident.name.clone(),
                val,
                meta.value_type(&self.objects.metas),
            );
        }
    }

    fn swallow_value(&mut self, expr: &Expr) {
        let val_types = self.t.expr_value_types(expr);
        current_func_emitter!(self).emit_pop(&val_types, Some(expr.pos(&self.ast_objs)));
    }

    fn lhs_on_stack_value_types(lhs: &LeftHandSide) -> Vec<ValueType> {
        let mut on_stack_types = vec![];
        match lhs {
            LeftHandSide::Primitive(_) => {}
            LeftHandSide::IndexExpr(info) => {
                on_stack_types.push(info.t1);
                if let Some(t) = info.t2 {
                    on_stack_types.push(t);
                }
            }
            LeftHandSide::SelExpr(info) => {
                on_stack_types.push(info.t1);
            }
            LeftHandSide::Deref(_) => on_stack_types.push(ValueType::Pointer),
        }
        on_stack_types
    }

    pub fn gen_with_files(&mut self, files: &Vec<File>, tcpkg: TCPackageKey, index: OpIndex) {
        let pkey = self.pkg_key;
        let fmeta = self.objects.s_meta.default_sig;
        let f = GosValue::function_with_meta(
            pkey,
            fmeta,
            self.objects,
            self.dummy_gcv,
            FuncFlag::PkgCtor,
        );
        let fkey = *f.as_function();
        // the 0th member is the constructor
        self.objects.packages[pkey].add_member(
            String::new(),
            GosValue::new_closure_static(fkey, &self.objects.functions),
            ValueType::Closure,
        );
        self.pkg_key = pkey;
        self.func_stack.push(fkey);

        let (names, vars) = self.pkg_helper.sort_var_decls(files, self.t.type_info());
        self.add_pkg_var_member(pkey, &names);

        self.pkg_helper.gen_imports(tcpkg, current_func_mut!(self));

        for f in files.iter() {
            for d in f.decls.iter() {
                self.visit_decl(d)
            }
        }
        for v in vars.iter() {
            self.gen_def_var(v);
        }

        let mut emitter = Emitter::new(&mut self.objects.functions[fkey]);
        emitter.emit_return(Some(index), None);
        self.func_stack.pop();
    }
}

impl<'a> ExprVisitor for CodeGen<'a> {
    type Result = ();

    fn visit_expr(&mut self, expr: &Expr) {
        if let Some(mode) = self.t.try_expr_mode(expr) {
            if let OperandMode::Constant(_) = mode {
                self.gen_const(expr.id(), Some(expr.pos(&self.ast_objs)));
                return;
            }
        }
        walk_expr(self, expr);
    }

    fn visit_expr_ident(&mut self, expr: &Expr, ident: &IdentKey) {
        let index = self.resolve_any_ident(ident, Some(expr));
        let t = self.t.obj_use_value_type(*ident);
        let fkey = self.func_stack.last().unwrap();
        let p = Some(self.ast_objs.idents[*ident].pos);
        current_func_emitter!(self).emit_load(
            index,
            Some((self.pkg_helper.pairs_mut(), *fkey)),
            t,
            p,
        );
    }

    fn visit_expr_ellipsis(&mut self, _: &Expr, _els: &Option<Expr>) {
        unreachable!();
    }

    fn visit_expr_basic_lit(&mut self, this: &Expr, blit: &BasicLit) {
        self.gen_const(this.id(), Some(blit.pos));
    }

    /// Add function as a const and then generate a closure of it
    fn visit_expr_func_lit(&mut self, this: &Expr, flit: &FuncLit) {
        let tc_type = self.t.node_tc_type(this.id());
        let fkey = self.gen_func_def(tc_type, flit.typ, None, &flit.body);
        let mut emitter = current_func_emitter!(self);
        let i = emitter.add_const(None, GosValue::new_function(fkey));
        let pos = Some(flit.body.l_brace);
        emitter.emit_literal(ValueType::Function, None, i.into(), pos);
    }

    fn visit_expr_composit_lit(&mut self, _: &Expr, clit: &CompositeLit) {
        let tctype = self.t.expr_tc_type(clit.typ.as_ref().unwrap());
        self.gen_composite_literal(clit, tctype);
    }

    fn visit_expr_paren(&mut self, _: &Expr, expr: &Expr) {
        self.visit_expr(expr)
    }

    fn visit_expr_selector(&mut self, this: &Expr, expr: &Expr, ident: &IdentKey) {
        let pos = Some(expr.pos(&self.ast_objs));
        if let Some(key) = self.t.try_pkg_key(expr) {
            let pkg = self.pkg_helper.get_runtime_key(key);
            let t = self.t.obj_use_value_type(*ident);
            let fkey = self.func_stack.last().unwrap();
            current_func_emitter!(self).emit_load(
                EntIndex::PackageMember(pkg, (*ident).data()),
                Some((self.pkg_helper.pairs_mut(), *fkey)),
                t,
                pos,
            );
            return;
        }

        let lhs_meta = self.t.node_meta(expr.id(), self.objects, self.dummy_gcv);
        let lhs_type = lhs_meta.value_type(&self.objects.metas);
        let (_, _, indices, stype) = self.t.selection_vtypes_indices_sel_typ(this.id());
        let indices = indices.clone();
        match &stype {
            SelectionType::MethodNonPtrRecv | SelectionType::MethodPtrRecv => {
                let index_count = indices.len();
                let index = indices[index_count - 1] as OpIndex; // the final index
                let embedded_indices = Vec::from_iter(indices[..index_count - 1].iter().cloned());
                let lhs_has_embedded = index_count > 1;
                let final_lhs_meta = match lhs_has_embedded {
                    false => lhs_meta,
                    true => self.get_field_meta(&lhs_meta, &embedded_indices),
                };
                let final_lhs_type = final_lhs_meta.value_type(&self.objects.metas);
                if (final_lhs_type != ValueType::Pointer && final_lhs_type != ValueType::Interface)
                    && stype == SelectionType::MethodPtrRecv
                {
                    if !lhs_has_embedded {
                        self.gen_ref_expr(expr, None);
                    } else {
                        self.visit_expr(expr);
                        current_func_emitter!(self).emit_struct_field_op(
                            Opcode::REF_STRUCT_FIELD,
                            &embedded_indices,
                            lhs_type,
                            pos,
                        );
                    }
                } else {
                    self.visit_expr(expr);
                    if lhs_has_embedded {
                        self.gen_load_field(&embedded_indices, lhs_meta, lhs_type, pos);
                    }
                    if final_lhs_type == ValueType::Pointer
                        && stype == SelectionType::MethodNonPtrRecv
                    {
                        current_func_mut!(self).emit_code_with_type(Opcode::DEREF, lhs_type, pos);
                    }
                }

                if final_lhs_type == ValueType::Interface {
                    current_func_mut!(self).emit_code_with_type_imm(
                        Opcode::BIND_INTERFACE_METHOD,
                        final_lhs_type,
                        index,
                        pos,
                    );
                } else {
                    let func = current_func_mut!(self);
                    func.emit_code_with_type(Opcode::BIND_METHOD, final_lhs_type, pos);
                    let point = func.next_code_index();
                    func.emit_raw_inst(0, pos); // placeholder for FunctionKey
                    let fkey = *self.func_stack.last().unwrap();
                    self.call_helper
                        .add_call(fkey, point, final_lhs_meta, index);
                }
            }
            SelectionType::NonMethod => {
                self.visit_expr(expr);
                self.gen_load_field(&indices, lhs_meta, lhs_type, pos);
            }
        }
    }

    fn visit_expr_index(&mut self, e: &Expr, container: &Expr, index: &Expr) {
        let t = self.t.expr_value_type(e);
        self.gen_index(container, index, t, false);
    }

    fn visit_expr_slice(
        &mut self,
        _: &Expr,
        expr: &Expr,
        low: &Option<Expr>,
        high: &Option<Expr>,
        max: &Option<Expr>,
    ) -> Self::Result {
        self.visit_expr(expr);
        let (t0, t1) = self
            .t
            .sliceable_expr_value_types(expr, self.objects, self.dummy_gcv);
        let pos = Some(expr.pos(&self.ast_objs));
        match low {
            None => current_func_emitter!(self).emit_push_imm(ValueType::Int, 0, pos),
            Some(e) => self.visit_expr(e),
        }
        match high {
            None => current_func_emitter!(self).emit_push_imm(ValueType::Int, -1, pos),
            Some(e) => self.visit_expr(e),
        }
        match max {
            None => current_func_mut!(self).emit_code_with_type2(Opcode::SLICE, t0, Some(t1), pos),
            Some(e) => {
                self.visit_expr(e);
                current_func_mut!(self).emit_code_with_type2(Opcode::SLICE_FULL, t0, Some(t1), pos);
            }
        }
    }

    fn visit_expr_type_assert(&mut self, _: &Expr, expr: &Expr, typ: &Option<Expr>) {
        self.gen_type_assert(expr, typ, false);
    }

    fn visit_expr_call(&mut self, _: &Expr, func_expr: &Expr, params: &Vec<Expr>, ellipsis: bool) {
        self.gen_call(func_expr, params, ellipsis, CallStyle::Default);
    }

    fn visit_expr_star(&mut self, _: &Expr, expr: &Expr) {
        let pos = Some(expr.pos(&self.ast_objs));
        match self.t.expr_mode(expr) {
            OperandMode::TypeExpr => {
                let m = self
                    .t
                    .tc_type_to_meta(self.t.expr_tc_type(expr), self.objects, self.dummy_gcv)
                    .ptr_to();
                let mut emitter = current_func_emitter!(self);
                emitter.emit_load(EntIndex::TypeMeta(m), None, ValueType::Metadata, pos);
            }
            _ => {
                self.visit_expr(expr);
                let t = self.t.expr_value_type(expr);
                current_func_mut!(self).emit_code_with_type(Opcode::DEREF, t, pos);
            }
        }
    }

    fn visit_expr_unary(&mut self, this: &Expr, expr: &Expr, op: &Token) {
        if op == &Token::AND {
            self.gen_ref_expr(expr, Some(this));
            return;
        }

        self.visit_expr(expr);
        let code = match op {
            Token::ADD => Opcode::UNARY_ADD,
            Token::SUB => Opcode::UNARY_SUB,
            Token::XOR => Opcode::UNARY_XOR,
            Token::NOT => Opcode::NOT,
            Token::ARROW => Opcode::RECV,
            _ => {
                dbg!(op);
                unreachable!()
            }
        };
        let t = self.t.expr_value_type(expr);
        let pos = Some(expr.pos(&self.ast_objs));
        current_func_mut!(self).emit_code_with_type(code, t, pos);
    }

    fn visit_expr_binary(&mut self, _: &Expr, left: &Expr, op: &Token, right: &Expr) {
        self.visit_expr(left);
        let t = self.t.expr_value_type(left);
        let code = match op {
            Token::ADD => Opcode::ADD,
            Token::SUB => Opcode::SUB,
            Token::MUL => Opcode::MUL,
            Token::QUO => Opcode::QUO,
            Token::REM => Opcode::REM,
            Token::AND => Opcode::AND,
            Token::OR => Opcode::OR,
            Token::XOR => Opcode::XOR,
            Token::SHL => Opcode::SHL,
            Token::SHR => Opcode::SHR,
            Token::AND_NOT => Opcode::AND_NOT,
            Token::LAND => Opcode::SHORT_CIRCUIT_AND,
            Token::LOR => Opcode::SHORT_CIRCUIT_OR,
            Token::EQL => Opcode::EQL,
            Token::LSS => Opcode::LSS,
            Token::GTR => Opcode::GTR,
            Token::NEQ => Opcode::NEQ,
            Token::LEQ => Opcode::LEQ,
            Token::GEQ => Opcode::GEQ,
            _ => unreachable!(),
        };
        let pos = Some(left.pos(&self.ast_objs));
        // handles short circuit
        let mark = match code {
            Opcode::SHORT_CIRCUIT_AND | Opcode::SHORT_CIRCUIT_OR => {
                let emitter = current_func_emitter!(self);
                emitter.f.emit_code(code, pos);
                Some(emitter.f.next_code_index() - 1)
            }
            _ => None,
        };
        self.visit_expr(right);

        if let Some(i) = mark {
            let emitter = current_func_emitter!(self);
            let diff = emitter.f.next_code_index() - i - 1;
            current_func_emitter!(self)
                .f
                .instruction_mut(i)
                .set_imm(diff as OpIndex);
        } else {
            let t1 = match code {
                Opcode::SHL | Opcode::SHR | Opcode::EQL => Some(self.t.expr_value_type(right)),
                _ => None,
            };
            current_func_mut!(self).emit_code_with_type2(code, t, t1, pos);
        }
    }

    fn visit_expr_key_value(&mut self, _e: &Expr, _key: &Expr, _val: &Expr) {
        unreachable!();
    }

    fn visit_expr_array_type(&mut self, this: &Expr, _: &Option<Expr>, _: &Expr) {
        self.gen_type_meta(this)
    }

    fn visit_expr_struct_type(&mut self, this: &Expr, _s: &StructType) {
        self.gen_type_meta(this)
    }

    fn visit_expr_func_type(&mut self, this: &Expr, _s: &FuncTypeKey) {
        self.gen_type_meta(this)
    }

    fn visit_expr_interface_type(&mut self, this: &Expr, _s: &InterfaceType) {
        self.gen_type_meta(this)
    }

    fn visit_map_type(&mut self, this: &Expr, _: &Expr, _: &Expr, _map: &Expr) {
        self.gen_type_meta(this)
    }

    fn visit_chan_type(&mut self, this: &Expr, _chan: &Expr, _dir: &ChanDir) {
        self.gen_type_meta(this)
    }

    fn visit_bad_expr(&mut self, _: &Expr, _e: &BadExpr) {
        unreachable!();
    }
}

impl<'a> StmtVisitor for CodeGen<'a> {
    type Result = ();

    fn visit_stmt(&mut self, stmt: &Stmt) {
        walk_stmt(self, stmt)
    }

    fn visit_decl(&mut self, decl: &Decl) {
        walk_decl(self, decl)
    }

    fn visit_stmt_decl_gen(&mut self, gdecl: &GenDecl) {
        for s in gdecl.specs.iter() {
            let spec = &self.ast_objs.specs[*s];
            match spec {
                Spec::Import(_) => {
                    //handled elsewhere
                }
                Spec::Type(ts) => {
                    let m = self.t.obj_def_meta(ts.name, self.objects, self.dummy_gcv);
                    self.current_func_add_const_def(
                        &ts.name,
                        GosValue::new_metadata(m),
                        ValueType::Metadata,
                    );
                }
                Spec::Value(vs) => match &gdecl.token {
                    Token::VAR => {
                        // package level vars are handled elsewhere due to ordering
                        if !current_func!(self).is_ctor() {
                            self.gen_def_var(vs);
                        }
                    }
                    Token::CONST => self.gen_def_const(&vs.names),
                    _ => unreachable!(),
                },
            }
        }
    }

    fn visit_stmt_decl_func(&mut self, fdecl: &FuncDeclKey) -> Self::Result {
        let decl = &self.ast_objs.fdecls[*fdecl];
        if decl.body.is_none() {
            return;
            // unimplemented!()
        }
        let tc_type = self.t.obj_def_tc_type(decl.name);
        let stmt = decl.body.as_ref().unwrap();
        let fkey = self.gen_func_def(tc_type, decl.typ, decl.recv.clone(), stmt);
        let cls = GosValue::new_closure_static(fkey, &self.objects.functions);
        // this is a struct method
        if let Some(self_ident) = &decl.recv {
            let field = &self.ast_objs.fields[self_ident.list[0]];
            let name = &self.ast_objs.idents[decl.name].name;
            let meta = self
                .t
                .node_meta(field.typ.id(), self.objects, self.dummy_gcv);
            meta.set_method_code(name, fkey, &mut self.objects.metas);
        } else {
            let name = &self.ast_objs.idents[decl.name].name;
            let pkg = &mut self.objects.packages[self.pkg_key];
            match name.as_str() {
                "init" => pkg.add_init_func(cls),
                _ => {
                    pkg.add_member(name.clone(), cls, ValueType::Closure);
                }
            };
        }
    }

    fn visit_stmt_labeled(&mut self, lstmt: &LabeledStmtKey) {
        let stmt = &self.ast_objs.l_stmts[*lstmt];
        let offset = current_func!(self).code().len();
        let entity = self.t.object_def(stmt.label).data();
        let is_breakable = match &stmt.stmt {
            Stmt::For(_) | Stmt::Range(_) | Stmt::Select(_) | Stmt::Switch(_) => true,
            _ => false,
        };
        self.branch_helper.add_label(entity, offset, is_breakable);
        self.visit_stmt(&stmt.stmt);
    }

    fn visit_stmt_send(&mut self, sstmt: &SendStmt) {
        self.visit_expr(&sstmt.chan);
        self.visit_expr(&sstmt.val);
        let t = self.t.expr_value_type(&sstmt.val);
        current_func_mut!(self).emit_code_with_type(Opcode::SEND, t, Some(sstmt.arrow));
    }

    fn visit_stmt_incdec(&mut self, idcstmt: &IncDecStmt) {
        self.gen_assign(&idcstmt.token, &vec![&idcstmt.expr], RightHandSide::Nothing);
    }

    fn visit_stmt_assign(&mut self, astmt: &AssignStmtKey) {
        let stmt = &self.ast_objs.a_stmts[*astmt];
        self.gen_assign(
            &stmt.token,
            &stmt.lhs.iter().map(|x| x).collect(),
            RightHandSide::Values(&stmt.rhs),
        );
    }

    fn visit_stmt_go(&mut self, gostmt: &GoStmt) {
        match &gostmt.call {
            Expr::Call(call) => {
                self.gen_call(
                    &call.func,
                    &call.args,
                    call.ellipsis.is_some(),
                    CallStyle::Async,
                );
            }
            _ => unreachable!(),
        }
    }

    fn visit_stmt_defer(&mut self, dstmt: &DeferStmt) {
        current_func_mut!(self).flag = FuncFlag::HasDefer;
        match &dstmt.call {
            Expr::Call(call) => {
                self.gen_call(
                    &call.func,
                    &call.args,
                    call.ellipsis.is_some(),
                    CallStyle::Defer,
                );
            }
            _ => unreachable!(),
        }
    }

    fn visit_stmt_return(&mut self, rstmt: &ReturnStmt) {
        if !rstmt.results.is_empty() {
            for expr in rstmt.results.iter() {
                self.visit_expr(expr);
            }

            let return_types = self.get_exprs_final_types(&rstmt.results);
            let types = self
                .t
                .sig_returns_tc_types(*self.func_t_stack.last().unwrap());
            assert_eq!(return_types.len(), types.len());
            let count: OpIndex = return_types.len() as OpIndex;
            let types: Vec<ValueType> = return_types
                .iter()
                .enumerate()
                .map(|(i, typ)| {
                    let index = i as i32 - count;
                    let pos = typ.1;
                    let t = self.try_cast_to_iface(Some(types[i]), typ.0, index, pos);
                    let mut emitter = current_func_emitter!(self);
                    emitter.emit_store(
                        &LeftHandSide::Primitive(EntIndex::LocalVar(i as OpIndex)),
                        index,
                        None,
                        None,
                        t,
                        Some(pos),
                    );
                    t
                })
                .collect();
            current_func_emitter!(self).emit_pop(&types, Some(rstmt.ret));
        }
        current_func_emitter!(self).emit_return(None, Some(rstmt.ret));
    }

    fn visit_stmt_branch(&mut self, bstmt: &BranchStmt) {
        match bstmt.token {
            Token::BREAK | Token::CONTINUE => {
                let entity = bstmt.label.map(|x| use_ident_unique_key!(self, x));
                self.branch_helper.add_point(
                    current_func_mut!(self),
                    bstmt.token.clone(),
                    entity,
                    bstmt.token_pos,
                );
            }
            Token::GOTO => {
                let fkey = self.func_stack.last().unwrap();
                let label = bstmt.label.unwrap();
                let entity = use_ident_unique_key!(self, label);
                self.branch_helper.go_to(
                    &mut self.objects.functions,
                    *fkey,
                    entity,
                    bstmt.token_pos,
                );
            }
            Token::FALLTHROUGH => {
                // handled in gen_switch_body
            }
            _ => unreachable!(),
        }
    }

    fn visit_stmt_block(&mut self, bstmt: &BlockStmt) {
        for stmt in bstmt.list.iter() {
            self.visit_stmt(stmt);
        }
    }

    fn visit_stmt_if(&mut self, ifstmt: &IfStmt) {
        if let Some(init) = &ifstmt.init {
            self.visit_stmt(init);
        }
        self.visit_expr(&ifstmt.cond);
        let func = current_func_mut!(self);
        func.emit_code(Opcode::JUMP_IF_NOT, Some(ifstmt.if_pos));
        let top_marker = func.next_code_index();

        drop(func);
        self.visit_stmt_block(&ifstmt.body);
        let marker_if_arm_end = if ifstmt.els.is_some() {
            let func = current_func_mut!(self);
            // imm to be set later
            func.emit_code(Opcode::JUMP, Some(ifstmt.if_pos));
            Some(func.next_code_index())
        } else {
            None
        };

        // set the correct else jump target
        let func = current_func_mut!(self);
        let offset = func.offset(top_marker);
        func.instruction_mut(top_marker - 1).set_imm(offset);

        if let Some(els) = &ifstmt.els {
            self.visit_stmt(els);
            // set the correct if_arm_end jump target
            let func = current_func_mut!(self);
            let marker = marker_if_arm_end.unwrap();
            let offset = func.offset(marker);
            func.instruction_mut(marker - 1).set_imm(offset);
        }
    }

    fn visit_stmt_case(&mut self, _cclause: &CaseClause) {
        unreachable!(); // handled at upper level of the tree
    }

    fn visit_stmt_switch(&mut self, sstmt: &SwitchStmt) {
        self.branch_helper.enter_block(false);

        if let Some(init) = &sstmt.init {
            self.visit_stmt(init);
        }
        let tag_type = match &sstmt.tag {
            Some(e) => {
                self.visit_expr(e);
                self.t.expr_value_type(e)
            }
            None => {
                current_func_mut!(self).emit_code(Opcode::PUSH_TRUE, None);
                ValueType::Bool
            }
        };

        self.gen_switch_body(&*sstmt.body, tag_type);

        self.branch_helper
            .leave_block(current_func_mut!(self), None);
    }

    fn visit_stmt_type_switch(&mut self, tstmt: &TypeSwitchStmt) {
        if let Some(init) = &tstmt.init {
            self.visit_stmt(init);
        }

        let (ident_expr, assert) = match &tstmt.assign {
            Stmt::Assign(ass_key) => {
                let ass = &self.ast_objs.a_stmts[*ass_key];
                (Some(&ass.lhs[0]), &ass.rhs[0])
            }
            Stmt::Expr(e) => (None, &**e),
            _ => unreachable!(),
        };
        let (v, pos) = match assert {
            Expr::TypeAssert(ta) => (&ta.expr, Some(ta.l_paren)),
            _ => unreachable!(),
        };

        if let Some(_) = ident_expr {
            let inst_data: Vec<(ValueType, OpIndex)> = tstmt
                .body
                .list
                .iter()
                .map(|stmt| {
                    let tc_obj = self.t.object_implicit(&stmt.id());
                    let (index, _, meta) = self.add_local_var(tc_obj);
                    (meta.value_type(&self.objects.metas), index.into())
                })
                .collect();
            self.visit_expr(v);
            let func = current_func_mut!(self);
            func.emit_code_with_imm(Opcode::TYPE, inst_data.len() as OpIndex, pos);
            for data in inst_data.into_iter() {
                func.emit_inst(Opcode::VOID, [Some(data.0), None, None], Some(data.1), pos)
            }
        } else {
            self.visit_expr(v);
            current_func_mut!(self).emit_code(Opcode::TYPE, pos);
        }

        self.gen_switch_body(&*tstmt.body, ValueType::Metadata);
    }

    fn visit_stmt_comm(&mut self, _cclause: &CommClause) {
        unimplemented!();
    }

    fn visit_stmt_select(&mut self, sstmt: &SelectStmt) {
        /*
        Execution of a "select" statement proceeds in several steps:

        1. For all the cases in the statement, the channel operands of receive operations
        and the channel and right-hand-side expressions of send statements are evaluated
        exactly once, in source order, upon entering the "select" statement. The result
        is a set of channels to receive from or send to, and the corresponding values to
        send. Any side effects in that evaluation will occur irrespective of which (if any)
        communication operation is selected to proceed. Expressions on the left-hand side
        of a RecvStmt with a short variable declaration or assignment are not yet evaluated.
        2. If one or more of the communications can proceed, a single one that can proceed
        is chosen via a uniform pseudo-random selection. Otherwise, if there is a default
        case, that case is chosen. If there is no default case, the "select" statement
        blocks until at least one of the communications can proceed.
        3. Unless the selected case is the default case, the respective communication operation
        is executed.
        4. If the selected case is a RecvStmt with a short variable declaration or an assignment,
        the left-hand side expressions are evaluated and the received value (or values)
        are assigned.
        5. The statement list of the selected case is executed.

        Since communication on nil channels can never proceed, a select with only nil
        channels and no default case blocks forever.
        */
        self.branch_helper.enter_block(false);

        let mut helper = SelectHelper::new();
        let comms: Vec<&CommClause> = sstmt
            .body
            .list
            .iter()
            .map(|s| SelectHelper::to_comm_clause(s))
            .collect();
        for c in comms.iter() {
            let (typ, pos) = match &c.comm {
                Some(comm) => match comm {
                    Stmt::Send(send_stmt) => {
                        self.visit_expr(&send_stmt.chan);
                        self.visit_expr(&send_stmt.val);
                        let t = self.t.expr_value_type(&send_stmt.val);
                        (CommType::Send(t), send_stmt.arrow)
                    }
                    Stmt::Assign(ass_key) => {
                        let ass = &self.ast_objs.a_stmts[*ass_key];
                        let (e, pos) = SelectHelper::unwrap_recv(&ass.rhs[0]);
                        self.visit_expr(e);
                        let t = match &ass.lhs.len() {
                            1 => CommType::Recv(&ass),
                            2 => CommType::RecvCommaOk(&ass),
                            _ => unreachable!(),
                        };
                        (t, pos)
                    }
                    Stmt::Expr(expr_stmt) => {
                        let (e, pos) = SelectHelper::unwrap_recv(expr_stmt);
                        self.visit_expr(e);
                        (CommType::RecvNoLhs, pos)
                    }
                    _ => unreachable!(),
                },
                None => (CommType::Default, c.colon),
            };
            helper.add_comm(typ, pos);
        }

        helper.emit_select(current_func_mut!(self));

        let last_index = comms.len() - 1;
        for (i, c) in comms.iter().enumerate() {
            let begin = current_func!(self).next_code_index();

            match helper.comm_type(i) {
                CommType::Recv(ass) | CommType::RecvCommaOk(ass) => {
                    self.gen_assign(
                        &ass.token,
                        &ass.lhs.iter().map(|x| x).collect(),
                        RightHandSide::SelectRecv(&ass.rhs[0]),
                    );
                }
                _ => {}
            }

            for stmt in c.body.iter() {
                self.visit_stmt(stmt);
            }
            let func = current_func_mut!(self);
            let mut end = func.next_code_index();
            // the last block doesn't jump
            if i < last_index {
                func.emit_code(Opcode::JUMP, None);
            } else {
                end -= 1;
            }

            helper.set_block_begin_end(i, begin, end);
        }

        helper.patch_select(current_func_mut!(self));

        self.branch_helper
            .leave_block(current_func_mut!(self), None);
    }

    fn visit_stmt_for(&mut self, fstmt: &ForStmt) {
        self.branch_helper.enter_block(true);

        if let Some(init) = &fstmt.init {
            self.visit_stmt(init);
        }
        let top_marker = current_func!(self).next_code_index();
        let out_marker = if let Some(cond) = &fstmt.cond {
            self.visit_expr(&cond);
            let func = current_func_mut!(self);
            func.emit_code(Opcode::JUMP_IF_NOT, Some(fstmt.for_pos));
            Some(func.next_code_index())
        } else {
            None
        };
        self.visit_stmt_block(&fstmt.body);
        let continue_marker = if let Some(post) = &fstmt.post {
            // "continue" jumps to post statements
            let m = current_func!(self).next_code_index();
            self.visit_stmt(post);
            m
        } else {
            // "continue" jumps to top directly if no post statements
            top_marker
        };

        // jump to the top
        let func = current_func_mut!(self);
        let offset = -func.offset(top_marker) - 1;
        func.emit_code_with_imm(Opcode::JUMP, offset, Some(fstmt.for_pos));

        // set the correct else jump out target
        if let Some(m) = out_marker {
            let func = current_func_mut!(self);
            let offset = func.offset(m);
            func.instruction_mut(m - 1).set_imm(offset);
        }

        self.branch_helper
            .leave_block(current_func_mut!(self), Some(continue_marker));
    }

    fn visit_stmt_range(&mut self, rstmt: &RangeStmt) {
        self.branch_helper.enter_block(true);

        let blank = Expr::Ident(self.blank_ident);
        let lhs = vec![
            rstmt.key.as_ref().unwrap_or(&blank),
            rstmt.val.as_ref().unwrap_or(&blank),
        ];
        let marker = self
            .gen_assign(&rstmt.token, &lhs, RightHandSide::Range(&rstmt.expr))
            .unwrap();

        self.visit_stmt_block(&rstmt.body);
        // jump to the top
        let func = current_func_mut!(self);
        let offset = -func.offset(marker) - 1;
        // tell Opcode::RANGE where to jump after it's done
        let end_offset = func.offset(marker);
        func.instruction_mut(marker).set_imm(end_offset);
        func.emit_code_with_imm(Opcode::JUMP, offset, Some(rstmt.token_pos));

        self.branch_helper
            .leave_block(current_func_mut!(self), Some(marker));
    }

    fn visit_expr_stmt(&mut self, e: &Expr) {
        self.visit_expr(e);
        self.swallow_value(e);
    }

    fn visit_empty_stmt(&mut self, _e: &EmptyStmt) {}

    fn visit_bad_stmt(&mut self, _b: &BadStmt) {
        unreachable!();
    }

    fn visit_bad_decl(&mut self, _b: &BadDecl) {
        unreachable!();
    }
}
