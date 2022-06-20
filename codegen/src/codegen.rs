// Copyright 2022 The Goscript Authors. All rights reserved.
// Use of this source code is governed by a BSD-style
// license that can be found in the LICENSE file.

use super::branch::*;
use super::consts::*;
use super::package::PkgHelper;
use super::selector::*;
use super::types::{SelectionType, TypeCache, TypeLookup};
use crate::context::*;
use std::iter::FromIterator;

use goscript_vm::gc::GcoVec;
use goscript_vm::instruction::*;
use goscript_vm::metadata::*;
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

macro_rules! func_ctx {
    ($gen:ident) => {
        $gen.func_ctx_stack.last_mut().unwrap()
    };
}

macro_rules! expr_ctx {
    ($gen:ident) => {
        $gen.expr_ctx_stack.last_mut().unwrap()
    };
}

/// CodeGen implements the code generation logic.
pub struct CodeGen<'a, 'c> {
    objects: &'a mut VMObjects,
    consts: &'c Consts,
    ast_objs: &'a AstObjects,
    tc_objs: &'a TCObjects,
    dummy_gcv: &'a mut GcoVec,
    t: TypeLookup<'a>,
    iface_selector: &'a mut IfaceSelector,
    struct_selector: &'a mut StructSelector,
    branch_helper: &'a mut BranchHelper,
    pkg_helper: &'a mut PkgHelper<'a>,

    pkg_key: PackageKey,
    blank_ident: IdentKey,
    func_ctx_stack: Vec<FuncCtx<'c>>,
    expr_ctx_stack: Vec<ExprCtx>,
    results: Vec<FuncCtx<'c>>,
}

impl<'a, 'c> CodeGen<'a, 'c> {
    pub fn new(
        objects: &'a mut VMObjects,
        consts: &'c Consts,
        ast_objs: &'a AstObjects,
        tc_objs: &'a TCObjects,
        dummy_gcv: &'a mut GcoVec,
        ti: &'a TypeInfo,
        type_cache: &'a mut TypeCache,
        iface_selector: &'a mut IfaceSelector,
        struct_selector: &'a mut StructSelector,
        branch_helper: &'a mut BranchHelper,
        pkg_helper: &'a mut PkgHelper<'a>,
        pkg_key: PackageKey,
        blank_ident: IdentKey,
    ) -> CodeGen<'a, 'c> {
        CodeGen {
            objects,
            consts,
            ast_objs,
            tc_objs,
            dummy_gcv,
            t: TypeLookup::new(tc_objs, ti, type_cache),
            iface_selector,
            struct_selector,
            branch_helper,
            pkg_helper,
            pkg_key,
            blank_ident,
            func_ctx_stack: vec![],
            expr_ctx_stack: vec![],
            results: vec![],
        }
    }

    pub fn pkg_helper(&mut self) -> &mut PkgHelper<'a> {
        &mut self.pkg_helper
    }

    fn resolve_any_ident(&mut self, ident: &IdentKey, expr: Option<&Expr>) -> VirtualAddr {
        let mode = expr.map_or(&OperandMode::Value, |x| self.t.expr_mode(x));
        match mode {
            OperandMode::TypeExpr => {
                let tctype = self.t.underlying_tc(self.t.obj_use_tc_type(*ident));
                match self.t.basic_type_meta(tctype, self.objects) {
                    Some(meta) => VirtualAddr::Direct(func_ctx!(self).add_metadata(meta)),
                    None => {
                        let id = &self.ast_objs.idents[*ident];
                        if id.name == "error" {
                            let m = self.t.tc_type_to_meta(tctype, self.objects, self.dummy_gcv);
                            VirtualAddr::Direct(func_ctx!(self).add_metadata(m))
                        } else {
                            self.resolve_var_ident(ident)
                        }
                    }
                }
            }
            OperandMode::Value => {
                let id = &self.ast_objs.idents[*ident];
                match &*id.name {
                    "true" => {
                        VirtualAddr::Direct(func_ctx!(self).add_const(GosValue::new_bool(true)))
                    }
                    "false" => {
                        VirtualAddr::Direct(func_ctx!(self).add_const(GosValue::new_bool(false)))
                    }
                    "nil" => VirtualAddr::ZeroValue,
                    _ => self.resolve_var_ident(ident),
                }
            }
            _ => self.resolve_var_ident(ident),
        }
    }

    fn resolve_var_ident(&mut self, ident: &IdentKey) -> VirtualAddr {
        let okey = self.t.object_use(*ident);
        // 1. try local first
        if let Some(index) = func_ctx!(self).entity_index(&okey).map(|x| *x) {
            return VirtualAddr::Direct(index);
        }
        // 2. try upvalue
        let upvalue = self
            .func_ctx_stack
            .iter()
            .skip(1) // skip package constructor
            .rev()
            .skip(1) // skip itself
            .find_map(|ctx| {
                let index = ctx.entity_index(&okey).map(|x| *x);
                if let Some(ind) = index {
                    let desc = ValueDesc::new(
                        ctx.f_key,
                        ind.as_var_index() as OpIndex,
                        self.t.obj_use_value_type(*ident),
                        true,
                    );
                    Some(desc)
                } else {
                    None
                }
            });
        if let Some(uv) = upvalue {
            let ctx = func_ctx!(self);
            let index = ctx.add_upvalue(&okey, uv);
            return index;
        }
        // 3. must be package member
        self.pkg_helper
            .get_member_index(func_ctx!(self), okey, *ident)
    }

    fn add_local_or_resolve_ident(
        &mut self,
        ikey: &IdentKey,
        is_def: bool,
    ) -> (VirtualAddr, Option<TCTypeKey>, usize) {
        let ident = &self.ast_objs.idents[*ikey];
        let pos = ident.pos;
        if ident.is_blank() {
            return (VirtualAddr::Blank, None, pos);
        }
        if is_def {
            let tc_obj = self.t.object_def(*ikey);
            let (index, tc_type, _) = self.add_local_var(tc_obj);
            let ctx = func_ctx!(self);
            if ctx.is_ctor(&self.objects.functions) {
                let pkg_key = self.objects.functions[ctx.f_key].package;
                let pkg = &mut self.objects.packages[pkg_key];
                pkg.add_var_mapping(ident.name.clone(), index.as_var_index() as OpIndex);
            }
            (VirtualAddr::Direct(index), Some(tc_type), pos)
        } else {
            let index = self.resolve_var_ident(ikey);
            let t = self.t.obj_use_tc_type(*ikey);
            (index, Some(t), pos)
        }
    }

    fn add_local_var(&mut self, okey: TCObjKey) -> (Addr, TCTypeKey, Meta) {
        let tc_type = self.t.obj_tc_type(okey);
        let meta = self
            .t
            .tc_type_to_meta(tc_type, self.objects, self.dummy_gcv);
        let zero_val = meta.zero(&self.objects.metas, self.dummy_gcv);
        let ctx = func_ctx!(self);
        let index = ctx.add_local(Some(okey), Some(zero_val));
        (index, tc_type, meta)
    }

    fn gen_def_var(&mut self, vs: &ValueSpec) {
        let lhs = vs
            .names
            .iter()
            .map(|n| -> (VirtualAddr, Option<TCTypeKey>, usize) {
                let (vaddr, t, pos) = self.add_local_or_resolve_ident(n, true);
                (vaddr, t, pos)
            })
            .collect::<Vec<(VirtualAddr, Option<TCTypeKey>, usize)>>();
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
            self.add_const_def(name, val, typ);
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
            .map(|expr| match expr {
                Expr::Ident(ident) => {
                    let is_def = self.t.ident_is_def(ident);
                    let (vaddr, typ, pos) = self.add_local_or_resolve_ident(ident, is_def);
                    (vaddr, typ, pos)
                }
                Expr::Index(ind_expr) => {
                    let obj = &ind_expr.as_ref().expr;
                    let obj_addr = self.load(|g| g.gen_expr(obj));
                    let ind = &ind_expr.as_ref().index;
                    let ind_addr = self.load(|g| g.gen_expr(ind));
                    let obj_typ = self.t.expr_value_type(obj);
                    let typ = self.t.expr_tc_type(expr);
                    let pos = ind_expr.as_ref().l_brack;
                    let va = match obj_typ {
                        ValueType::Array => VirtualAddr::ArrayEntry(obj_addr, ind_addr),
                        ValueType::Slice => VirtualAddr::SliceEntry(obj_addr, ind_addr),
                        ValueType::Map => {
                            let zero_addr = self.add_zero_val(typ);
                            VirtualAddr::MapEntry(obj_addr, ind_addr, zero_addr)
                        }
                        _ => unreachable!(),
                    };
                    (va, Some(typ), pos)
                }
                Expr::Selector(sexpr) => {
                    let typ = Some(self.t.expr_tc_type(expr));
                    let pos = self.ast_objs.idents[sexpr.sel].pos;
                    match self.t.try_pkg_key(&sexpr.expr) {
                        Some(key) => {
                            let pkg = self.pkg_helper.get_runtime_key(key);
                            let pkg_addr = func_ctx!(self).add_const(GosValue::new_package(pkg));
                            let index_addr = Addr::PkgMemberIndex(pkg, sexpr.sel);
                            (VirtualAddr::PackageMember(pkg_addr, index_addr), typ, pos)
                        }
                        None => {
                            let struct_addr = self.load(|g| g.gen_expr(&sexpr.expr));
                            let t = self
                                .t
                                .node_meta(sexpr.expr.id(), self.objects, self.dummy_gcv);
                            let name = &self.ast_objs.idents[sexpr.sel].name;
                            let indices: Vec<OpIndex> = t
                                .field_indices(name, &self.objects.metas)
                                .iter()
                                .map(|x| *x as OpIndex)
                                .collect();
                            let (_, index) = self.get_struct_field_op_index(indices, Opcode::VOID);
                            (
                                VirtualAddr::StructMember(struct_addr, Addr::Imm(index)),
                                typ,
                                pos,
                            )
                        }
                    }
                }
                Expr::Star(sexpr) => {
                    let typ = Some(self.t.expr_tc_type(expr));
                    let pos = sexpr.star;
                    let addr = self.load(|g| g.gen_expr(&sexpr.expr));
                    (VirtualAddr::Pointee(addr), typ, pos)
                }
                _ => unreachable!(),
            })
            .collect::<Vec<(VirtualAddr, Option<TCTypeKey>, usize)>>();

        match rhs {
            RightHandSide::Nothing => {
                let code = match token {
                    Token::INC => Opcode::INC,
                    Token::DEC => Opcode::DEC,
                    _ => unreachable!(),
                };
                let typ = self.t.expr_value_type(&lhs_exprs[0]);
                self.gen_op_assign(&lhs[0].0, code, typ, None, None, lhs[0].2);
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
                    self.gen_op_assign(&lhs[0].0, code, ltyp, Some(&rhs_exprs[0]), rtyp, lhs[0].2);
                    None
                } else {
                    self.gen_assign_def_var(&lhs, &None, &rhs)
                }
            }
            _ => self.gen_assign_def_var(&lhs, &None, &rhs),
        }
    }

    fn gen_op_assign(
        &mut self,
        left: &VirtualAddr,
        opcode: Opcode,
        typ: ValueType,
        right: Option<&Expr>,
        r_type: Option<ValueType>,
        p: usize,
    ) {
        let pos = Some(p);
        let rhs_addr = match right {
            Some(e) => self.load(|g| g.gen_expr(e)),
            // inc/dec
            None => Addr::Void,
        };
        func_ctx!(self).emit_assign(left.clone(), rhs_addr, Some((opcode, typ, r_type)), pos);
    }

    fn gen_assign_def_var(
        &mut self,
        lhs: &Vec<(VirtualAddr, Option<TCTypeKey>, usize)>,
        typ: &Option<Expr>,
        rhs: &RightHandSide,
    ) -> Option<usize> {
        //let mut range_marker = None;
        // handle the right hand side
        match rhs {
            RightHandSide::Nothing => {
                // define without values
                let t = self.t.expr_tc_type(&typ.as_ref().unwrap());
                let zero_addr = self.add_zero_val(t);
                let fctx = func_ctx!(self);
                for (addr, _, p) in lhs {
                    // dont need to worry about casting to interface
                    fctx.emit_assign(addr.clone(), zero_addr, None, Some(*p))
                }
                None
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

                    let mut cur_reg = expr_ctx!(self).cur_reg;
                    self.push_expr_ctx(ExprMode::Store(lhs[0].0.clone(), lhs[0].1), cur_reg);
                    cur_reg += 1;
                    let ok_ectx = comma_ok.then(|| {
                        let mode = ExprMode::Store(lhs[1].0.clone(), None);
                        ExprCtx::new(mode, cur_reg)
                    });
                    match val0 {
                        Expr::TypeAssert(tae) => {
                            self.gen_expr_type_assert(&tae.expr, &tae.typ, ok_ectx);
                        }
                        Expr::Index(ie) => {
                            self.gen_expr_index(&ie.expr, &ie.index, types[0], ok_ectx);
                        }
                        Expr::Unary(recv_expr) => {
                            assert_eq!(recv_expr.op, Token::ARROW);
                            self.gen_expr_recv(
                                &recv_expr.expr,
                                types[0],
                                ok_ectx,
                                Some(recv_expr.op_pos),
                            );
                        }
                        _ => {
                            unreachable!()
                        }
                    }
                    self.pop_expr_ctx();
                } else if values.len() == lhs.len() {
                    // define or assign with values
                    for (i, l) in lhs.iter().enumerate() {
                        self.store(l.0.clone(), l.1, |g| g.gen_expr(&values[i]));
                    }
                } else if values.len() == 1 {
                    // define or assign with function call that returns multiple value on the right
                    self.discard(|g| g.gen_expr(&val0));
                    // now assgin the return values
                    let reg_begin = expr_ctx!(self).cur_reg;
                    let types = self.t.expr_tuple_tc_types(val0);
                    for (i, l) in lhs.iter().enumerate() {
                        self.store(l.0.clone(), l.1, |g| {
                            g.cur_expr_emit_direct_assign(
                                types[i],
                                Addr::Regsiter(reg_begin + i),
                                Some(l.2),
                            );
                        });
                    }
                } else {
                    unreachable!();
                }
                None
            }
            RightHandSide::Range(r) => {
                // the range statement
                let right_addr = self.load(|g| g.gen_expr(r));
                let tkv = self.t.expr_range_tc_types(r);
                let types = [
                    Some(self.t.tc_type_to_value_type(tkv[0])),
                    //Some(self.t.tc_type_to_value_type(tkv[1])),
                    Some(self.t.tc_type_to_value_type(tkv[2])),
                ];
                let pos = Some(r.pos(&self.ast_objs));
                let init_inst = InterInst::with_op_t_index(
                    Opcode::RANGE_INIT,
                    types[0],
                    types[1],
                    Addr::Void,
                    right_addr,
                    Addr::Void,
                );
                func_ctx!(self).emit_inst(init_inst, pos);
                let range_marker = func_ctx!(self).next_code_index();

                let mut cur_reg = expr_ctx!(self).cur_reg;
                let k_mode = ExprMode::Store(lhs[0].0.clone(), lhs[0].1);
                self.push_expr_ctx(k_mode, cur_reg);
                cur_reg += 1;
                let v_mode = ExprMode::Store(lhs[1].0.clone(), lhs[1].1);
                let mut ectx_ex = ExprCtx::new(v_mode, cur_reg);
                self.emit_double_store(
                    &mut ectx_ex,
                    Opcode::RANGE,
                    Addr::Imm(0), // the block_end address, to be set
                    Addr::Void,
                    tkv[0],
                    Some(tkv[2]),
                    types[0],
                    types[1],
                    Addr::Void,
                    pos,
                );
                self.pop_expr_ctx();

                Some(range_marker)
            }
            // For Select, the result is already in registers
            RightHandSide::SelectRecv(addr, ok) => {
                let l = &lhs[0];
                func_ctx!(self).emit_assign(l.0.clone(), *addr, None, Some(l.2));
                if *ok {
                    let l = &lhs[1];
                    let reg = Addr::Regsiter(addr.as_reg_index() + 1);
                    func_ctx!(self).emit_assign(l.0.clone(), reg, None, Some(l.2));
                }
                None
            }
        }
    }

    fn gen_switch_body(&mut self, body: &BlockStmt, tag_addr: Addr, tag_type: ValueType) {
        let mut helper = SwitchHelper::new();
        let mut has_default = false;
        for (i, stmt) in body.list.iter().enumerate() {
            helper.add_case_clause();
            let cc = SwitchHelper::to_case_clause(stmt);
            match &cc.list {
                Some(l) => {
                    for c in l.iter() {
                        let pos = Some(stmt.pos(&self.ast_objs));
                        let addr = self.load(|g| g.gen_expr(c));
                        let fctx = func_ctx!(self);
                        helper.tags.add_case(i, fctx.next_code_index());
                        fctx.emit_inst(
                            InterInst::with_op_t_index(
                                Opcode::SWITCH,
                                Some(tag_type),
                                None,
                                Addr::Void,
                                tag_addr,
                                addr,
                            ),
                            pos,
                        );
                    }
                }
                None => has_default = true,
            }
        }

        let fctx = func_ctx!(self);
        helper.tags.add_default(fctx.next_code_index());
        fctx.emit_inst(InterInst::with_op(Opcode::JUMP), None);

        for (i, stmt) in body.list.iter().enumerate() {
            let cc = SwitchHelper::to_case_clause(stmt);
            let fctx = func_ctx!(self);
            let default = cc.list.is_none();
            if default {
                helper.tags.patch_default(fctx, fctx.next_code_index());
            } else {
                helper.tags.patch_case(fctx, i, fctx.next_code_index());
            }
            for s in cc.body.iter() {
                self.visit_stmt(s);
            }
            if !SwitchHelper::has_fall_through(stmt) {
                let fctx = func_ctx!(self);
                if default {
                    helper.ends.add_default(fctx.next_code_index());
                } else {
                    helper.ends.add_case(i, fctx.next_code_index());
                }
                fctx.emit_inst(InterInst::with_op(Opcode::JUMP), None);
            }
        }
        let end = func_ctx!(self).next_code_index();
        helper.patch_ends(func_ctx!(self), end);
        // jump to the end if there is no default code
        if !has_default {
            let func = func_ctx!(self);
            helper.tags.patch_default(func, end);
        }
    }

    fn gen_func_def(
        &mut self,
        tc_type: TCTypeKey, // Meta,
        f_type_key: FuncTypeKey,
        recv: Option<FieldList>,
        body: &BlockStmt,
    ) -> FunctionKey {
        let typ = &self.ast_objs.ftypes[f_type_key];
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
        let mut fctx = FuncCtx::new(fkey, Some(tc_type), self.consts);
        if let Some(fl) = &typ.results {
            fctx.add_params(&fl, self.ast_objs, &self.t);
        }
        match recv {
            Some(recv) => {
                let mut fields = recv;
                fields.list.append(&mut typ.params.list.clone());
                fctx.add_params(&fields, self.ast_objs, &self.t)
            }
            None => fctx.add_params(&typ.params, self.ast_objs, &self.t),
        };
        self.func_ctx_stack.push(fctx);
        // process function body
        self.visit_stmt_block(body);

        func_ctx!(self).emit_return(None, Some(body.r_brace), &self.objects.functions);

        let f = self.func_ctx_stack.pop().unwrap();
        self.results.push(f);
        fkey
    }

    fn gen_builtin_call(
        &mut self,
        func_expr: &Expr,
        params: &Vec<Expr>,
        builtin: &Builtin,
        return_types: &[TCTypeKey],
        ellipsis: bool,
        pos: Option<usize>,
    ) {
        let slice_op_types = |g: &mut CodeGen| {
            let t0 = if ellipsis && g.t.expr_value_type(&params[1]) == ValueType::String {
                ValueType::String
            } else {
                ValueType::Slice
            };
            let (_, t_elem) =
                g.t.sliceable_expr_value_types(&params[0], g.objects, g.dummy_gcv);
            (t0, g.t.tc_type_to_value_type(t_elem))
        };
        match builtin {
            Builtin::Make => {
                let meta_addr = self.load(|g| g.gen_expr(&params[0]));
                let mut flag = ValueType::FlagA;
                let mut arg1 = Addr::Void;
                let mut arg2 = Addr::Void;
                if params.len() >= 2 {
                    flag = ValueType::FlagB;
                    arg1 = self.load(|g| g.gen_expr(&params[1]));
                }
                if params.len() >= 3 {
                    flag = ValueType::FlagC;
                    arg2 = self.load(|g| g.gen_expr(&params[2]));
                }
                self.cur_expr_emit_assign(return_types[0], pos, |f, d, p| {
                    let inst = InterInst::with_op_t_index(
                        Opcode::MAKE,
                        Some(flag),
                        None,
                        d,
                        meta_addr,
                        arg1,
                    );
                    f.emit_inst(inst, p);
                    if arg2 != Addr::Void {
                        let inst = InterInst::with_op_index(Opcode::VOID, d, arg2, Addr::Void);
                        f.emit_inst(inst, p);
                    }
                });
            }
            Builtin::Complex => {
                let addr0 = self.load(|g| g.gen_expr(&params[0]));
                let addr1 = self.load(|g| g.gen_expr(&params[1]));
                let t = self.t.expr_value_type(&params[0]);
                self.cur_expr_emit_assign(return_types[0], pos, |f, d, p| {
                    let inst =
                        InterInst::with_op_t_index(Opcode::COMPLEX, Some(t), None, d, addr0, addr1);
                    f.emit_inst(inst, p);
                });
            }
            Builtin::New
            | Builtin::Real
            | Builtin::Imag
            | Builtin::Len
            | Builtin::Cap
            | Builtin::Ffi => {
                let addr0 = self.load(|g| g.gen_expr(&params[0]));
                let addr1 = if params.len() > 1 {
                    self.load(|g| g.gen_expr(&params[1]))
                } else {
                    Addr::Void
                };
                self.cur_expr_emit_assign(return_types[0], pos, |f, d, p| {
                    let op = match builtin {
                        Builtin::New => Opcode::NEW,
                        Builtin::Real => Opcode::REAL,
                        Builtin::Imag => Opcode::IMAG,
                        Builtin::Len => Opcode::LEN,
                        Builtin::Cap => Opcode::CAP,
                        Builtin::Ffi => Opcode::FFI,
                        _ => unreachable!(),
                    };
                    let inst = InterInst::with_op_index(op, d, addr0, addr1);
                    f.emit_inst(inst, p);
                });
            }
            Builtin::Append => {
                let ft = self.t.try_expr_tc_type(func_expr).unwrap();
                let init_reg = expr_ctx!(self).cur_reg;
                self.gen_call_params(ft, params, ellipsis);
                let types = slice_op_types(self);
                self.cur_expr_emit_assign(return_types[0], pos, |f, d, p| {
                    let inst = InterInst::with_op_t_index(
                        Opcode::APPEND,
                        Some(types.0),
                        Some(types.1),
                        d,
                        Addr::Regsiter(init_reg),
                        Addr::Regsiter(init_reg + 1),
                    );
                    f.emit_inst(inst, p);
                });
                expr_ctx!(self).cur_reg = init_reg;
            }
            Builtin::Copy => {
                let addr0 = self.load(|g| g.gen_expr(&params[0]));
                let addr1 = self.load(|g| g.gen_expr(&params[1]));
                let types = slice_op_types(self);
                self.cur_expr_emit_assign(return_types[0], pos, |f, d, p| {
                    let inst = InterInst::with_op_t_index(
                        Opcode::COPY,
                        Some(types.0),
                        Some(types.1),
                        d,
                        addr0,
                        addr1,
                    );
                    f.emit_inst(inst, p);
                });
            }
            Builtin::Delete | Builtin::Close | Builtin::Panic | Builtin::Assert => {
                let addr0 = self.load(|g| g.gen_expr(&params[0]));
                let addr1 = if params.len() > 1 {
                    self.load(|g| g.gen_expr(&params[1]))
                } else {
                    Addr::Void
                };
                let op = match builtin {
                    Builtin::Delete => Opcode::DELETE,
                    Builtin::Close => Opcode::CLOSE,
                    Builtin::Panic => Opcode::PANIC,
                    Builtin::Assert => Opcode::ASSERT,
                    _ => unreachable!(),
                };
                let inst = InterInst::with_op_index(op, Addr::Void, addr0, addr1);
                func_ctx!(self).emit_inst(inst, pos);
            }

            Builtin::Recover => {
                let inst = InterInst::with_op(Opcode::RECOVER);
                func_ctx!(self).emit_inst(inst, pos);
            }
            _ => unimplemented!(),
        };
    }

    fn gen_conversion(&mut self, to: &Expr, from: &Expr, pos: Option<usize>) {
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
        let from_addr = self.load(|g| g.gen_expr(from));
        let mut converted = false;

        let tc_to = self.t.underlying_tc(self.t.expr_tc_type(to));
        let typ_to = self.t.tc_type_to_value_type(tc_to);
        let tc_from = self.t.underlying_tc(self.t.expr_tc_type(from));
        let typ_from = self.t.tc_type_to_value_type(tc_from);

        if typ_from == ValueType::Void || identical_ignore_tags(tc_to, tc_from, self.tc_objs) {
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
                        self.cur_expr_emit_assign(tc_to, pos, |f, d, p| {
                            f.emit_cast_iface(d, from_addr, iface_index, p);
                        });
                        converted = true;
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
                        ValueType::String => (typ_from == ValueType::Slice)
                            .then(|| self.tc_objs.types[tc_from].try_as_slice().unwrap().elem()),
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

                    self.cur_expr_emit_assign(tc_to, pos, |f, d, p| {
                        f.emit_cast(d, from_addr, Addr::Void, typ_to, Some(typ_from), t2, p);
                    });
                    converted = true;
                }
                ValueType::Channel => { /* nothing to be done */ }
                _ => {
                    dbg!(typ_to);
                    unreachable!()
                }
            }
        }
        if !converted {
            self.cur_expr_emit_direct_assign(tc_from, from_addr, pos);
        }
    }

    fn gen_expr_call(
        &mut self,
        func_expr: &Expr,
        params: &Vec<Expr>,
        ellipsis: bool,
        style: CallStyle,
    ) {
        let pos = Some(func_expr.pos(&self.ast_objs));
        let ft = self.t.expr_tc_type(func_expr);

        match *self.t.expr_mode(func_expr) {
            // built in function
            OperandMode::Builtin(builtin) => {
                let return_types = self.t.sig_returns_tc_types(ft);
                self.gen_builtin_call(func_expr, params, &builtin, &return_types, ellipsis, pos);
            }
            // conversion
            OperandMode::TypeExpr => {
                assert!(params.len() == 1);
                self.gen_conversion(func_expr, &params[0], pos);
            }
            // normal goscript function
            _ => {
                let next_sb = Addr::Regsiter(expr_ctx!(self).cur_reg);
                // next_sb may func_addr overlap, which is fine
                let func_addr = self.load(|g| g.gen_expr(func_expr));
                let inst = InterInst::with_op_index(
                    Opcode::PRE_CALL,
                    func_addr,
                    next_sb,
                    Addr::Imm(params.len() as OpIndex),
                );
                func_ctx!(self).emit_inst(inst, pos);

                // make sure params are at next_sb
                expr_ctx!(self).cur_reg = next_sb.as_reg_index();
                self.gen_call_params(ft, params, ellipsis);
                func_ctx!(self).emit_call(style, pos);

                let return_types = self.t.sig_returns_tc_types(ft);
                if !return_types.is_empty() {
                    // assgin the first return value
                    // the cases of returning multiple values are handled elsewhere
                    self.cur_expr_emit_direct_assign(return_types[0], next_sb, pos);
                }

                let return_count = return_types.len();
                expr_ctx!(self).cur_reg += return_count;
            }
        }
    }

    fn gen_call_params(&mut self, func: TCTypeKey, params: &Vec<Expr>, ellipsis: bool) {
        let (sig_params, variadic) = self.t.sig_params_tc_types(func);
        let need_pack = !ellipsis && variadic.is_some();
        let non_variadic_count = sig_params.len() - if need_pack { 1 } else { 0 };

        let init_reg = expr_ctx!(self).cur_reg;
        for (i, e) in params.iter().enumerate() {
            let addr = expr_ctx!(self).inc_cur_reg();
            let lhs_type = if i < non_variadic_count {
                sig_params[i]
            } else {
                variadic.unwrap()
            };
            self.store(VirtualAddr::Direct(addr), Some(lhs_type), |g| g.gen_expr(e));
        }

        debug_assert!(params.len() >= non_variadic_count);
        if need_pack {
            if let Some(t) = variadic {
                let variadic_count = params.len() - non_variadic_count;
                let variadic_begin_reg = init_reg + non_variadic_count;
                let pos = Some(params[non_variadic_count].pos(&self.ast_objs));
                let t_elem = self.t.tc_type_to_value_type(t);
                let begin = Addr::Regsiter(variadic_begin_reg);
                let end = Addr::Regsiter(variadic_begin_reg + variadic_count);
                let inst = InterInst::with_op_t_index(
                    Opcode::PACK_VARIADIC,
                    Some(t_elem),
                    None,
                    begin,
                    begin,
                    end,
                );
                func_ctx!(self).emit_inst(inst, pos);

                expr_ctx!(self).cur_reg = variadic_begin_reg + 1; // done with the rest registers
            }
        }
    }

    fn gen_expr_recv(
        &mut self,
        channel: &Expr,
        val_tc_type: TCTypeKey,
        ok_lhs_ectx: Option<ExprCtx>,
        pos: Option<usize>,
    ) {
        let channel_addr = self.load(|g| g.gen_expr(channel));
        match ok_lhs_ectx {
            Some(mut ok_ectx) => {
                self.emit_double_store(
                    &mut ok_ectx,
                    Opcode::RECV,
                    channel_addr,
                    Addr::Void,
                    val_tc_type,
                    None,
                    None,
                    Some(ValueType::FlagB),
                    Addr::Void,
                    pos,
                );
            }
            None => {
                self.cur_expr_emit_assign(val_tc_type, pos, |f, d, p| {
                    let inst = InterInst::with_op_t_index(
                        Opcode::RECV,
                        None,
                        Some(ValueType::FlagA),
                        d,
                        channel_addr,
                        Addr::Void,
                    );
                    f.emit_inst(inst, p);
                });
            }
        }
    }

    fn gen_expr_index(
        &mut self,
        container: &Expr,
        index: &Expr,
        val_tc_type: TCTypeKey,
        ok_lhs_ectx: Option<ExprCtx>,
    ) {
        let container_addr = self.load(|g| g.gen_expr(container));
        let index_reg = self.load(|g| g.gen_expr(index));
        let zero = self.add_zero_val(val_tc_type);
        let pos = Some(container.pos(&self.ast_objs));
        match ok_lhs_ectx {
            Some(mut ok_ectx) => {
                self.emit_double_store(
                    &mut ok_ectx,
                    Opcode::LOAD_MAP,
                    container_addr,
                    index_reg,
                    val_tc_type,
                    None,
                    None,
                    Some(ValueType::FlagB),
                    zero,
                    pos,
                );
            }
            None => {
                let (op, t1) = match self.t.expr_value_type(container) {
                    ValueType::Map => (Opcode::LOAD_MAP, ValueType::FlagA),
                    ValueType::Array => (Opcode::LOAD_ARRAY, ValueType::Void),
                    ValueType::Slice => (Opcode::LOAD_SLICE, ValueType::Void),
                    _ => unreachable!(),
                };
                self.cur_expr_emit_assign(val_tc_type, pos, |f, d, p| {
                    let inst = InterInst::with_op_t_index(
                        op,
                        None,
                        Some(t1),
                        d,
                        container_addr,
                        index_reg,
                    );
                    f.emit_inst(inst, p);
                    if op == Opcode::LOAD_MAP {
                        let inst_ex =
                            InterInst::with_op_index(Opcode::VOID, Addr::Void, zero, Addr::Void);
                        f.emit_inst(inst_ex, p);
                    }
                });
            }
        }
    }

    fn gen_expr_type_assert(
        &mut self,
        expr: &Expr,
        typ: &Option<Expr>,
        ok_lhs_ectx: Option<ExprCtx>,
    ) {
        let val_addr = self.load(|g| g.gen_expr(expr));
        let val_tc_type = self.t.expr_tc_type(typ.as_ref().unwrap());
        let meta = self
            .t
            .tc_type_to_meta(val_tc_type, self.objects, self.dummy_gcv);
        let meta_addr = func_ctx!(self).add_const(GosValue::new_metadata(meta));
        let pos = Some(expr.pos(self.ast_objs));
        match ok_lhs_ectx {
            Some(mut ok_ectx) => {
                self.emit_double_store(
                    &mut ok_ectx,
                    Opcode::TYPE_ASSERT,
                    val_addr,
                    meta_addr,
                    val_tc_type,
                    None,
                    None,
                    Some(ValueType::FlagB),
                    Addr::Void,
                    pos,
                );
            }
            None => {
                self.cur_expr_emit_assign(val_tc_type, pos, |f, d, p| {
                    let inst = InterInst::with_op_t_index(
                        Opcode::TYPE_ASSERT,
                        None,
                        Some(ValueType::FlagA),
                        d,
                        val_addr,
                        meta_addr,
                    );
                    f.emit_inst(inst, p);
                });
            }
        }
    }

    fn gen_expr_ref(&mut self, expr: &Expr, ref_tc_type: TCTypeKey) {
        let pos = Some(expr.pos(&self.ast_objs));
        match expr {
            Expr::Ident(ikey) => {
                let va = self.resolve_any_ident(ikey, None);
                match va {
                    VirtualAddr::Direct(_) => {
                        let meta = self.t.node_meta(expr.id(), self.objects, self.dummy_gcv);
                        let t = meta.value_type(&self.objects.metas);
                        let entity_key = self.t.object_use(*ikey);
                        let fctx = func_ctx!(self);
                        let ind = *fctx.entity_index(&entity_key).unwrap();
                        let desc =
                            ValueDesc::new(fctx.f_key, ind.as_var_index() as OpIndex, t, false);
                        // for package ctors, all locals are "closed"
                        if !fctx.is_ctor(&self.objects.functions) {
                            let uv_index = fctx.add_upvalue(&entity_key, desc);
                            self.cur_expr_emit_assign(ref_tc_type, pos, |f, d, p| {
                                let inst = InterInst::with_op_index(
                                    Opcode::REF_UPVALUE,
                                    d,
                                    uv_index.as_direct_addr(),
                                    Addr::Void,
                                );
                                f.emit_inst(inst, p);
                            });
                        } else {
                            self.cur_expr_emit_assign(ref_tc_type, pos, |f, d, p| {
                                let inst =
                                    InterInst::with_op_index(Opcode::REF, d, ind, Addr::Void);
                                f.emit_inst(inst, p);
                            });
                        }
                    }
                    VirtualAddr::UpValue(addr) => {
                        self.cur_expr_emit_assign(ref_tc_type, pos, |f, d, p| {
                            let inst =
                                InterInst::with_op_index(Opcode::REF_UPVALUE, d, addr, Addr::Void);
                            f.emit_inst(inst, p);
                        });
                    }
                    VirtualAddr::PackageMember(pkg, ident) => {
                        self.cur_expr_emit_assign(ref_tc_type, pos, |f, d, p| {
                            let inst =
                                InterInst::with_op_index(Opcode::REF_PKG_MEMBER, d, pkg, ident);
                            f.emit_inst(inst, p);
                        });
                    }
                    _ => unreachable!(),
                }
            }
            Expr::Index(iexpr) => {
                let (t0, _) =
                    self.t
                        .sliceable_expr_value_types(&iexpr.expr, self.objects, self.dummy_gcv);
                let t1 = self.t.expr_value_type(&iexpr.index);
                let lhs_addr = self.load(|g| g.gen_expr(&iexpr.expr));
                let index_addr = self.load(|g| g.gen_expr(&iexpr.index));
                let pos = Some(iexpr.index.pos(&self.ast_objs));
                self.cur_expr_emit_assign(ref_tc_type, pos, |f, d, p| {
                    let inst = InterInst::with_op_t_index(
                        Opcode::REF_SLICE_MEMBER,
                        Some(t0),
                        Some(t1),
                        d,
                        lhs_addr,
                        index_addr,
                    );
                    f.emit_inst(inst, p);
                });
            }
            Expr::Selector(sexpr) => match self.t.try_pkg_key(&sexpr.expr) {
                Some(key) => {
                    let pkey = self.pkg_helper.get_runtime_key(key);
                    let pkg_addr = func_ctx!(self).add_package(pkey);
                    let index = Addr::PkgMemberIndex(pkey, sexpr.sel);
                    self.cur_expr_emit_assign(ref_tc_type, pos, |f, d, p| {
                        let inst =
                            InterInst::with_op_index(Opcode::REF_PKG_MEMBER, d, pkg_addr, index);
                        f.emit_inst(inst, p);
                    });
                }
                None => {
                    let struct_addr = self.load(|g| g.gen_expr(&sexpr.expr));
                    let (_, _, indices, _) = self.t.selection_vtypes_indices_sel_typ(sexpr.id());
                    let rt_indices = indices.iter().map(|x| *x as OpIndex).collect();
                    let (op, index) =
                        self.get_struct_field_op_index(rt_indices, Opcode::REF_STRUCT_FIELD);
                    self.cur_expr_emit_assign(ref_tc_type, pos, |f, d, p| {
                        let inst = InterInst::with_op_index(op, d, struct_addr, Addr::Imm(index));
                        f.emit_inst(inst, p);
                    });
                }
            },
            Expr::CompositeLit(_) => {
                let addr = self.load(|g| g.gen_expr(expr));
                self.cur_expr_emit_assign(ref_tc_type, pos, |f, d, p| {
                    let inst = InterInst::with_op_index(Opcode::REF, d, addr, Addr::Void);
                    f.emit_inst(inst, p);
                });
            }
            _ => {
                dbg!(&expr);
                unimplemented!()
            }
        }
    }

    fn gen_expr_type(&mut self, typ: &Expr) {
        let m = self.t.node_meta(typ.id(), self.objects, self.dummy_gcv);
        let pos = Some(typ.pos(&self.ast_objs));
        let addr = func_ctx!(self).add_const(GosValue::new_metadata(m));
        self.cur_expr_emit_direct_assign(self.t.expr_tc_type(typ), addr, pos);
    }

    fn gen_expr_const(&mut self, expr: &Expr, pos: Option<Pos>) {
        let (tc_type, val) = self.t.const_type_value(expr.id());
        let fctx = func_ctx!(self);
        //let t = val.typ();
        let addr = fctx.add_const(val);
        self.cur_expr_emit_direct_assign(tc_type, addr, pos);
    }

    fn gen_expr(&mut self, expr: &Expr) {
        if let Some(mode) = self.t.try_expr_mode(expr) {
            if let OperandMode::Constant(_) = mode {
                self.gen_expr_const(expr, Some(expr.pos(&self.ast_objs)));
                return;
            }
        }
        walk_expr(self, expr);
    }

    fn get_field_meta(&self, parent: &Meta, indices: &[usize]) -> Meta {
        match parent.mtype_unwraped(&self.objects.metas) {
            MetadataType::Struct(f, _) => f.get(indices, &self.objects.metas).meta,
            _ => unreachable!(),
        }
    }

    fn add_const_def(&mut self, ikey: &IdentKey, cst: GosValue, typ: ValueType) -> Addr {
        let fctx = func_ctx!(self);
        let index = fctx.add_const_var(self.t.object_def(*ikey), cst.clone());
        if fctx.is_ctor(&self.objects.functions) {
            let pkg_key = self.objects.functions[fctx.f_key].package;
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

    fn add_zero_val(&mut self, typ: TCTypeKey) -> Addr {
        // if self.zero_types.insert(typ) {
        //     // Make sure type info is cached
        //     self.t.tc_type_to_meta(typ, self.objects, self.dummy_gcv);
        // }
        // Addr::ZeroValue(typ)
        let zero = self
            .t
            .tc_type_to_meta(typ, self.objects, self.dummy_gcv)
            .zero(&self.objects.metas, self.dummy_gcv);
        Addr::Const(self.consts.add_const(zero))
    }

    pub fn get_struct_field_op_index(
        &mut self,
        indices: Vec<OpIndex>,
        default_op: Opcode,
    ) -> (Opcode, OpIndex) {
        debug_assert!(indices.len() > 0);
        if indices.len() == 1 {
            (default_op, indices[0] as OpIndex)
        } else {
            (
                match default_op {
                    Opcode::REF_STRUCT_FIELD => Opcode::REF_STRUCT_EMBEDDED_FIELD,
                    Opcode::LOAD_STRUCT => Opcode::LOAD_STRUCT_EMBEDDED,
                    Opcode::STORE_STRUCT => Opcode::STORE_STRUCT_EMBEDDED,
                    _ => default_op,
                },
                self.struct_selector.get_index(indices),
            )
        }
    }

    fn emit_double_store(
        &mut self,
        ectx_ex: &mut ExprCtx,
        op: Opcode,
        s0: Addr,
        s1: Addr,
        t0: TCTypeKey,
        t1: Option<TCTypeKey>,
        vt0: Option<ValueType>,
        vt1: Option<ValueType>,
        ex_s0: Addr,
        pos: Option<usize>,
    ) {
        let val_ectx = expr_ctx!(self);
        let (val_addr, val_direct, val_cast_i) = CodeGen::get_store_addr(
            &mut self.t,
            self.iface_selector,
            val_ectx,
            self.objects,
            self.dummy_gcv,
            t0,
        );
        let t1 = t1.unwrap_or(self.t.bool_tc_type());
        let (ok_addr, ok_direct, ok_cast_i) = CodeGen::get_store_addr(
            &mut self.t,
            self.iface_selector,
            ectx_ex,
            self.objects,
            self.dummy_gcv,
            t1,
        );

        let fctx = func_ctx!(self);
        if s1 != Addr::Void {
            let inst = InterInst::with_op_t_index(op, vt0, vt1, val_addr, s0, s1);
            let inst_ex = InterInst::with_op_index(Opcode::VOID, ok_addr, ex_s0, Addr::Void);
            fctx.emit_inst(inst, pos);
            fctx.emit_inst(inst_ex, pos);
        } else {
            let inst = InterInst::with_op_t_index(op, vt0, vt1, val_addr, s0, ok_addr);
            fctx.emit_inst(inst, pos);
        }

        if !val_direct {
            val_ectx.emit_direct_assign(fctx, val_addr, val_cast_i, pos);
        }
        if !ok_direct {
            ectx_ex.emit_direct_assign(fctx, ok_addr, ok_cast_i, pos);
        }
    }

    fn get_store_addr(
        t: &mut TypeLookup,
        iface_sel: &mut IfaceSelector,
        ectx: &mut ExprCtx,
        objs: &mut VMObjects,
        dummy_gcv: &mut GcoVec,
        t_rhs: TCTypeKey,
    ) -> (Addr, bool, Option<OpIndex>) {
        let (va, typ) = ectx.mode.as_store();
        let need_cast = typ.is_some() && t.should_cast_to_iface(typ.unwrap(), t_rhs);
        let direct = need_cast || va.try_as_direct_addr().is_some();
        let addr = match direct {
            true => va.as_direct_addr(),
            false => ectx.inc_cur_reg(),
        };
        let cast_index =
            need_cast.then(|| iface_sel.get_index((typ.unwrap(), t_rhs), t, objs, dummy_gcv));
        (addr, direct, cast_index)
    }

    fn cast_to_iface_index(&mut self, lhs: TCTypeKey, rhs: TCTypeKey) -> Option<OpIndex> {
        match self.t.should_cast_to_iface(lhs, rhs) {
            true => {
                let index = self.iface_selector.get_index(
                    (lhs, rhs),
                    &mut self.t,
                    self.objects,
                    self.dummy_gcv,
                );
                Some(index)
            }
            false => None,
        }
    }

    fn visit_stmt(&mut self, stmt: &Stmt) {
        let init_reg = self.expr_ctx_stack.last().map(|x| x.cur_reg).unwrap_or(0);
        self.push_expr_ctx(ExprMode::Discard, init_reg);
        walk_stmt(self, stmt);
        self.pop_expr_ctx();
    }

    fn load<F>(&mut self, f: F) -> Addr
    where
        F: FnOnce(&mut CodeGen),
    {
        let reg = expr_ctx!(self).cur_reg;
        self.push_expr_ctx(ExprMode::Load, reg);
        f(self);
        let ectx = self.pop_expr_ctx();
        if ectx.occupying_reg {
            expr_ctx!(self).inc_cur_reg();
        }
        ectx.load_addr
    }

    fn store<F>(&mut self, va: VirtualAddr, lhs_type: Option<TCTypeKey>, f: F)
    where
        F: FnOnce(&mut CodeGen),
    {
        let reg = expr_ctx!(self).cur_reg;
        self.push_expr_ctx(ExprMode::Store(va, lhs_type), reg);
        f(self);
        self.pop_expr_ctx();
    }

    fn discard<F>(&mut self, f: F)
    where
        F: FnOnce(&mut CodeGen),
    {
        let reg = expr_ctx!(self).cur_reg;
        self.push_expr_ctx(ExprMode::Discard, reg);
        f(self);
        self.pop_expr_ctx();
    }

    fn push_expr_ctx(&mut self, mode: ExprMode, cur_reg: usize) {
        self.expr_ctx_stack.push(ExprCtx::new(mode, cur_reg));
    }

    fn pop_expr_ctx(&mut self) -> ExprCtx {
        let ctx = self.expr_ctx_stack.pop().unwrap();
        func_ctx!(self).update_max_reg(ctx.cur_reg);
        ctx
    }

    fn cur_expr_emit_assign<F>(&mut self, rhs_type: TCTypeKey, pos: Option<Pos>, f: F)
    where
        F: FnOnce(&mut FuncCtx, Addr, Option<Pos>),
    {
        let lhs = expr_ctx!(self).lhs_type();
        let index = lhs.map(|x| self.cast_to_iface_index(x, rhs_type)).flatten();
        expr_ctx!(self).emit_assign(func_ctx!(self), index, pos, f);
    }

    fn cur_expr_emit_direct_assign(&mut self, rhs_type: TCTypeKey, src: Addr, pos: Option<Pos>) {
        let lhs = expr_ctx!(self).lhs_type();
        let index = lhs.map(|x| self.cast_to_iface_index(x, rhs_type)).flatten();
        expr_ctx!(self).emit_direct_assign(func_ctx!(self), src, index, pos);
    }

    pub fn gen_with_files(mut self, files: &Vec<File>, tcpkg: TCPackageKey) -> Vec<FuncCtx<'c>> {
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
        self.func_ctx_stack
            .push(FuncCtx::new(fkey, None, self.consts));

        let (names, vars) = self.pkg_helper.sort_var_decls(files, self.t.type_info());
        self.add_pkg_var_member(pkey, &names);

        self.pkg_helper.gen_imports(tcpkg, &mut func_ctx!(self));

        for f in files.iter() {
            for d in f.decls.iter() {
                self.visit_decl(d)
            }
        }
        for v in vars.iter() {
            self.push_expr_ctx(ExprMode::Discard, 0);
            self.gen_def_var(v);
            self.pop_expr_ctx();
        }

        func_ctx!(self).emit_return(Some(self.pkg_key), None, &self.objects.functions);
        self.results.push(self.func_ctx_stack.pop().unwrap());
        self.results
    }
}

impl<'a, 'c> ExprVisitor for CodeGen<'a, 'c> {
    type Result = ();

    fn visit_expr_ident(&mut self, this: &Expr, ident: &IdentKey) {
        let va = self.resolve_any_ident(ident, Some(this));
        let pos = Some(self.ast_objs.idents[*ident].pos);
        let tc_type = self.t.expr_tc_type(this);
        match va {
            VirtualAddr::Direct(addr) => self.cur_expr_emit_direct_assign(tc_type, addr, pos),
            VirtualAddr::PackageMember(pkg, index) => {
                self.cur_expr_emit_assign(tc_type, pos, |f, d, p| {
                    f.emit_load_pkg(d, pkg, index, p)
                });
            }
            VirtualAddr::UpValue(uv) => {
                self.cur_expr_emit_assign(tc_type, pos, |f, d, p| {
                    let inst = InterInst::with_op_index(Opcode::LOAD_UP_VALUE, d, uv, Addr::Void);
                    f.emit_inst(inst, p);
                });
            }
            _ => {
                dbg!(&va);
                unreachable!()
            }
        };
    }

    fn visit_expr_ellipsis(&mut self, _: &Expr, _els: &Option<Expr>) {
        unreachable!();
    }

    fn visit_expr_basic_lit(&mut self, this: &Expr, blit: &BasicLit) {
        self.gen_expr_const(this, Some(blit.pos));
    }

    /// Add function as a const and then generate a closure of it
    fn visit_expr_func_lit(&mut self, this: &Expr, flit: &FuncLit) {
        let tc_type = self.t.expr_tc_type(this);
        let fkey = self.gen_func_def(tc_type, flit.typ, None, &flit.body);
        let fctx = func_ctx!(self);
        let addr = fctx.add_const(GosValue::new_function(fkey));
        let pos = Some(flit.body.l_brace);
        self.cur_expr_emit_assign(tc_type, pos, |f, d, p| f.emit_closure(d, addr, p));
    }

    fn visit_expr_composit_lit(&mut self, _: &Expr, clit: &CompositeLit) {
        let tc_type = self.t.expr_tc_type(clit.typ.as_ref().unwrap());
        let meta = self
            .t
            .tc_type_to_meta(tc_type, &mut self.objects, self.dummy_gcv);
        //let vt = self.t.tc_type_to_value_type(tc_type);
        let pos = Some(clit.l_brace);
        let typ = &self.tc_objs.types[tc_type].underlying_val(&self.tc_objs);
        let meta = meta.underlying(&self.objects.metas);
        let mtype = &self.objects.metas[meta.key].clone();

        let ectx = expr_ctx!(self);
        let reg_base = ectx.cur_reg;
        let count = match mtype {
            MetadataType::Slice(_) | MetadataType::Array(_, _) => {
                let elem_type = match typ {
                    Type::Array(detail) => detail.elem(),
                    Type::Slice(detail) => detail.elem(),
                    _ => unreachable!(),
                };
                for (i, expr) in clit.elts.iter().enumerate() {
                    let (key, elem) = match expr {
                        Expr::KeyValue(kv) => {
                            // the key is a constant
                            let key_const = self.t.try_tc_const_value(kv.key.id()).unwrap();
                            let (key_i64, ok) = key_const.int_as_i64();
                            debug_assert!(ok);
                            (key_i64 as i32, &kv.val)
                        }
                        _ => (-1, expr),
                    };
                    let reg_key = reg_base + i * 2;
                    let reg_elem = reg_key + 1;
                    let fctx = func_ctx!(self);
                    let index_addr = fctx.add_const(GosValue::new_int32(key as i32));
                    fctx.emit_assign(VirtualAddr::new_reg(reg_key), index_addr, None, pos);
                    self.store(VirtualAddr::new_reg(reg_elem), Some(elem_type), |g| {
                        g.gen_expr(elem)
                    });
                }
                clit.elts.len()
            }
            MetadataType::Map(_, _) => {
                let map_type = typ.try_as_map().unwrap();
                for (i, expr) in clit.elts.iter().enumerate() {
                    let reg_key = reg_base + i * 2;
                    let reg_elem = reg_key + 1;
                    match expr {
                        Expr::KeyValue(kv) => {
                            self.store(VirtualAddr::new_reg(reg_key), Some(map_type.key()), |g| {
                                g.gen_expr(&kv.key)
                            });
                            self.store(
                                VirtualAddr::new_reg(reg_elem),
                                Some(map_type.elem()),
                                |g| g.gen_expr(&kv.val),
                            );
                        }
                        _ => unreachable!(),
                    }
                }
                clit.elts.len()
            }
            MetadataType::Struct(f, _) => {
                let fields = typ.try_as_struct().unwrap().fields();
                for (i, expr) in clit.elts.iter().enumerate() {
                    let (index, expr) = match expr {
                        Expr::KeyValue(kv) => {
                            let ident = kv.key.try_as_ident().unwrap();
                            let index = f.index_by_name(&self.ast_objs.idents[*ident].name);
                            (index, &kv.val)
                        }
                        _ => (i, expr),
                    };
                    let reg_key = reg_base + i * 2;
                    let reg_elem = reg_key + 1;
                    let fctx = func_ctx!(self);
                    let index_addr = fctx.add_const(GosValue::new_uint(index));
                    fctx.emit_assign(VirtualAddr::new_reg(reg_key), index_addr, None, pos);
                    let field_type = self.tc_objs.lobjs[fields[index]].typ().unwrap();
                    self.store(VirtualAddr::new_reg(reg_elem), Some(field_type), |g| {
                        g.gen_expr(expr)
                    });
                }
                clit.elts.len()
            }
            _ => {
                dbg!(&mtype);
                unreachable!()
            }
        };
        let fctx = func_ctx!(self);
        fctx.update_max_reg(reg_base + count);
        let meta_addr = fctx.add_const(GosValue::new_metadata(meta));
        self.cur_expr_emit_assign(tc_type, pos, |f, d, p| {
            f.emit_literal(d, reg_base, count, meta_addr, p);
        });
    }

    fn visit_expr_paren(&mut self, _: &Expr, expr: &Expr) {
        self.gen_expr(expr);
    }

    fn visit_expr_selector(&mut self, this: &Expr, lhs_expr: &Expr, ident: &IdentKey) {
        let pos = Some(lhs_expr.pos(&self.ast_objs));
        if let Some(key) = self.t.try_pkg_key(lhs_expr) {
            let pkg = self.pkg_helper.get_runtime_key(key);
            let fctx = func_ctx!(self);
            let pkg_addr = fctx.add_package(pkg);
            let index = Addr::PkgMemberIndex(pkg, *ident);
            let tc_type = self.t.expr_tc_type(this);
            self.cur_expr_emit_assign(tc_type, pos, |f, d, p| {
                f.emit_load_pkg(d, pkg_addr, index, p)
            });
            return;
        }

        let lhs_meta = self
            .t
            .node_meta(lhs_expr.id(), self.objects, self.dummy_gcv);
        //let lhs_type = lhs_meta.value_type(&self.objects.metas);
        let (recv_type, expr_type, indices, stype) =
            self.t.selection_vtypes_indices_sel_typ(this.id());
        let indices = indices.clone();
        match &stype {
            SelectionType::MethodNonPtrRecv | SelectionType::MethodPtrRecv => {
                let index_count = indices.len();
                let final_index = indices[index_count - 1];
                let embedded_indices = Vec::from_iter(indices[..index_count - 1].iter().cloned());
                let lhs_has_embedded = index_count > 1;
                let final_lhs_meta = match lhs_has_embedded {
                    false => lhs_meta,
                    true => self.get_field_meta(&lhs_meta, &embedded_indices),
                };
                let final_lhs_type = final_lhs_meta.value_type(&self.objects.metas);
                let recv_addr = if (final_lhs_type != ValueType::Pointer
                    && final_lhs_type != ValueType::Interface)
                    && stype == SelectionType::MethodPtrRecv
                {
                    if !lhs_has_embedded {
                        self.load(|g| g.gen_expr_ref(lhs_expr, recv_type))
                    } else {
                        let lhs_addr = self.load(|g| g.gen_expr(lhs_expr));
                        let rt_indices = embedded_indices.iter().map(|x| *x as OpIndex).collect();
                        let (op, index) =
                            self.get_struct_field_op_index(rt_indices, Opcode::REF_STRUCT_FIELD);
                        let result_addr = expr_ctx!(self).inc_cur_reg();
                        let inst =
                            InterInst::with_op_index(op, result_addr, lhs_addr, Addr::Imm(index));
                        func_ctx!(self).emit_inst(inst, pos);
                        result_addr
                    }
                } else {
                    let mut struct_addr = self.load(|g| g.gen_expr(lhs_expr));
                    if lhs_has_embedded {
                        let rt_indices = embedded_indices.iter().map(|x| *x as OpIndex).collect();
                        let (op, index) =
                            self.get_struct_field_op_index(rt_indices, Opcode::LOAD_STRUCT);
                        let addr = expr_ctx!(self).inc_cur_reg();
                        let inst =
                            InterInst::with_op_index(op, addr, struct_addr, Addr::Imm(index));
                        func_ctx!(self).emit_inst(inst, pos);
                        struct_addr = addr;
                    }
                    if final_lhs_type == ValueType::Pointer
                        && stype == SelectionType::MethodNonPtrRecv
                    {
                        let addr = expr_ctx!(self).inc_cur_reg();
                        let inst = InterInst::with_op_index(
                            Opcode::LOAD_POINTER,
                            addr,
                            struct_addr,
                            Addr::Void,
                        );
                        func_ctx!(self).emit_inst(inst, pos);
                        struct_addr = addr;
                    }
                    struct_addr
                };

                if final_lhs_type == ValueType::Interface {
                    self.cur_expr_emit_assign(expr_type, pos, |f, d, p| {
                        let inst = InterInst::with_op_index(
                            Opcode::BIND_INTERFACE_METHOD,
                            d,
                            recv_addr,
                            Addr::Imm(final_index as OpIndex),
                        );
                        f.emit_inst(inst, p);
                    });
                } else {
                    self.cur_expr_emit_assign(expr_type, pos, |f, d, p| {
                        let inst = InterInst::with_op_index(
                            Opcode::BIND_METHOD,
                            d,
                            recv_addr,
                            f.add_method(final_lhs_meta, final_index),
                        );
                        f.emit_inst(inst, p);
                    });
                }
            }
            SelectionType::NonMethod => {
                let lhs_addr = self.load(|g| g.gen_expr(lhs_expr));
                let rt_indices = indices.iter().map(|x| *x as OpIndex).collect();
                let (op, index) = self.get_struct_field_op_index(rt_indices, Opcode::LOAD_STRUCT);
                self.cur_expr_emit_assign(expr_type, pos, |f, d, p| {
                    let inst = InterInst::with_op_index(op, d, lhs_addr, Addr::Imm(index));
                    f.emit_inst(inst, p);
                });
            }
        }
    }

    fn visit_expr_index(&mut self, e: &Expr, container: &Expr, index: &Expr) {
        let t = self.t.expr_tc_type(e);
        self.gen_expr_index(container, index, t, None);
    }

    fn visit_expr_slice(
        &mut self,
        _: &Expr,
        expr: &Expr,
        low: &Option<Expr>,
        high: &Option<Expr>,
        max: &Option<Expr>,
    ) -> Self::Result {
        let (t0, tct_elem) = self
            .t
            .sliceable_expr_value_types(expr, self.objects, self.dummy_gcv);
        let pos = Some(expr.pos(&self.ast_objs));

        let slice_array_addr = self.load(|g| g.gen_expr(expr));
        let low_addr = match low {
            None => func_ctx!(self).add_const(GosValue::new_int(0)),
            Some(e) => self.load(|g| g.gen_expr(e)),
        };
        let high_addr = match high {
            None => func_ctx!(self).add_const(GosValue::new_int(-1)),
            Some(e) => self.load(|g| g.gen_expr(e)),
        };
        let max_addr = match max {
            None => func_ctx!(self).add_const(GosValue::new_int(-1)),
            Some(e) => self.load(|g| g.gen_expr(e)),
        };
        let t_elem = self.t.tc_type_to_value_type(tct_elem);
        self.cur_expr_emit_assign(tct_elem, pos, |f, d, p| {
            let inst = InterInst::with_op_t_index(
                Opcode::SLICE,
                Some(t0),
                Some(t_elem),
                d,
                slice_array_addr,
                low_addr,
            );
            let inst_ex = InterInst::with_op_index(Opcode::VOID, Addr::Void, high_addr, max_addr);
            f.emit_inst(inst, p);
            f.emit_inst(inst_ex, p);
        })
    }

    fn visit_expr_type_assert(&mut self, _: &Expr, expr: &Expr, typ: &Option<Expr>) {
        self.gen_expr_type_assert(expr, typ, None);
    }

    fn visit_expr_call(&mut self, _: &Expr, func_expr: &Expr, params: &Vec<Expr>, ellipsis: bool) {
        self.gen_expr_call(func_expr, params, ellipsis, CallStyle::Default);
    }

    fn visit_expr_star(&mut self, this: &Expr, expr: &Expr) {
        match self.t.expr_mode(expr) {
            OperandMode::TypeExpr => {
                self.gen_expr_type(this);
            }
            _ => {
                let pos = Some(expr.pos(&self.ast_objs));
                let typ = self.t.expr_tc_type(this);
                let addr = self.load(|g| g.gen_expr(expr));
                self.cur_expr_emit_assign(typ, pos, |f, d, p| {
                    let inst = InterInst::with_op_index(Opcode::REF, d, addr, Addr::Void);
                    f.emit_inst(inst, p);
                });
            }
        }
    }

    fn visit_expr_unary(&mut self, this: &Expr, expr: &Expr, op: &Token) {
        let typ = self.t.expr_tc_type(this);
        if op == &Token::AND {
            self.gen_expr_ref(expr, typ);
            return;
        } else if op == &Token::ARROW {
            self.gen_expr_recv(expr, typ, None, Some(this.pos(&self.ast_objs)))
        } else if op == &Token::ADD {
            self.gen_expr(expr);
            return;
        }

        let addr = self.load(|g| g.gen_expr(expr));
        let opcode = match op {
            Token::SUB => Opcode::UNARY_SUB,
            Token::XOR => Opcode::UNARY_XOR,
            Token::NOT => Opcode::NOT,
            Token::ARROW => Opcode::RECV,
            _ => {
                dbg!(op);
                unreachable!()
            }
        };
        let t = self.t.tc_type_to_value_type(typ);
        let pos = Some(this.pos(&self.ast_objs));
        self.cur_expr_emit_assign(typ, pos, |f, d, p| {
            let inst = InterInst::with_op_t_index(opcode, Some(t), None, d, addr, Addr::Void);
            f.emit_inst(inst, p);
        });
    }

    fn visit_expr_binary(&mut self, this: &Expr, left: &Expr, op: &Token, right: &Expr) {
        let typ = self.t.expr_tc_type(this);
        let left_addr = self.load(|g| g.gen_expr(left));
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
            Token::LAND => Opcode::JUMP_IF,
            Token::LOR => Opcode::JUMP_IF_NOT,
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
            Opcode::JUMP_IF | Opcode::JUMP_IF_NOT => {
                let fctx = func_ctx!(self);
                let inst = InterInst::with_op_index(code, Addr::Void, left_addr, Addr::Void);
                fctx.emit_inst(inst, pos);
                Some(fctx.next_code_index() - 1)
            }
            _ => None,
        };

        let right_addr = self.load(|g| g.gen_expr(right));

        if let Some(i) = mark {
            let fctx = func_ctx!(self);
            let diff = fctx.next_code_index() - i - 1;
            fctx.inst_mut(i).d = Addr::Imm(diff as OpIndex);
            let const_addr = match code {
                Opcode::JUMP_IF => fctx.add_const(GosValue::new_bool(true)),
                Opcode::JUMP_IF_NOT => fctx.add_const(GosValue::new_bool(false)),
                _ => unreachable!(),
            };
            self.cur_expr_emit_direct_assign(typ, const_addr, pos);
        } else {
            let t1 = match code {
                Opcode::SHL | Opcode::SHR | Opcode::EQL => Some(self.t.expr_value_type(right)),
                _ => None,
            };
            self.cur_expr_emit_assign(typ, pos, |f, d, p| {
                let inst = InterInst::with_op_t_index(code, Some(t), t1, d, left_addr, right_addr);
                f.emit_inst(inst, p);
            });
        }
    }

    fn visit_expr_key_value(&mut self, _e: &Expr, _key: &Expr, _val: &Expr) {
        unreachable!();
    }

    fn visit_expr_array_type(&mut self, this: &Expr, _: &Option<Expr>, _: &Expr) {
        self.gen_expr_type(this)
    }

    fn visit_expr_struct_type(&mut self, this: &Expr, _s: &StructType) {
        self.gen_expr_type(this)
    }

    fn visit_expr_func_type(&mut self, this: &Expr, _s: &FuncTypeKey) {
        self.gen_expr_type(this)
    }

    fn visit_expr_interface_type(&mut self, this: &Expr, _s: &InterfaceType) {
        self.gen_expr_type(this)
    }

    fn visit_map_type(&mut self, this: &Expr, _: &Expr, _: &Expr, _map: &Expr) {
        self.gen_expr_type(this)
    }

    fn visit_chan_type(&mut self, this: &Expr, _chan: &Expr, _dir: &ChanDir) {
        self.gen_expr_type(this)
    }

    fn visit_bad_expr(&mut self, _: &Expr, _e: &BadExpr) {
        unreachable!();
    }
}

impl<'a, 'c> StmtVisitor for CodeGen<'a, 'c> {
    type Result = ();

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
                    self.add_const_def(&ts.name, GosValue::new_metadata(m), ValueType::Metadata);
                }
                Spec::Value(vs) => match &gdecl.token {
                    Token::VAR => {
                        // package level vars are handled elsewhere due to ordering
                        if !func_ctx!(self).is_ctor(&self.objects.functions) {
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
        let offset = func_ctx!(self).next_code_index();
        let entity = self.t.object_def(stmt.label);
        let is_breakable = match &stmt.stmt {
            Stmt::For(_) | Stmt::Range(_) | Stmt::Select(_) | Stmt::Switch(_) => true,
            _ => false,
        };
        self.branch_helper.add_label(entity, offset, is_breakable);
        self.visit_stmt(&stmt.stmt);
    }

    fn visit_stmt_send(&mut self, sstmt: &SendStmt) {
        let chan_addr = self.load(|g| g.gen_expr(&sstmt.chan));
        let val_addr = self.load(|g| g.gen_expr(&sstmt.val));
        let inst = InterInst::with_op_index(Opcode::SEND, Addr::Void, chan_addr, val_addr);
        func_ctx!(self).emit_inst(inst, Some(sstmt.arrow));
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
                self.gen_expr_call(
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
        self.objects.functions[func_ctx!(self).f_key].flag = FuncFlag::HasDefer;
        match &dstmt.call {
            Expr::Call(call) => {
                self.gen_expr_call(
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
            let types = self.t.sig_returns_tc_types(func_ctx!(self).tc_key.unwrap());
            for (i, expr) in rstmt.results.iter().enumerate() {
                let va = VirtualAddr::Direct(Addr::LocalVar(i));
                self.store(va, Some(types[i]), |g| g.gen_expr(expr));
            }
        }
        func_ctx!(self).emit_return(None, Some(rstmt.ret), &self.objects.functions);
    }

    fn visit_stmt_branch(&mut self, bstmt: &BranchStmt) {
        match bstmt.token {
            Token::BREAK | Token::CONTINUE => {
                let entity = bstmt.label.map(|x| self.t.object_use(x));
                self.branch_helper.add_jump_point(
                    func_ctx!(self),
                    bstmt.token.clone(),
                    entity,
                    bstmt.token_pos,
                );
            }
            Token::GOTO => {
                let addr = Addr::Label(self.t.object_use(bstmt.label.unwrap()));
                func_ctx!(self).emit_inst(
                    InterInst::with_op_index(Opcode::JUMP, addr, Addr::Void, Addr::Void),
                    Some(bstmt.token_pos),
                )
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
        let cond_addr = self.load(|g| g.gen_expr(&ifstmt.cond));
        let fctx = func_ctx!(self);
        // imm to be set later
        fctx.emit_inst(
            InterInst::with_op_index(Opcode::JUMP_IF_NOT, Addr::Void, cond_addr, Addr::Void),
            Some(ifstmt.if_pos),
        );
        let top_marker = fctx.next_code_index();

        self.visit_stmt_block(&ifstmt.body);
        let marker_if_arm_end = if ifstmt.els.is_some() {
            let fctx = func_ctx!(self);
            // imm to be set later
            fctx.emit_inst(InterInst::with_op(Opcode::JUMP), Some(ifstmt.if_pos));
            Some(fctx.next_code_index())
        } else {
            None
        };

        // set the correct else jump target
        let fctx = func_ctx!(self);
        let offset = fctx.offset(top_marker);
        fctx.inst_mut(top_marker - 1).d = Addr::Imm(offset);

        if let Some(els) = &ifstmt.els {
            self.visit_stmt(els);
            // set the correct if_arm_end jump target
            let fctx = func_ctx!(self);
            let marker = marker_if_arm_end.unwrap();
            let offset = fctx.offset(marker);
            fctx.inst_mut(marker - 1).d = Addr::Imm(offset);
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
        let (addr, typ) = match &sstmt.tag {
            Some(e) => (self.load(|g| g.gen_expr(e)), self.t.expr_value_type(e)),
            None => (
                func_ctx!(self).add_const(GosValue::new_bool(true)),
                ValueType::Bool,
            ),
        };
        self.gen_switch_body(&*sstmt.body, addr, typ);

        self.branch_helper.leave_block(func_ctx!(self), None);
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

        let dst = expr_ctx!(self).inc_cur_reg();
        if let Some(_) = ident_expr {
            let inst_data: Vec<(ValueType, Addr)> = tstmt
                .body
                .list
                .iter()
                .map(|stmt| {
                    let tc_obj = self.t.object_implicit(&stmt.id());
                    let (addr, _, meta) = self.add_local_var(tc_obj);
                    (meta.value_type(&self.objects.metas), addr)
                })
                .collect();
            let s0 = self.load(|g| g.gen_expr(v));
            let count = Addr::Imm(inst_data.len() as OpIndex);
            let fctx = func_ctx!(self);
            fctx.emit_inst(InterInst::with_op_index(Opcode::TYPE, dst, s0, count), pos);
            for (t, addr) in inst_data.into_iter() {
                fctx.emit_inst(
                    InterInst::with_op_t_index(
                        Opcode::VOID,
                        Some(t),
                        None,
                        addr,
                        Addr::Void,
                        Addr::Void,
                    ),
                    pos,
                );
            }
        } else {
            let s0 = self.load(|g| g.gen_expr(v));
            let count = Addr::Imm(0);
            func_ctx!(self).emit_inst(InterInst::with_op_index(Opcode::TYPE, dst, s0, count), pos);
        }

        self.gen_switch_body(&*tstmt.body, dst, ValueType::Metadata);
    }

    fn visit_stmt_comm(&mut self, _cclause: &CommClause) {
        unreachable!();
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
            let (typ, chan_addr, pos) = match &c.comm {
                Some(comm) => match comm {
                    Stmt::Send(send_stmt) => {
                        let chan_addr = self.load(|g| g.gen_expr(&send_stmt.chan));
                        let val_addr = self.load(|g| g.gen_expr(&send_stmt.val));
                        (CommType::Send(val_addr), Some(chan_addr), send_stmt.arrow)
                    }
                    Stmt::Assign(ass_key) => {
                        let ass = &self.ast_objs.a_stmts[*ass_key];
                        let (e, pos) = SelectHelper::unwrap_recv(&ass.rhs[0]);
                        let chan_addr = self.load(|g| g.gen_expr(e));
                        let val_reg = expr_ctx!(self).inc_cur_reg();
                        let t = match &ass.lhs.len() {
                            1 => CommType::Recv(*ass_key, val_reg, false),
                            2 => {
                                expr_ctx!(self).inc_cur_reg(); // extra register for ok
                                CommType::Recv(*ass_key, val_reg, true)
                            }
                            _ => unreachable!(),
                        };
                        (t, Some(chan_addr), pos)
                    }
                    Stmt::Expr(expr_stmt) => {
                        let (e, pos) = SelectHelper::unwrap_recv(expr_stmt);
                        let chan_addr = self.load(|g| g.gen_expr(e));
                        (CommType::RecvNoLhs, Some(chan_addr), pos)
                    }
                    _ => unreachable!(),
                },
                None => (CommType::Default, None, c.colon),
            };
            helper.add_comm(typ, chan_addr, pos);
        }

        helper.emit_select(func_ctx!(self), Some(sstmt.select));

        let last_index = comms.len() - 1;
        for (i, c) in comms.iter().enumerate() {
            let begin = func_ctx!(self).next_code_index();

            match helper.comm_type(i) {
                CommType::Recv(ass_key, addr, ok) => {
                    let ass = &self.ast_objs.a_stmts[*ass_key];
                    self.gen_assign(
                        &ass.token,
                        &ass.lhs.iter().map(|x| x).collect(),
                        RightHandSide::SelectRecv(*addr, *ok),
                    );
                }
                _ => {}
            }

            for stmt in c.body.iter() {
                self.visit_stmt(stmt);
            }
            let fctx = func_ctx!(self);
            let mut end = fctx.next_code_index();
            // the last block doesn't jump
            if i < last_index {
                fctx.emit_inst(InterInst::with_op(Opcode::JUMP), None);
            } else {
                end -= 1;
            }

            helper.set_block_begin_end(i, begin, end);
        }

        helper.patch_select(func_ctx!(self));

        self.branch_helper.leave_block(func_ctx!(self), None);
    }

    fn visit_stmt_for(&mut self, fstmt: &ForStmt) {
        self.branch_helper.enter_block(true);

        if let Some(init) = &fstmt.init {
            self.visit_stmt(init);
        }
        let top_marker = func_ctx!(self).next_code_index();
        let out_marker = if let Some(cond) = &fstmt.cond {
            let cond_addr = self.load(|g| g.gen_expr(&cond));
            let fctx = func_ctx!(self);
            fctx.emit_inst(
                InterInst::with_op_index(Opcode::JUMP_IF_NOT, Addr::Void, cond_addr, Addr::Void),
                Some(fstmt.for_pos),
            );
            Some(fctx.next_code_index() - 1)
        } else {
            None
        };
        self.visit_stmt_block(&fstmt.body);
        let continue_marker = if let Some(post) = &fstmt.post {
            // "continue" jumps to post statements
            let m = func_ctx!(self).next_code_index();
            self.visit_stmt(post);
            m
        } else {
            // "continue" jumps to top directly if no post statements
            top_marker
        };

        // jump to the top
        let fctx = func_ctx!(self);
        let offset = -fctx.offset(top_marker) - 1;
        fctx.emit_inst(
            InterInst::with_op_index(Opcode::JUMP, Addr::Imm(offset), Addr::Void, Addr::Void),
            Some(fstmt.for_pos),
        );

        // set the correct else jump out target
        if let Some(m) = out_marker {
            let fctx = func_ctx!(self);
            let offset = fctx.offset(m) - 1;
            fctx.inst_mut(m).d = Addr::Imm(offset);
        }

        self.branch_helper
            .leave_block(func_ctx!(self), Some(continue_marker));
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
        let fctx = func_ctx!(self);
        let offset = -fctx.offset(marker) - 1;
        // tell Opcode::RANGE where to jump after it's done
        let end_offset = fctx.offset(marker);
        fctx.inst_mut(marker).s0 = Addr::Imm(end_offset);
        fctx.emit_inst(
            InterInst::with_op_index(Opcode::JUMP, Addr::Imm(offset), Addr::Void, Addr::Void),
            Some(rstmt.token_pos),
        );

        self.branch_helper
            .leave_block(func_ctx!(self), Some(marker));
    }

    fn visit_expr_stmt(&mut self, e: &Expr) {
        self.discard(|g| g.gen_expr(&e));
    }

    fn visit_empty_stmt(&mut self, _e: &EmptyStmt) {}

    fn visit_bad_stmt(&mut self, _b: &BadStmt) {
        unreachable!();
    }

    fn visit_bad_decl(&mut self, _b: &BadDecl) {
        unreachable!();
    }
}
