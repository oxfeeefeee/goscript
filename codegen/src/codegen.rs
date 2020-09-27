#![allow(dead_code)]

use std::collections::HashMap;
use std::convert::TryFrom;

use super::func::*;
use super::types::TypeLookup;

use goscript_vm::instruction::*;
use goscript_vm::null_key;
use goscript_vm::objects::{EntIndex, MetadataType};
use goscript_vm::value::*;

use goscript_parser::ast::*;
use goscript_parser::objects::Objects as AstObjects;
use goscript_parser::objects::*;
use goscript_parser::token::Token;
use goscript_parser::visitor::{walk_decl, walk_expr, walk_stmt, ExprVisitor, StmtVisitor};
use goscript_types::{TCObjects, TypeInfo};

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

/// Built-in functions are not called like normal function for performance reasons
pub struct BuiltInFunc {
    name: &'static str,
    opcode: Opcode,
    params_count: isize,
    variadic: bool,
}

impl BuiltInFunc {
    pub fn new(name: &'static str, op: Opcode, params: isize, variadic: bool) -> BuiltInFunc {
        BuiltInFunc {
            name: name,
            opcode: op,
            params_count: params,
            variadic: variadic,
        }
    }
}

/// CodeGen implements the code generation logic.
pub struct CodeGen<'a> {
    objects: &'a mut VMObjects,
    ast_objs: &'a AstObjects,
    tc_objs: &'a TCObjects,
    tlookup: TypeLookup<'a>,
    pkg_key: PackageKey,
    func_stack: Vec<FunctionKey>,
    built_in_funcs: Vec<BuiltInFunc>,
    built_in_vals: HashMap<&'static str, Opcode>,
    blank_ident: IdentKey,
}

impl<'a> ExprVisitor for CodeGen<'a> {
    type Result = Result<(), ()>;

    fn visit_expr(&mut self, expr: &Expr) -> Self::Result {
        walk_expr(self, expr)
    }

    fn visit_expr_ident(&mut self, ident: &IdentKey) -> Self::Result {
        let index = self.resolve_ident(ident)?;
        let t = self.tlookup.get_use_value_type(*ident);
        current_func_mut!(self).emit_load(index, t);
        Ok(())
    }

    fn visit_expr_ellipsis(&mut self, _els: &Option<Expr>) -> Self::Result {
        unreachable!();
    }

    fn visit_expr_basic_lit(&mut self, _blit: &BasicLit, id: NodeId) -> Self::Result {
        let val = self.tlookup.get_const_value(id, self.objects)?;
        let func = current_func_mut!(self);
        let t = val.get_type();
        let i = func.add_const(None, val);
        func.emit_load(i, t);
        Ok(())
    }

    /// Add function as a const and then generate a closure of it
    fn visit_expr_func_lit(&mut self, flit: &FuncLit, id: NodeId) -> Self::Result {
        let vm_ftype = self.tlookup.gen_type_meta_by_node_id(id, self.objects);
        let fkey = self.gen_func_def(vm_ftype, flit.typ, None, &flit.body)?;
        let func = current_func_mut!(self);
        let i = func.add_const(None, GosValue::Function(fkey));
        func.emit_load(i, ValueType::Function);
        func.emit_new(ValueType::Function);
        Ok(())
    }

    fn visit_expr_composit_lit(&mut self, clit: &CompositeLit) -> Self::Result {
        let val = self.get_comp_value(clit.typ.as_ref().unwrap(), &clit)?;
        let func = current_func_mut!(self);
        let t = val.get_type();
        let i = func.add_const(None, val);
        func.emit_load(i, t);
        Ok(())
    }

    fn visit_expr_paren(&mut self, expr: &Expr) -> Self::Result {
        self.visit_expr(expr)
    }

    fn visit_expr_selector(&mut self, expr: &Expr, ident: &IdentKey, id: NodeId) -> Self::Result {
        let (t0, t1) = self.tlookup.get_selection_value_types(id);
        let t = self
            .tlookup
            .gen_type_meta_by_node_id(expr.id(), self.objects);
        let name = &self.ast_objs.idents[*ident].name;
        if t1 == ValueType::Closure {
            let i = MetadataVal::method_index(&t, name, &self.objects.metas);
            let method = MetadataVal::get_method(&t, i, &self.objects.metas);
            let fkey = method.as_closure().func;
            let boxed_recv = self.objects.metas[*self.objects.functions[fkey].meta.as_meta()]
                .sig_metadata()
                .boxed_recv(&self.objects.metas);
            if boxed_recv {
                // desugar
                self.visit_expr_unary(expr, &Token::AND)?;
            } else {
                self.visit_expr(expr)?;
            }
            let func = current_func_mut!(self);
            let mi = func.add_const(None, GosValue::Function(fkey));
            func.emit_bind_method(mi.into(), t0);
        } else {
            self.visit_expr(expr)?;
            let i = MetadataVal::field_index(&t, name, &self.objects.metas);
            current_func_mut!(self).emit_load_field_imm(i, t0);
        }
        Ok(())
    }

    fn visit_expr_index(&mut self, expr: &Expr, index: &Expr) -> Self::Result {
        let t0 = self.tlookup.get_expr_value_type(expr);
        let t1 = self.tlookup.get_expr_value_type(index);
        self.visit_expr(expr)?;
        if let Some(const_val) = self.tlookup.get_tc_const_value(index.id()) {
            let (ival, _) = const_val.to_int().int_as_i64();
            if let Ok(i) = OpIndex::try_from(ival) {
                current_func_mut!(self).emit_load_index_imm(i, t0);
                return Ok(());
            }
        }
        self.visit_expr(index)?;
        current_func_mut!(self).emit_load_index(t0, t1);
        Ok(())
    }

    fn visit_expr_slice(
        &mut self,
        expr: &Expr,
        low: &Option<Expr>,
        high: &Option<Expr>,
        max: &Option<Expr>,
    ) -> Self::Result {
        self.visit_expr(expr)?;
        let t = self.tlookup.get_expr_value_type(expr);
        match low {
            None => current_func_mut!(self).emit_code(Opcode::PUSH_IMM),
            Some(e) => self.visit_expr(e)?,
        }
        match high {
            None => current_func_mut!(self).emit_inst(Opcode::PUSH_IMM, None, None, None, Some(-1)),
            Some(e) => self.visit_expr(e)?,
        }
        match max {
            None => current_func_mut!(self).emit_code_with_type(Opcode::SLICE, t),
            Some(e) => {
                self.visit_expr(e)?;
                current_func_mut!(self).emit_code_with_type(Opcode::SLICE_FULL, t);
            }
        }
        Ok(())
    }

    fn visit_expr_type_assert(&mut self, _expr: &Expr, _typ: &Option<Expr>) -> Self::Result {
        unimplemented!();
    }

    fn visit_expr_call(&mut self, func: &Expr, params: &Vec<Expr>, ellipsis: bool) -> Self::Result {
        // check if this is a built in function first
        if let Expr::Ident(ikey) = func {
            let ident = self.ast_objs.idents[*ikey].clone();
            if ident.entity.into_key().is_none() {
                return if let Some(i) = self.built_in_func_index(&ident.name) {
                    let t = self.tlookup.get_expr_value_type(&params[0]);
                    let t_last = self.tlookup.get_expr_value_type(params.last().unwrap());
                    let count = params.iter().map(|e| self.visit_expr(e)).count();
                    let bf = &self.built_in_funcs[i as usize];
                    let func = current_func_mut!(self);
                    let (t_variadic, count) = if bf.variadic {
                        if ellipsis {
                            (None, Some(0)) // do not pack params if there is ellipsis
                        } else {
                            (
                                Some(t_last),
                                Some((bf.params_count - 1 - count as isize) as OpIndex),
                            )
                        }
                    } else {
                        (None, None)
                    };
                    func.emit_inst(bf.opcode, Some(t), t_variadic, None, count);
                    Ok(())
                } else {
                    Err(())
                };
            }
        }

        // normal goscript function
        self.visit_expr(func)?;
        current_func_mut!(self).emit_pre_call();
        let _ = params
            .iter()
            .map(|e| -> Self::Result { self.visit_expr(e) })
            .count();
        // do not pack params if there is ellipsis
        current_func_mut!(self).emit_call(ellipsis);
        Ok(())
    }

    fn visit_expr_star(&mut self, expr: &Expr) -> Self::Result {
        self.visit_expr(expr)?;
        let t = self.tlookup.get_expr_value_type(expr);
        //todo: could this be a pointer type?
        let code = Opcode::DEREF;
        current_func_mut!(self).emit_code_with_type(code, t);
        Ok(())
    }

    fn visit_expr_unary(&mut self, expr: &Expr, op: &Token) -> Self::Result {
        let t = self.tlookup.get_expr_value_type(expr);
        if op == &Token::AND {
            match expr {
                Expr::Ident(ident) => {
                    let index = self.resolve_ident(ident)?;
                    match index {
                        EntIndex::LocalVar(i) => {
                            let func = current_func_mut!(self);
                            func.emit_inst(Opcode::REF_LOCAL, Some(t), None, None, Some(i))
                        }
                        EntIndex::UpValue(i) => {
                            let func = current_func_mut!(self);
                            func.emit_inst(Opcode::REF_UPVALUE, Some(t), None, None, Some(i))
                        }
                        _ => unreachable!(),
                    }
                }
                Expr::Index(iexpr) => {
                    let t0 = self.tlookup.get_expr_value_type(&iexpr.expr);
                    let t1 = self.tlookup.get_expr_value_type(&iexpr.index);
                    self.visit_expr(&iexpr.expr)?;
                    self.visit_expr(&iexpr.index)?;
                    current_func_mut!(self).emit_inst(
                        Opcode::REF_SLICE_MEMBER,
                        Some(t0),
                        Some(t1),
                        None,
                        None,
                    );
                }
                Expr::Selector(sexpr) => {
                    self.visit_expr(&sexpr.expr)?;
                    let t0 = self
                        .tlookup
                        .gen_type_meta_by_node_id(sexpr.expr.id(), &mut self.objects);
                    let name = &self.ast_objs.idents[sexpr.sel].name;
                    let i = MetadataVal::field_index(&t0, name, &self.objects.metas);
                    current_func_mut!(self).emit_inst(
                        Opcode::REF_STRUCT_FIELD,
                        None,
                        None,
                        None,
                        Some(i),
                    );
                }
                _ => unimplemented!(),
            }
            return Ok(());
        }

        self.visit_expr(expr)?;
        let code = match op {
            Token::ADD => Opcode::UNARY_ADD,
            Token::SUB => Opcode::UNARY_SUB,
            Token::XOR => Opcode::UNARY_XOR,
            _ => unreachable!(),
        };
        current_func_mut!(self).emit_code_with_type(code, t);
        Ok(())
    }

    fn visit_expr_binary(&mut self, left: &Expr, op: &Token, right: &Expr) -> Self::Result {
        self.visit_expr(left)?;
        let t = self.tlookup.get_expr_value_type(left);
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
            Token::LAND => Opcode::PUSH_FALSE,
            Token::LOR => Opcode::PUSH_TRUE,
            Token::NOT => Opcode::NOT,
            Token::EQL => Opcode::EQL,
            Token::LSS => Opcode::LSS,
            Token::GTR => Opcode::GTR,
            Token::NEQ => Opcode::NEQ,
            Token::LEQ => Opcode::LEQ,
            Token::GEQ => Opcode::GEQ,
            _ => unreachable!(),
        };
        // handles short circuit
        let mark_code = match op {
            Token::LAND => {
                let func = current_func_mut!(self);
                func.emit_inst(Opcode::JUMP_IF_NOT, None, None, None, None);
                Some((func.code.len(), code))
            }
            Token::LOR => {
                let func = current_func_mut!(self);
                func.emit_inst(Opcode::JUMP_IF, None, None, None, None);
                Some((func.code.len(), code))
            }
            _ => None,
        };
        self.visit_expr(right)?;
        if let Some((i, c)) = mark_code {
            let func = current_func_mut!(self);
            func.emit_inst(Opcode::JUMP, None, None, None, Some(1));
            func.emit_code_with_type(c, t);
            let diff = func.code.len() - i - 1;
            func.code[i - 1].set_imm(diff as OpIndex);
        } else {
            current_func_mut!(self).emit_code_with_type(code, t);
        }
        Ok(())
    }

    fn visit_expr_key_value(&mut self, _key: &Expr, _val: &Expr) -> Self::Result {
        unimplemented!();
    }

    fn visit_expr_array_type(&mut self, _: &Option<Expr>, _: &Expr, arr: &Expr) -> Self::Result {
        let val = self
            .tlookup
            .gen_type_meta_by_node_id(arr.id(), self.objects);
        let func = current_func_mut!(self);
        let t = val.get_type();
        let i = func.add_const(None, val);
        func.emit_load(i, t);
        Ok(())
    }

    fn visit_expr_struct_type(&mut self, _s: &StructType) -> Self::Result {
        unimplemented!();
    }

    fn visit_expr_func_type(&mut self, _s: &FuncTypeKey) -> Self::Result {
        unimplemented!();
    }

    fn visit_expr_interface_type(&mut self, _s: &InterfaceType) -> Self::Result {
        unimplemented!();
    }

    fn visit_map_type(&mut self, _: &Expr, _: &Expr, _map: &Expr) -> Self::Result {
        unimplemented!();
    }

    fn visit_chan_type(&mut self, _chan: &Expr, _dir: &ChanDir) -> Self::Result {
        unimplemented!();
    }

    fn visit_bad_expr(&mut self, _e: &BadExpr) -> Self::Result {
        unreachable!();
    }
}

impl<'a> StmtVisitor for CodeGen<'a> {
    type Result = Result<(), ()>;

    fn visit_stmt(&mut self, stmt: &Stmt) -> Self::Result {
        walk_stmt(self, stmt)
    }

    fn visit_decl(&mut self, decl: &Decl) -> Self::Result {
        walk_decl(self, decl)
    }

    fn visit_stmt_decl_gen(&mut self, gdecl: &GenDecl) -> Self::Result {
        for s in gdecl.specs.iter() {
            let spec = &self.ast_objs.specs[*s];
            match spec {
                Spec::Import(_is) => unimplemented!(),
                Spec::Type(ts) => {
                    let ident = self.ast_objs.idents[ts.name].clone();
                    let ident_key = ident.entity.into_key();
                    let typ = self.tlookup.gen_def_type_meta(ts.name, self.objects);
                    self.current_func_add_const_def(ident_key.unwrap(), typ);
                }
                Spec::Value(vs) => match &gdecl.token {
                    Token::VAR => {
                        let lhs = vs
                            .names
                            .iter()
                            .map(|n| -> Result<LeftHandSide, ()> {
                                let index = self.add_local_or_resolve_ident(n, true)?;
                                Ok(LeftHandSide::Primitive(index))
                            })
                            .collect::<Result<Vec<LeftHandSide>, ()>>()?;
                        self.gen_assign_def_var(&lhs, &vs.values, &vs.typ, None)?;
                    }
                    Token::CONST => self.gen_def_const(&vs.names, &vs.values)?,
                    _ => unreachable!(),
                },
            }
        }
        Ok(())
    }

    fn visit_stmt_decl_func(&mut self, fdecl: &FuncDeclKey) -> Self::Result {
        let decl = &self.ast_objs.fdecls[*fdecl];
        if decl.body.is_none() {
            unimplemented!()
        }
        let vm_ftype = self.tlookup.gen_def_type_meta(decl.name, self.objects);
        let stmt = decl.body.as_ref().unwrap();
        let fkey = self.gen_func_def(vm_ftype, decl.typ, decl.recv.clone(), stmt)?;
        let cls = GosValue::new_closure(fkey);
        // this is a struct method
        if let Some(self_ident) = &decl.recv {
            let field = &self.ast_objs.fields[self_ident.list[0]];
            let name = &self.ast_objs.idents[decl.name].name;
            let meta_key = *self.get_use_type_meta(&field.typ).as_meta();
            let key = match self.objects.metas[meta_key].typ() {
                MetadataType::Boxed(b) => *b.as_meta(),
                MetadataType::Named(_) => meta_key,
                _ => unreachable!(),
            };
            self.objects.metas[key].add_method(name.clone(), cls);
        } else {
            let ident = &self.ast_objs.idents[decl.name];
            let pkg = &mut self.objects.packages[self.pkg_key];
            let index = pkg.add_member(ident.entity_key().unwrap(), cls);
            if ident.name == "main" {
                pkg.set_main_func(index);
            }
        }
        Ok(())
    }

    fn visit_stmt_labeled(&mut self, _lstmt: &LabeledStmtKey) -> Self::Result {
        unimplemented!();
    }

    fn visit_stmt_send(&mut self, _sstmt: &SendStmt) -> Self::Result {
        unimplemented!();
    }

    fn visit_stmt_incdec(&mut self, idcstmt: &IncDecStmt) -> Self::Result {
        self.gen_assign(&idcstmt.token, &vec![&idcstmt.expr], &vec![], None)?;
        Ok(())
    }

    fn visit_stmt_assign(&mut self, astmt: &AssignStmtKey) -> Self::Result {
        let stmt = &self.ast_objs.a_stmts[*astmt];
        self.gen_assign(
            &stmt.token,
            &stmt.lhs.iter().map(|x| x).collect(),
            &stmt.rhs,
            None,
        )?;
        Ok(())
    }

    fn visit_stmt_go(&mut self, _gostmt: &GoStmt) -> Self::Result {
        unimplemented!();
    }

    fn visit_stmt_defer(&mut self, _dstmt: &DeferStmt) -> Self::Result {
        unimplemented!();
    }

    fn visit_stmt_return(&mut self, rstmt: &ReturnStmt) -> Self::Result {
        for (i, expr) in rstmt.results.iter().enumerate() {
            self.visit_expr(expr)?;
            let t = self.tlookup.get_expr_value_type(expr);
            let f = current_func_mut!(self);
            f.emit_store(
                &LeftHandSide::Primitive(EntIndex::LocalVar(i as OpIndex)),
                -1,
                None,
                t,
            );
            f.emit_pop(t);
        }
        current_func_mut!(self).emit_return();
        Ok(())
    }

    fn visit_stmt_branch(&mut self, _bstmt: &BranchStmt) -> Self::Result {
        unimplemented!();
    }

    fn visit_stmt_block(&mut self, bstmt: &BlockStmt) -> Self::Result {
        for stmt in bstmt.list.iter() {
            self.visit_stmt(stmt)?;
        }
        Ok(())
    }

    fn visit_stmt_if(&mut self, ifstmt: &IfStmt) -> Self::Result {
        if let Some(init) = &ifstmt.init {
            self.visit_stmt(init)?;
        }
        self.visit_expr(&ifstmt.cond)?;
        let func = current_func_mut!(self);
        func.emit_inst(Opcode::JUMP_IF_NOT, None, None, None, None);
        let top_marker = func.code.len();

        drop(func);
        self.visit_stmt_block(&ifstmt.body)?;
        let marker_if_arm_end = if ifstmt.els.is_some() {
            let func = current_func_mut!(self);
            // imm to be set later
            func.emit_inst(Opcode::JUMP, None, None, None, None);
            Some(func.code.len())
        } else {
            None
        };

        // set the correct else jump target
        let func = current_func_mut!(self);
        // todo: don't crash if OpIndex overflows
        let offset = OpIndex::try_from(func.code.len() - top_marker).unwrap();
        func.code[top_marker - 1].set_imm(offset);

        if let Some(els) = &ifstmt.els {
            self.visit_stmt(els)?;
            // set the correct if_arm_end jump target
            let func = current_func_mut!(self);
            let marker = marker_if_arm_end.unwrap();
            // todo: don't crash if OpIndex overflows
            let offset = OpIndex::try_from(func.code.len() - marker).unwrap();
            func.code[marker - 1].set_imm(offset);
        }
        Ok(())
    }

    fn visit_stmt_case(&mut self, _cclause: &CaseClause) -> Self::Result {
        unimplemented!();
    }

    fn visit_stmt_switch(&mut self, _sstmt: &SwitchStmt) -> Self::Result {
        unimplemented!();
    }

    fn visit_stmt_type_switch(&mut self, _tstmt: &TypeSwitchStmt) -> Self::Result {
        unimplemented!();
    }

    fn visit_stmt_comm(&mut self, _cclause: &CommClause) -> Self::Result {
        unimplemented!();
    }

    fn visit_stmt_select(&mut self, _sstmt: &SelectStmt) -> Self::Result {
        unimplemented!();
    }

    fn visit_stmt_for(&mut self, fstmt: &ForStmt) -> Self::Result {
        if let Some(init) = &fstmt.init {
            self.visit_stmt(init)?;
        }
        let top_marker = current_func!(self).code.len();
        let out_marker = if let Some(cond) = &fstmt.cond {
            self.visit_expr(&cond)?;
            let func = current_func_mut!(self);
            func.emit_inst(Opcode::JUMP_IF_NOT, None, None, None, None);
            Some(func.code.len())
        } else {
            None
        };
        self.visit_stmt_block(&fstmt.body)?;
        if let Some(post) = &fstmt.post {
            self.visit_stmt(post)?;
        }

        // jump to the top
        let func = current_func_mut!(self);
        // todo: don't crash if OpIndex overflows
        let offset = OpIndex::try_from(-((func.code.len() + 1 - top_marker) as isize)).unwrap();
        func.emit_inst(Opcode::JUMP, None, None, None, Some(offset));

        // set the correct else jump out target
        if let Some(m) = out_marker {
            let func = current_func_mut!(self);
            // todo: don't crash if OpIndex overflows
            let offset = OpIndex::try_from(func.code.len() - m).unwrap();
            func.code[m - 1].set_imm(offset);
        }
        Ok(())
    }

    fn visit_stmt_range(&mut self, rstmt: &RangeStmt) -> Self::Result {
        let blank = Expr::Ident(self.blank_ident);
        let lhs = vec![
            rstmt.key.as_ref().unwrap_or(&blank),
            rstmt.val.as_ref().unwrap_or(&blank),
        ];
        let marker = self
            .gen_assign(&rstmt.token, &lhs, &vec![], Some(&rstmt.expr))?
            .unwrap();

        self.visit_stmt_block(&rstmt.body)?;
        // jump to the top
        let func = current_func_mut!(self);
        // todo: don't crash if OpIndex overflows
        let offset = OpIndex::try_from(-((func.code.len() + 1 - marker) as isize)).unwrap();
        func.emit_inst(Opcode::JUMP, None, None, None, Some(offset));
        // now tell Opcode::RANGE where to jump after it's done
        // todo: don't crash if OpIndex overflows
        let end_offset = OpIndex::try_from(func.code.len() - (marker + 1)).unwrap();
        func.code[marker].set_imm(end_offset);
        Ok(())
    }

    fn visit_empty_stmt(&mut self, _e: &EmptyStmt) -> Self::Result {
        Ok(())
    }

    fn visit_bad_stmt(&mut self, _b: &BadStmt) -> Self::Result {
        unreachable!();
    }

    fn visit_bad_decl(&mut self, _b: &BadDecl) -> Self::Result {
        unreachable!();
    }
}

impl<'a> CodeGen<'a> {
    pub fn new(
        vmo: &'a mut VMObjects,
        asto: &'a AstObjects,
        tco: &'a TCObjects,
        ti: &'a TypeInfo,
        pkg: PackageKey,
        bk: IdentKey,
    ) -> CodeGen<'a> {
        let funcs = vec![
            BuiltInFunc::new("new", Opcode::NEW, 1, false),
            BuiltInFunc::new("make", Opcode::MAKE, 2, true),
            BuiltInFunc::new("len", Opcode::LEN, 1, false),
            BuiltInFunc::new("cap", Opcode::CAP, 1, false),
            BuiltInFunc::new("append", Opcode::APPEND, 2, true),
            BuiltInFunc::new("assert", Opcode::ASSERT, 1, false),
        ];
        let mut vals = HashMap::new();
        vals.insert("true", Opcode::PUSH_TRUE);
        vals.insert("false", Opcode::PUSH_FALSE);
        vals.insert("nil", Opcode::PUSH_NIL);
        CodeGen {
            objects: vmo,
            ast_objs: asto,
            tc_objs: tco,
            tlookup: TypeLookup::new(tco, ti),
            pkg_key: pkg,
            func_stack: Vec::new(),
            built_in_funcs: funcs,
            built_in_vals: vals,
            blank_ident: bk,
        }
    }

    fn resolve_ident(&mut self, ident: &IdentKey) -> Result<EntIndex, ()> {
        let id = &self.ast_objs.idents[*ident];
        // 0. try built-ins
        if id.entity_key().is_none() {
            if let Some(op) = self.built_in_vals.get(&*id.name) {
                return Ok(EntIndex::BuiltIn(*op));
            } else {
                return Err(());
            }
        }

        // 1. try local first
        let entity_key = id.entity_key().unwrap();
        let local = current_func_mut!(self)
            .entity_index(&entity_key)
            .map(|x| *x);
        if local.is_some() {
            return Ok(local.unwrap());
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
                    let desc = ValueDesc {
                        func: *ifunc,
                        index: ind.into(),
                        typ: self.tlookup.get_use_value_type(*ident),
                    };
                    Some(desc)
                } else {
                    None
                }
            });
        if let Some(uv) = upvalue {
            let func = current_func_mut!(self);
            let index = func.try_add_upvalue(&entity_key, uv);
            return Ok(index);
        }
        // 3. try the package level
        let pkg = &self.objects.packages[self.pkg_key];
        if let Some(index) = pkg.get_member_index(&entity_key) {
            return Ok(EntIndex::PackageMember(*index));
        }

        unreachable!();
    }

    fn add_local_or_resolve_ident(
        &mut self,
        ikey: &IdentKey,
        is_def: bool,
    ) -> Result<EntIndex, ()> {
        let ident = &self.ast_objs.idents[*ikey];
        if ident.is_blank() {
            return Ok(EntIndex::Blank);
        }
        if is_def {
            let meta = *self
                .tlookup
                .gen_def_type_meta(*ikey, self.objects)
                .as_meta();
            let zero_val = self.objects.metas[meta].zero_val().clone();
            let func = current_func_mut!(self);
            let ident_key = ident.entity.clone().into_key();
            let index = func.add_local(ident_key, Some(zero_val));
            if func.is_ctor() {
                let pkg_key = func.package;
                let pkg = &mut self.objects.packages[pkg_key];
                pkg.add_var(
                    ident_key.unwrap(),
                    index.into(),
                    self.objects.zero_val.zero_val_mark.clone(),
                );
            }
            Ok(index)
        } else {
            self.resolve_ident(ikey)
        }
    }

    fn gen_def_const(&mut self, names: &Vec<IdentKey>, values: &Vec<Expr>) -> Result<(), ()> {
        if names.len() != values.len() {
            return Err(());
        }
        for i in 0..names.len() {
            let ident = self.ast_objs.idents[names[i]].clone();
            let val = self.tlookup.get_const_value(values[i].id(), self.objects)?;
            let ident_key = ident.entity.into_key();
            self.current_func_add_const_def(ident_key.unwrap(), val);
        }
        Ok(())
    }

    /// entrance for all assign related stmts
    /// var x
    /// x := 0
    /// x += 1
    /// x++
    /// for x := range xxx
    fn gen_assign(
        &mut self,
        token: &Token,
        lhs_exprs: &Vec<&Expr>,
        rhs_exprs: &Vec<Expr>,
        range: Option<&Expr>,
    ) -> Result<Option<usize>, ()> {
        let is_def = *token == Token::DEFINE;
        let lhs = lhs_exprs
            .iter()
            .map(|expr| {
                match expr {
                    Expr::Ident(ident) => {
                        let idx = self.add_local_or_resolve_ident(ident, is_def)?;
                        Ok(LeftHandSide::Primitive(idx))
                    }
                    Expr::Index(ind_expr) => {
                        let obj = &ind_expr.as_ref().expr;
                        self.visit_expr(obj)?;
                        let obj_typ = self.tlookup.get_expr_value_type(obj);
                        let ind = &ind_expr.as_ref().index;

                        let mut index_const = None;
                        let mut index_typ = None;
                        if let Some(const_val) = self.tlookup.get_tc_const_value(ind.id()) {
                            let (ival, _) = const_val.to_int().int_as_i64();
                            if let Ok(i) = i8::try_from(ival) {
                                index_const = Some(i);
                            }
                        }
                        if index_const.is_none() {
                            self.visit_expr(ind)?;
                            index_typ = Some(self.tlookup.get_expr_value_type(ind));
                        }
                        Ok(LeftHandSide::IndexSelExpr(IndexSelInfo::new(
                            0,
                            obj_typ,
                            index_typ,
                            true,
                            index_const,
                        ))) // the true index will be calculated later
                    }
                    Expr::Selector(sexpr) => {
                        self.visit_expr(&sexpr.expr)?;
                        self.gen_push_ident_str(&sexpr.sel);
                        let obj_typ = self.tlookup.get_expr_value_type(&sexpr.expr);
                        // the true index will be calculated later
                        Ok(LeftHandSide::IndexSelExpr(IndexSelInfo::new(
                            0,
                            obj_typ,
                            Some(ValueType::Str),
                            false,
                            None,
                        )))
                    }
                    Expr::Star(sexpr) => {
                        self.visit_expr(&sexpr.expr)?;
                        Ok(LeftHandSide::Deref(0)) // the true index will be calculated later
                    }
                    _ => unreachable!(),
                }
            })
            .collect::<Result<Vec<LeftHandSide>, ()>>()?;

        let simple_op = match token {
            Token::ADD_ASSIGN | Token::INC => Some(Opcode::ADD), // +=
            Token::SUB_ASSIGN | Token::DEC => Some(Opcode::SUB), // -=
            Token::MUL_ASSIGN => Some(Opcode::MUL),              // *=
            Token::QUO_ASSIGN => Some(Opcode::QUO),              // /=
            Token::REM_ASSIGN => Some(Opcode::REM),              // %=
            Token::AND_ASSIGN => Some(Opcode::AND),              // &=
            Token::OR_ASSIGN => Some(Opcode::OR),                // |=
            Token::XOR_ASSIGN => Some(Opcode::XOR),              // ^=
            Token::SHL_ASSIGN => Some(Opcode::SHL),              // <<=
            Token::SHR_ASSIGN => Some(Opcode::SHR),              // >>=
            Token::AND_NOT_ASSIGN => Some(Opcode::AND_NOT),      // &^=

            Token::ASSIGN | Token::DEFINE => None,
            _ => unreachable!(),
        };
        if let Some(code) = simple_op {
            if *token == Token::INC || *token == Token::DEC {
                let typ = self.tlookup.get_expr_value_type(&lhs_exprs[0]);
                self.gen_op_assign(&lhs[0], code, None, typ)?;
            } else {
                assert_eq!(lhs_exprs.len(), 1);
                assert_eq!(rhs_exprs.len(), 1);
                let typ = self.tlookup.get_expr_value_type(&rhs_exprs[0]);
                self.gen_op_assign(&lhs[0], code, Some(&rhs_exprs[0]), typ)?;
            }
            Ok(None)
        } else {
            self.gen_assign_def_var(&lhs, &rhs_exprs, &None, range)
        }
    }

    fn gen_assign_def_var(
        &mut self,
        lhs: &Vec<LeftHandSide>,
        values: &Vec<Expr>,
        typ: &Option<Expr>,
        range: Option<&Expr>,
    ) -> Result<Option<usize>, ()> {
        let mut range_marker = None;
        // handle the right hand side
        let types = if let Some(r) = range {
            // the range statement
            self.visit_expr(r)?;
            let tkv = self.tlookup.get_range_value_types(r);
            let func = current_func_mut!(self);
            func.emit_inst(Opcode::PUSH_IMM, None, None, None, Some(-1));
            range_marker = Some(func.code.len());
            // the block_end address to be set
            func.emit_inst(
                Opcode::RANGE,
                Some(tkv[0]),
                Some(tkv[1]),
                Some(tkv[2]),
                None,
            );
            tkv[1..].to_vec()
        } else if values.len() == lhs.len() {
            // define or assign with values
            let mut types = Vec::with_capacity(values.len());
            for val in values.iter() {
                self.visit_expr(val)?;
                types.push(self.tlookup.get_expr_value_type(val));
            }
            types
        } else if values.len() == 0 {
            // define without values
            let (val, t) = self.get_type_default(&typ.as_ref().unwrap());
            let mut types = Vec::with_capacity(lhs.len());
            for _ in 0..lhs.len() {
                let func = current_func_mut!(self);
                let i = func.add_const(None, val.clone());
                func.emit_load(i, t);
                types.push(t);
            }
            types
        } else if values.len() == 1 {
            // define or assign with function call on the right
            if let Expr::Call(_) = values[0] {
                self.visit_expr(&values[0])?;
                self.tlookup.get_return_value_types(&values[0])
            } else {
                return Err(());
            }
        } else {
            return Err(());
        };

        // now the values should be on stack, generate code to set them to the lhs
        let func = current_func_mut!(self);
        let total_lhs_val = lhs.iter().fold(0, |acc, x| match x {
            LeftHandSide::Primitive(_) => acc,
            LeftHandSide::IndexSelExpr(info) => acc + info.stack_space(),
            LeftHandSide::Deref(_) => acc + 1,
        });
        assert_eq!(lhs.len(), types.len());
        let total_rhs_val = types.len() as OpIndex;
        let total_val = (total_lhs_val + total_rhs_val) as OpIndex;
        let mut current_indexing_index = -total_val;
        for (i, l) in lhs.iter().enumerate() {
            let val_index = i as OpIndex - total_rhs_val;
            let typ = types[i];
            match l {
                LeftHandSide::Primitive(_) => {
                    func.emit_store(l, val_index, None, typ);
                }
                LeftHandSide::IndexSelExpr(info) => {
                    func.emit_store(
                        &LeftHandSide::IndexSelExpr(info.with_index(current_indexing_index)),
                        val_index,
                        None,
                        typ,
                    );
                    // the lhs of IndexSelExpr takes two spots
                    current_indexing_index += 2;
                }
                LeftHandSide::Deref(_) => {
                    func.emit_store(
                        &LeftHandSide::Deref(current_indexing_index),
                        val_index,
                        None,
                        typ,
                    );
                    current_indexing_index += 1;
                }
            }
        }

        // pop rhs
        for t in types.iter().rev() {
            func.emit_pop(*t);
        }
        // pop lhs
        for i in lhs.iter().rev() {
            match i {
                LeftHandSide::Primitive(_) => {}
                LeftHandSide::IndexSelExpr(info) => {
                    if let Some(t) = info.t2 {
                        func.emit_pop(t);
                    }
                    func.emit_pop(info.t1);
                }
                LeftHandSide::Deref(_) => func.emit_pop(ValueType::Boxed),
            }
        }
        Ok(range_marker)
    }

    fn gen_op_assign(
        &mut self,
        left: &LeftHandSide,
        op: Opcode,
        right: Option<&Expr>,
        typ: ValueType,
    ) -> Result<(), ()> {
        if let Some(e) = right {
            self.visit_expr(e)?;
        } else {
            // it's inc/dec
            let func = current_func_mut!(self);
            func.emit_inst(Opcode::PUSH_IMM, None, None, None, Some(1));
        }
        let func = current_func_mut!(self);
        match left {
            LeftHandSide::Primitive(_) => {
                // why no magic number?
                // local index is resolved in gen_assign
                func.emit_store(left, -1, Some(op), typ);
                func.emit_pop(typ);
            }
            LeftHandSide::IndexSelExpr(info) => {
                // stack looks like this(bottom to top) :
                //  [... target, index, value] or [... target, value]
                func.emit_store(
                    &LeftHandSide::IndexSelExpr(info.with_index(-info.stack_space() - 1)),
                    -1,
                    Some(op),
                    typ,
                );
                func.emit_pop(typ);
                if let Some(t) = info.t2 {
                    func.emit_pop(t);
                }
                func.emit_pop(info.t1);
            }
            LeftHandSide::Deref(_) => {
                // why -2?  stack looks like this(bottom to top) :
                //  [... target, value]
                func.emit_store(&LeftHandSide::Deref(-2), -1, Some(op), typ);
                func.emit_pop(typ);
                func.emit_pop(ValueType::Boxed);
            }
        }
        Ok(())
    }

    fn gen_func_def(
        &mut self,
        fmeta: GosValue,
        fkey: FuncTypeKey,
        recv: Option<FieldList>,
        body: &BlockStmt,
    ) -> Result<FunctionKey, ()> {
        let typ = &self.ast_objs.ftypes[fkey];
        let sig_metadata = &self.objects.metas[*fmeta.as_meta()].sig_metadata();
        let fval = FunctionVal::new(
            self.pkg_key.clone(),
            fmeta.clone(),
            sig_metadata.variadic,
            false,
        );
        let fkey = self.objects.functions.insert(fval);
        let mut func = &mut self.objects.functions[fkey];
        func.ret_count = match &typ.results {
            Some(fl) => func.add_params(&fl, self.ast_objs)?,
            None => 0,
        };
        func.param_count = match recv {
            Some(recv) => {
                let mut fields = recv;
                fields.list.append(&mut typ.params.list.clone());
                func.add_params(&fields, self.ast_objs)?
            }
            None => func.add_params(&typ.params, self.ast_objs)?,
        };
        self.func_stack.push(fkey);
        // process function body
        self.visit_stmt_block(body)?;
        // it will not be executed if it's redundant
        self.objects.functions[fkey].emit_return();

        self.func_stack.pop();
        Ok(fkey)
    }

    fn get_use_type_meta(&mut self, expr: &Expr) -> GosValue {
        let ikey = match expr {
            Expr::Star(s) => *s.expr.try_as_ident().unwrap(),
            Expr::Ident(i) => *i,
            _ => unreachable!(),
        };
        let typ = self.tlookup.get_use_tc_type(ikey);
        match self.tc_objs.types[typ].try_as_basic() {
            Some(_) => self.tlookup.type_from_tc(typ, self.objects),
            None => {
                let i = self.resolve_ident(&ikey).unwrap();
                match i {
                    EntIndex::Const(i) => {
                        let func = current_func_mut!(self);
                        func.const_val(i.into()).clone()
                    }
                    EntIndex::PackageMember(i) => {
                        let pkg = &self.objects.packages[self.pkg_key];
                        pkg.member(i).clone()
                    }
                    _ => unreachable!(),
                }
            }
        }
    }

    fn value_from_literal(&mut self, typ: Option<&Expr>, expr: &Expr) -> Result<GosValue, ()> {
        match typ {
            Some(type_expr) => match type_expr {
                Expr::Array(_) | Expr::Map(_) | Expr::Struct(_) => {
                    self.value_from_comp_literal(type_expr, expr)
                }
                _ => self.tlookup.get_const_value(expr.id(), self.objects),
            },
            None => self.tlookup.get_const_value(expr.id(), self.objects),
        }
    }

    fn get_type_default(&mut self, expr: &Expr) -> (GosValue, ValueType) {
        let meta = self.get_use_type_meta(expr);
        let t = MetadataVal::get_value_type(&meta, &self.objects.metas);
        let meta_val = MetadataVal::get_underlying(&meta, &self.objects.metas);
        let zero_val = meta_val.zero_val().clone();
        (zero_val, t)
    }

    fn value_from_comp_literal(&mut self, typ: &Expr, expr: &Expr) -> Result<GosValue, ()> {
        match expr {
            Expr::CompositeLit(lit) => self.get_comp_value(typ, lit),
            _ => unreachable!(),
        }
    }

    fn get_comp_value(&mut self, typ: &Expr, literal: &CompositeLit) -> Result<GosValue, ()> {
        match typ {
            Expr::Array(arr) => {
                if arr.as_ref().len.is_some() {
                    // array is not supported yet
                    unimplemented!()
                }
                let vals = literal
                    .elts
                    .iter()
                    .map(|elt| self.value_from_literal(Some(&arr.as_ref().elt), elt))
                    .collect::<Result<Vec<GosValue>, ()>>()?;
                Ok(GosValue::with_slice_val(vals, &mut self.objects.slices))
            }
            Expr::Map(map) => {
                let key_vals = literal
                    .elts
                    .iter()
                    .map(|etl| {
                        if let Expr::KeyValue(kv) = etl {
                            let k = self.value_from_literal(Some(&map.key), &kv.as_ref().key)?;
                            let v = self.value_from_literal(Some(&map.val), &kv.as_ref().val)?;
                            Ok((k, v))
                        } else {
                            unreachable!()
                        }
                    })
                    .collect::<Result<Vec<(GosValue, GosValue)>, ()>>()?;
                let (val, _) = self.get_type_default(&map.val);
                let map = GosValue::new_map(val, &mut self.objects.maps);
                for kv in key_vals.iter() {
                    map.as_map().insert(kv.0.clone(), kv.1.clone());
                }
                Ok(map)
            }
            _ => unimplemented!(),
        }
    }

    fn current_func_add_const_def(&mut self, entity: EntityKey, cst: GosValue) -> EntIndex {
        let func = current_func_mut!(self);
        let index = func.add_const(Some(entity.clone()), cst.clone());
        if func.is_ctor() {
            let pkg_key = func.package;
            drop(func);
            let pkg = &mut self.objects.packages[pkg_key];
            pkg.add_member(entity, cst);
        }
        index
    }

    fn gen_push_ident_str(&mut self, ident: &IdentKey) {
        let name = self.ast_objs.idents[*ident].name.clone();
        let gos_val = GosValue::new_str(name);
        let func = current_func_mut!(self);
        let index = func.add_const(None, gos_val);
        func.emit_load(index, ValueType::Str);
    }

    fn built_in_func_index(&self, name: &str) -> Option<OpIndex> {
        self.built_in_funcs.iter().enumerate().find_map(|(i, x)| {
            if x.name == name {
                Some(i as OpIndex)
            } else {
                None
            }
        })
    }

    pub fn gen_with_files(&mut self, files: &Vec<File>, index: OpIndex) {
        let pkey = self.pkg_key;
        let fmeta = self.objects.default_sig_meta.clone().unwrap();
        let fval = FunctionVal::new(pkey, fmeta, None, true);
        let fkey = self.objects.functions.insert(fval);
        // the 0th member is the constructor
        self.objects.packages[pkey].add_member(null_key!(), GosValue::new_closure(fkey));
        self.pkg_key = pkey;
        self.func_stack.push(fkey);
        for f in files.iter() {
            for d in f.decls.iter() {
                if let Err(_) = self.visit_decl(d) {
                    dbg!("error when generating");
                }
            }
        }
        let func = &mut self.objects.functions[fkey];
        func.emit_return_init_pkg(index);
        self.func_stack.pop();
    }
}
