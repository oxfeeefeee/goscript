#![allow(dead_code)]
#[macro_use]
use super::opcode::*;
use super::prim_ops::PrimOps;
use super::types::Objects as VMObjects;
use super::types::*;
use super::vm::UpValue;
use std::collections::HashMap;
use std::convert::TryInto;
use std::fs;

use goscript_frontend::ast::*;
use goscript_frontend::ast_objects::Objects as AstObjects;
use goscript_frontend::ast_objects::*;
use goscript_frontend::errors::{ErrorList, FilePosErrors};
use goscript_frontend::position;
use goscript_frontend::token::Token;
use goscript_frontend::visitor::{walk_decl, walk_expr, walk_stmt, Visitor};
use goscript_frontend::{FileSet, Parser};

macro_rules! current_func_mut {
    ($owner:ident) => {
        &mut $owner.objects.functions[*$owner.func_stack.last().unwrap()]
    };
}

// ----------------------------------------------------------------------------
// Package
#[derive(Clone, Debug)]
pub struct Package {
    name: String,
    members: Vec<GosValue>, // imports, const, var, func are all stored here
    member_indices: HashMap<EntityKey, OpIndex>,
    main_func_index: Option<usize>,
    // maps func_member_index of the constructor to pkg_member_index
    var_mapping: Option<HashMap<OpIndex, OpIndex>>,
}

impl Package {
    fn new(name: String) -> Package {
        Package {
            name: name,
            members: Vec::new(),
            member_indices: HashMap::new(),
            main_func_index: None,
            var_mapping: Some(HashMap::new()),
        }
    }

    fn add_member(&mut self, entity: EntityKey, val: GosValue) -> OpIndex {
        self.members.push(val);
        let index = (self.members.len() - 1) as OpIndex;
        self.member_indices.insert(entity, index);
        index as OpIndex
    }

    // add placeholder for vars, will be initialized when imported
    fn add_var(&mut self, entity: EntityKey, fn_index: EntIndex) -> OpIndex {
        let index = self.add_member(entity, GosValue::Nil);
        self.var_mapping
            .as_mut()
            .unwrap()
            .insert(fn_index.into(), index);
        index
    }

    pub fn init_var(&mut self, fn_member_index: &OpIndex, val: GosValue) {
        let index = self.var_mapping.as_ref().unwrap()[fn_member_index];
        self.members[index as usize] = val;
    }

    pub fn var_count(&self) -> usize {
        self.var_mapping.as_ref().unwrap().len()
    }

    fn set_main_func(&mut self, index: OpIndex) {
        self.main_func_index = Some(index as usize);
    }

    fn member_index(&self, entity: &EntityKey) -> OpIndex {
        self.member_indices[entity]
    }

    pub fn inited(&self) -> bool {
        self.var_mapping.is_none()
    }

    pub fn set_inited(&mut self) {
        self.var_mapping = None
    }

    // pass negative index for main func
    pub fn member(&self, i: OpIndex) -> GosValue {
        if i >= 0 {
            self.members[i as usize]
        } else {
            self.members[self.main_func_index.unwrap()]
        }
    }

    pub fn set_member(&mut self, i: OpIndex, val: GosValue) {
        self.members[i as usize] = val;
    }
}

// ----------------------------------------------------------------------------
// LeftHandSide

#[derive(Clone, Debug)]
enum LeftHandSide {
    Primitive(EntIndex),
    IndexSelExpr(OpIndex),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum EntIndex {
    Const(OpIndex),
    LocalVar(OpIndex),
    UpValue(OpIndex),
    PackageMember(OpIndex),
    Global(OpIndex),
    Blank,
}

impl From<EntIndex> for OpIndex {
    fn from(t: EntIndex) -> OpIndex {
        match t {
            EntIndex::Const(i) => i,
            EntIndex::LocalVar(i) => i,
            EntIndex::UpValue(i) => i,
            EntIndex::PackageMember(i) => i,
            EntIndex::Global(i) => i,
            EntIndex::Blank => unreachable!(),
        }
    }
}

// ----------------------------------------------------------------------------
// Function

#[derive(Clone, Debug)]
pub struct Function {
    pub package: PackageKey,
    pub typ: TypeKey,
    pub code: Vec<CodeData>,
    pub consts: Vec<GosValue>,
    pub up_ptrs: Vec<UpValue>,
    // param_count, ret_count can be read from typ,
    // these fields are for faster access
    param_count: usize,
    ret_count: usize,
    entities: HashMap<EntityKey, EntIndex>,
    local_alloc: u16,
    is_ctor: bool,
}

impl Function {
    fn new(package: PackageKey, typ: TypeKey, ctor: bool) -> Function {
        Function {
            package: package,
            typ: typ,
            code: Vec::new(),
            consts: Vec::new(),
            up_ptrs: Vec::new(),
            param_count: 0,
            ret_count: 0,
            entities: HashMap::new(),
            local_alloc: 0,
            is_ctor: ctor,
        }
    }

    pub fn ret_count(&self) -> usize {
        self.ret_count
    }

    pub fn local_count(&self) -> usize {
        self.local_alloc as usize - self.param_count - self.ret_count
    }

    fn entity_index(&self, entity: &EntityKey) -> EntIndex {
        self.entities.get(entity).unwrap().clone()
    }

    fn const_val(&self, index: OpIndex) -> &GosValue {
        &self.consts[index as usize]
    }

    // for unnamed return values, entity == None
    fn add_local(&mut self, entity: Option<EntityKey>) -> EntIndex {
        let result = self.local_alloc as OpIndex;
        if let Some(key) = entity {
            let old = self.entities.insert(key, EntIndex::LocalVar(result));
            assert_eq!(old, None);
        };
        self.local_alloc += 1;
        EntIndex::LocalVar(result)
    }

    fn add_const(&mut self, entity: Option<EntityKey>, cst: GosValue) -> EntIndex {
        // todo: filter out duplicated consts
        self.consts.push(cst);
        let result = (self.consts.len() - 1).try_into().unwrap();
        if let Some(key) = entity {
            let old = self.entities.insert(key, EntIndex::Const(result));
            assert_eq!(old, None);
        }
        EntIndex::Const(result)
    }

    fn add_const_def(&mut self, entity: EntityKey, cst: GosValue, pkg: &mut Package) -> EntIndex {
        let index = self.add_const(Some(entity.clone()), cst);
        if self.is_ctor {
            pkg.add_member(entity, cst);
        }
        index
    }

    fn try_add_upvalue(&mut self, entity: &EntityKey, uv: UpValue) -> EntIndex {
        self.entities
            .get(entity)
            .map(|x| *x)
            .or_else(|| {
                self.up_ptrs.push(uv);
                let i = (self.up_ptrs.len() - 1).try_into().ok();
                let et = EntIndex::UpValue(i.unwrap());
                self.entities.insert(*entity, et);
                i.map(|x| EntIndex::UpValue(x))
            })
            .unwrap()
    }

    fn add_params<'e>(
        &mut self,
        fl: &FieldList,
        o: &AstObjects,
        _errors: &FilePosErrors<'e>,
    ) -> usize {
        fl.list
            .iter()
            .map(|f| {
                let names = &o.fields[*f].names;
                if names.len() == 0 {
                    self.add_local(None);
                    1
                } else {
                    names
                        .iter()
                        .map(|n| {
                            let ident = &o.idents[*n];
                            self.add_local(ident.entity.clone().into_key());
                        })
                        .count()
                }
            })
            .sum()
    }

    fn emit_load(&mut self, index: EntIndex) {
        match index {
            EntIndex::Const(i) => {
                // todo: optimizaiton, replace PUSH_CONST with PUSH_NIL/_TRUE/_FALSE/_SHORT
                self.code.push(CodeData::Code(Opcode::PUSH_CONST));
                self.code.push(CodeData::Data(i));
            }
            EntIndex::LocalVar(i) => {
                let code = Opcode::get_load_local(i);
                self.code.push(CodeData::Code(code));
                if let Opcode::LOAD_LOCAL = code {
                    self.code.push(CodeData::Data(i));
                }
            }
            EntIndex::UpValue(i) => {
                self.code.push(CodeData::Code(Opcode::LOAD_UPVALUE));
                self.code.push(CodeData::Data(i));
            }
            EntIndex::PackageMember(i) => {
                self.code.push(CodeData::Code(Opcode::LOAD_THIS_PKG_FIELD));
                self.code.push(CodeData::Data(i));
            }
            EntIndex::Global(i) => {
                self.code.push(CodeData::Code(Opcode::LOAD_GLOBAL));
                self.code.push(CodeData::Data(i));
            }
            EntIndex::Blank => unreachable!(),
        }
    }

    fn emit_store(&mut self, lhs: &LeftHandSide, stack_index: OpIndex) {
        match lhs {
            LeftHandSide::Primitive(index) => match index {
                EntIndex::Blank => {}
                _ => {
                    let (code, i) = match index {
                        EntIndex::Const(_) => unreachable!(),
                        EntIndex::LocalVar(i) => (
                            if stack_index == 0 {
                                Opcode::STORE_LOCAL
                            } else {
                                Opcode::STORE_LOCAL_NT
                            },
                            i,
                        ),
                        EntIndex::UpValue(i) => (
                            if stack_index == 0 {
                                Opcode::STORE_UPVALUE
                            } else {
                                Opcode::STORE_UPVALUE_NT
                            },
                            i,
                        ),
                        EntIndex::PackageMember(_) => unimplemented!(),
                        EntIndex::Global(_) => unreachable!(),
                        EntIndex::Blank => unreachable!(),
                    };
                    self.code.push(CodeData::Code(code));
                    self.code.push(CodeData::Data(*i));
                    if stack_index < 0 {
                        self.code.push(CodeData::Data(stack_index));
                    }
                }
            },
            LeftHandSide::IndexSelExpr(i) => {
                let code = if stack_index == 0 {
                    Opcode::STORE_FIELD
                } else {
                    Opcode::STORE_FIELD_NT
                };
                self.code.push(CodeData::Code(code));
                self.code.push(CodeData::Data(*i));
                if stack_index < 0 {
                    self.code.push(CodeData::Data(stack_index));
                }
            }
        }
    }

    fn emit_import(&mut self, index: OpIndex) {
        self.emit_code(Opcode::IMPORT);
        self.emit_data(index);
        self.emit_code(Opcode::JUMP_IF);
        let mut cd = vec![
            CodeData::Code(Opcode::PUSH_IMM),
            CodeData::Data(0 as OpIndex),
            CodeData::Code(Opcode::LOAD_FIELD),
            CodeData::Code(Opcode::PRE_CALL),
            CodeData::Code(Opcode::CALL),
            CodeData::Code(Opcode::INIT_PKG),
            CodeData::Data(index),
        ];
        self.emit_data(cd.len() as OpIndex);
        self.code.append(&mut cd);
    }

    fn emit_code(&mut self, code: Opcode) {
        self.code.push(CodeData::Code(code));
    }

    fn emit_data(&mut self, data: OpIndex) {
        self.code.push(CodeData::Data(data));
    }

    fn emit_pop(&mut self) {
        self.emit_code(Opcode::POP);
    }

    fn emit_binary_prim_call(&mut self, prim: PrimOps) {
        self.emit_code(Opcode::CALL_PRIM_2_1);
        self.emit_data(prim as OpIndex);
    }

    fn emit_load_field(&mut self) {
        self.emit_code(Opcode::LOAD_FIELD);
    }

    fn emit_load_field_imm(&mut self, imm: OpIndex) {
        self.emit_code(Opcode::LOAD_FIELD_IMM);
        self.emit_data(imm);
    }

    fn emit_return(&mut self) {
        self.emit_code(Opcode::RETURN);
    }

    fn emit_pre_call(&mut self) {
        self.emit_code(Opcode::PRE_CALL);
    }

    fn emit_call(&mut self) {
        self.code.push(CodeData::Code(Opcode::CALL));
    }

    fn emit_new_closure(&mut self) {
        self.emit_code(Opcode::NEW_CLOSURE);
    }
}

// ----------------------------------------------------------------------------
// CodeGen
pub struct CodeGen<'a> {
    objects: VMObjects,
    ast_objs: &'a AstObjects,
    package_indices: HashMap<String, OpIndex>,
    packages: Vec<PackageKey>,
    current_pkg: PackageKey,
    func_stack: Vec<FunctionKey>,
    errors: &'a FilePosErrors<'a>,
}

impl<'a> Visitor for CodeGen<'a> {
    fn visit_expr(&mut self, expr: &Expr) {
        walk_expr(self, expr);
    }

    fn visit_stmt(&mut self, stmt: &Stmt) {
        walk_stmt(self, stmt);
    }

    fn visit_decl(&mut self, decl: &Decl) {
        walk_decl(self, decl);
    }

    fn visit_expr_ident(&mut self, ident: &IdentKey) {
        let index = self.resolve_ident(ident);
        current_func_mut!(self).emit_load(index);
    }

    fn visit_expr_option(&mut self, op: &Option<Expr>) {
        unimplemented!();
    }

    fn visit_expr_ellipsis(&mut self) {
        unimplemented!();
    }

    fn visit_expr_basic_lit(&mut self, blit: &BasicLit) {
        let val = self.get_const_value(None, &blit);
        let func = current_func_mut!(self);
        let i = func.add_const(None, val);
        func.emit_load(i);
    }

    /// Add function as a const and then generate a closure of it
    fn visit_expr_func_lit(&mut self, flit: &FuncLit) {
        let fkey = self.gen_func_def(&flit.typ, &flit.body);
        let func = current_func_mut!(self);
        let i = func.add_const(None, GosValue::Function(fkey));
        func.emit_load(i);
        func.emit_new_closure();
    }

    fn visit_expr_composit_lit(&mut self, clit: &CompositeLit) {
        let val = self.get_comp_value(clit.typ.as_ref().unwrap(), &clit);
        let func = current_func_mut!(self);
        let i = func.add_const(None, val);
        func.emit_load(i);
    }

    fn visit_expr_paren(&mut self) {
        unimplemented!();
    }

    fn visit_expr_selector(&mut self, ident: &IdentKey) {
        // todo: use index instead of string when Type Checker is in place
        self.gen_push_ident_str(ident);
        current_func_mut!(self).emit_load_field();
    }

    fn visit_expr_index(&mut self) {
        current_func_mut!(self).emit_load_field();
    }

    fn visit_expr_slice(&mut self) {
        unimplemented!();
    }

    fn visit_expr_type_assert(&mut self) {
        unimplemented!();
    }

    fn visit_expr_call(&mut self, args: &Vec<Expr>) {
        current_func_mut!(self).emit_pre_call();
        let _count = args.iter().map(|e| self.visit_expr(e)).count();
        current_func_mut!(self).emit_call();
    }

    fn visit_expr_star(&mut self) {
        unimplemented!();
    }

    fn visit_expr_unary(&mut self, op: &Token) {
        unimplemented!();
    }

    fn visit_expr_binary(&mut self, op: &Token) {
        let primi = match op {
            Token::ADD => PrimOps::Add,
            Token::SUB => PrimOps::Sub,
            _ => unimplemented!(),
        };
        current_func_mut!(self).emit_binary_prim_call(primi);
    }

    fn visit_expr_key_value(&mut self) {
        unimplemented!();
    }

    fn visit_expr_array_type(&mut self) {
        unimplemented!();
    }

    fn visit_expr_slice_type(&mut self) {
        unimplemented!();
    }

    fn visit_expr_struct_type(&mut self, s: &StructType) {
        unimplemented!();
    }

    fn visit_expr_func_type(&mut self, s: &FuncType) {
        unimplemented!();
    }

    fn visit_expr_interface_type(&mut self, s: &InterfaceType) {
        unimplemented!();
    }

    fn visit_map_type(&mut self) {
        unimplemented!();
    }

    fn visit_chan_type(&mut self, dir: &ChanDir) {
        unimplemented!();
    }

    fn visit_stmt_decl_gen(&mut self, gdecl: &GenDecl) {
        for s in gdecl.specs.iter() {
            let spec = &self.ast_objs.specs[*s];
            match spec {
                Spec::Import(_is) => unimplemented!(),
                Spec::Type(ts) => {
                    let ident = self.ast_objs.idents[ts.name].clone();
                    let ident_key = ident.entity.into_key();
                    let typ = self.get_or_gen_type(&ts.typ);
                    let func = current_func_mut!(self);
                    let pkg = &mut self.objects.packages[func.package];
                    func.add_const_def(ident_key.unwrap(), typ, pkg);
                }
                Spec::Value(vs) => {
                    if gdecl.token == Token::VAR {
                        let pos = self.ast_objs.idents[vs.names[0]].pos;
                        let lhs = vs
                            .names
                            .iter()
                            .map(|n| {
                                LeftHandSide::Primitive(self.add_local_or_resolve_ident(n, true))
                            })
                            .collect();
                        self.gen_assign_def_var(&lhs, &vs.values, &vs.typ, pos);
                    } else {
                        assert!(gdecl.token == Token::CONST);
                        self.gen_def_const(&vs.names, &vs.values, &vs.typ);
                    }
                }
            }
        }
    }

    fn visit_stmt_decl_func(&mut self, fdecl: &FuncDeclKey) {
        let decl = &self.ast_objs.decls[*fdecl];
        if decl.body.is_none() {
            unimplemented!()
        }
        let stmt = decl.body.as_ref().unwrap();
        let typ = &decl.typ;
        let fkey = self.gen_func_def(typ, stmt);

        let ident = &self.ast_objs.idents[decl.name];
        let pkg = &mut self.objects.packages[self.current_pkg];
        let index = pkg.add_member(ident.entity_key().unwrap(), GosValue::Function(fkey));
        if ident.name == "main" {
            pkg.set_main_func(index);
        }
    }

    fn visit_stmt_labeled(&mut self, lstmt: &LabeledStmtKey) {
        unimplemented!();
    }

    fn visit_stmt_send(&mut self, sstmt: &SendStmt) {
        unimplemented!();
    }

    fn visit_stmt_incdec(&mut self, idcstmt: &IncDecStmt) {
        unimplemented!();
    }

    fn visit_stmt_assign(&mut self, astmt: &AssignStmtKey) {
        let stmt = &self.ast_objs.a_stmts[*astmt];
        let is_def = stmt.token == Token::DEFINE;
        let pos = stmt.lhs[0].pos(&self.ast_objs);
        let lhs: Vec<LeftHandSide> = stmt
            .lhs
            .iter()
            .map(|expr| match expr {
                Expr::Ident(ident) => {
                    let idx = self.add_local_or_resolve_ident(ident.as_ref(), is_def);
                    LeftHandSide::Primitive(idx)
                }
                Expr::Index(ind_expr) => {
                    self.visit_expr(&ind_expr.as_ref().expr);
                    self.visit_expr(&ind_expr.as_ref().index);
                    LeftHandSide::IndexSelExpr(0) // the true index will be calculated later
                }
                Expr::Selector(sexpr) => {
                    self.visit_expr(&sexpr.expr);
                    self.gen_push_ident_str(&sexpr.sel);
                    LeftHandSide::IndexSelExpr(0) // the true index will be calculated later
                }
                _ => unreachable!(),
            })
            .collect();
        self.gen_assign_def_var(&lhs, &stmt.rhs, &None, pos);
    }

    fn visit_stmt_go(&mut self, gostmt: &GoStmt) {
        unimplemented!();
    }

    fn visit_stmt_defer(&mut self, dstmt: &DeferStmt) {
        unimplemented!();
    }

    fn visit_stmt_return(&mut self, rstmt: &ReturnStmt) {
        for (i, expr) in rstmt.results.iter().enumerate() {
            self.visit_expr(expr);
            let f = current_func_mut!(self);
            f.emit_store(
                &LeftHandSide::Primitive(EntIndex::LocalVar(i as OpIndex)),
                0,
            );
            f.emit_pop();
        }
        current_func_mut!(self).emit_return();
    }

    fn visit_stmt_branch(&mut self, bstmt: &BranchStmt) {
        unimplemented!();
    }

    fn visit_stmt_block(&mut self, bstmt: &BlockStmt) {
        dbg!(bstmt);
        for stmt in bstmt.list.iter() {
            self.visit_stmt(stmt);
        }
    }

    fn visit_stmt_if(&mut self, ifstmt: &IfStmt) {
        //dbg!(ifstmt);
    }

    fn visit_stmt_case(&mut self, cclause: &CaseClause) {
        unimplemented!();
    }

    fn visit_stmt_switch(&mut self, sstmt: &SwitchStmt) {
        unimplemented!();
    }

    fn visit_stmt_type_switch(&mut self, tstmt: &TypeSwitchStmt) {
        unimplemented!();
    }

    fn visit_stmt_comm(&mut self, cclause: &CommClause) {
        unimplemented!();
    }

    fn visit_stmt_select(&mut self, sstmt: &SelectStmt) {
        unimplemented!();
    }

    fn visit_stmt_for(&mut self, fstmt: &ForStmt) {
        unimplemented!();
    }

    fn visit_stmt_range(&mut self, rstmt: &RangeStmt) {
        unimplemented!();
    }
}

impl<'a> CodeGen<'a> {
    pub fn new(aobjects: &'a AstObjects, err: &'a FilePosErrors) -> CodeGen<'a> {
        CodeGen {
            objects: VMObjects::new(),
            ast_objs: aobjects,
            package_indices: HashMap::new(),
            packages: Vec::new(),
            current_pkg: slotmap::Key::null(),
            func_stack: Vec::new(),
            errors: err,
        }
    }

    fn resolve_ident(&mut self, ident: &IdentKey) -> EntIndex {
        // 1. try local frist
        let id = &self.ast_objs.idents[*ident];
        let entity_key = id.entity_key().unwrap();
        let local = current_func_mut!(self)
            .entities
            .get(&entity_key)
            .map(|x| *x);
        if local.is_some() {
            return local.unwrap();
        }
        // 2. try upvalue
        let upvalue = self
            .func_stack
            .clone()
            .iter()
            .skip(1) // skip constructor
            .rev()
            .skip(1) // skip itself
            .find_map(|ifunc| {
                let f = &mut self.objects.functions[*ifunc];
                let index = f.entities.get(&entity_key).map(|x| *x);
                if let Some(ind) = index {
                    Some(UpValue::Open(*ifunc, ind.into()))
                } else {
                    None
                }
            });
        if let Some(uv) = upvalue {
            let func = current_func_mut!(self);
            let index = func.try_add_upvalue(&entity_key, uv);
            return index;
        }
        // 3. try the package level
        let pkg = &self.objects.packages[self.current_pkg];
        EntIndex::PackageMember(pkg.member_index(&entity_key))
    }

    fn add_local_or_resolve_ident(&mut self, ikey: &IdentKey, is_def: bool) -> EntIndex {
        let ident = self.ast_objs.idents[*ikey].clone();
        let func = current_func_mut!(self);
        if ident.is_blank() {
            EntIndex::Blank
        } else if is_def {
            let ident_key = ident.entity.into_key();
            let index = func.add_local(ident_key.clone());
            if func.is_ctor {
                dbg!(func.package);
                let pkg = &mut self.objects.packages[func.package];
                pkg.add_var(ident_key.unwrap(), index);
            }
            index
        } else {
            self.resolve_ident(ikey)
        }
    }

    fn gen_def_const(&mut self, names: &Vec<IdentKey>, values: &Vec<Expr>, typ: &Option<Expr>) {
        if names.len() != values.len() {
            let ident = &self.ast_objs.idents[names[0]];
            self.error_mismatch(ident.pos, names.len(), values.len());
            return;
        }
        for i in 0..names.len() {
            let ident = self.ast_objs.idents[names[i]].clone();
            let val = self.value_from_basic_literal(typ.as_ref(), &values[i]);
            let func = current_func_mut!(self);
            let ident_key = ident.entity.into_key();
            let pkg = &mut self.objects.packages[func.package];
            func.add_const_def(ident_key.unwrap(), val, pkg);
        }
    }

    fn gen_assign_def_var(
        &mut self,
        lhs: &Vec<LeftHandSide>,
        values: &Vec<Expr>,
        typ: &Option<Expr>,
        pos: position::Pos,
    ) {
        // handle the right hand side
        if values.len() == lhs.len() {
            for val in values.iter() {
                self.visit_expr(val);
            }
        } else if values.len() == 0 {
            let val = self.get_type_default(&typ.as_ref().unwrap());
            for _ in 0..lhs.len() {
                let func = current_func_mut!(self);
                let i = func.add_const(None, val);
                func.emit_load(i);
            }
        } else if values.len() == 1 {
            // function call
            if let Expr::Call(_) = values[0] {
                self.visit_expr(&values[0]);
            } else {
                self.error_mismatch(pos, lhs.len(), values.len());
            }
        } else {
            self.error_mismatch(pos, lhs.len(), values.len());
        }

        // now the values should be on stack, generate code to set them to the lhs
        let func = current_func_mut!(self);
        let total_lhs_val = lhs.iter().fold(0, |acc, x| match x {
            LeftHandSide::Primitive(_) => acc,
            LeftHandSide::IndexSelExpr(_) => acc + 2,
        });
        let total_rhs_val = lhs.len() as i16;
        let total_val = (total_lhs_val + total_rhs_val) as i16;
        let mut current_indexing_index = 1 - total_val;
        for (i, l) in lhs.iter().enumerate() {
            let val_index = i as i16 + 1 - total_rhs_val;
            match l {
                LeftHandSide::Primitive(_) => {
                    func.emit_store(l, val_index);
                }
                LeftHandSide::IndexSelExpr(_) => {
                    func.emit_store(
                        &LeftHandSide::IndexSelExpr(current_indexing_index),
                        val_index,
                    );
                    current_indexing_index += 2;
                }
            }
        }
        for _ in 0..total_val {
            func.emit_pop();
        }
    }

    fn gen_func_def(&mut self, typ: &FuncType, body: &BlockStmt) -> FunctionKey {
        let ftype = self.get_or_gen_type(&Expr::Func(Box::new(typ.clone())));
        let mut func = Function::new(self.current_pkg.clone(), *ftype.get_type(), false);
        func.ret_count = match &typ.results {
            Some(fl) => func.add_params(&fl, self.ast_objs, self.errors),
            None => 0,
        };
        func.param_count = func.add_params(&typ.params, self.ast_objs, self.errors);

        let fkey = self.objects.functions.insert(func);
        self.func_stack.push(fkey.clone());
        // process function body
        self.visit_stmt_block(body);
        // it will not be executed if it's redundant
        self.objects.functions[fkey].emit_return();

        self.func_stack.pop();
        fkey
    }

    fn value_from_literal(&mut self, typ: Option<&Expr>, expr: &Expr) -> GosValue {
        match typ {
            Some(type_expr) => match type_expr {
                Expr::Array(_) | Expr::Map(_) | Expr::Struct(_) => {
                    self.value_from_comp_literal(type_expr, expr)
                }
                _ => self.value_from_basic_literal(typ, expr),
            },
            None => self.value_from_basic_literal(None, expr),
        }
    }

    fn value_from_basic_literal(&mut self, typ: Option<&Expr>, expr: &Expr) -> GosValue {
        match expr {
            Expr::BasicLit(lit) => self.get_const_value(typ, lit),
            _ => {
                self.errors.add_str(
                    expr.pos(self.ast_objs),
                    "complex constant not supported yet",
                );
                GosValue::Nil
            }
        }
    }

    // this is a simplified version of Go's constant evaluation, which needs to be implemented
    // as part of the Type Checker
    fn get_const_value(&mut self, typ: Option<&Expr>, blit: &BasicLit) -> GosValue {
        let type_val = typ.as_ref().map(|e| self.get_type_default(e));
        if type_val.is_none() {
            match &blit.token {
                Token::INT(i) => GosValue::Int(i.parse::<isize>().unwrap()),
                Token::FLOAT(f) => GosValue::Float64(f.parse::<f64>().unwrap()),
                Token::IMAG(_) => unimplemented!(),
                Token::CHAR(c) => GosValue::Int(c.chars().skip(1).next().unwrap() as isize),
                Token::STRING(s) => GosValue::new_str(s.to_string(), &mut self.objects),
                _ => unreachable!(),
            }
        } else {
            match (type_val.unwrap(), &blit.token) {
                (GosValue::Int(_), Token::FLOAT(f)) => {
                    let fval = f.parse::<f64>().unwrap();
                    if fval.fract() != 0.0 {
                        self.errors
                            .add(blit.pos, format!("constant {} truncated to integer", f));
                        GosValue::Nil
                    } else if (fval.round() as isize) > std::isize::MAX
                        || (fval.round() as isize) < std::isize::MIN
                    {
                        self.errors.add(blit.pos, format!("{} overflows int", f));
                        GosValue::Nil
                    } else {
                        GosValue::Int(fval.round() as isize)
                    }
                }
                (GosValue::Int(_), Token::INT(ilit)) => match ilit.parse::<isize>() {
                    Ok(i) => GosValue::Int(i),
                    Err(_) => {
                        self.errors.add(blit.pos, format!("{} overflows int", ilit));
                        GosValue::Nil
                    }
                },
                (GosValue::Int(_), Token::CHAR(c)) => {
                    GosValue::Int(c.chars().skip(1).next().unwrap() as isize)
                }
                (GosValue::Float64(_), Token::FLOAT(f)) => {
                    GosValue::Float64(f.parse::<f64>().unwrap())
                }
                (GosValue::Float64(_), Token::INT(i)) => {
                    GosValue::Float64(i.parse::<f64>().unwrap())
                }
                (GosValue::Float64(_), Token::CHAR(c)) => {
                    GosValue::Float64(c.chars().skip(1).next().unwrap() as isize as f64)
                }
                (GosValue::Str(_), Token::STRING(s)) => {
                    GosValue::new_str(s.to_string(), &mut self.objects)
                }
                (_, _) => {
                    self.errors.add_str(blit.pos, "invalid constant literal");
                    GosValue::Nil
                }
            }
        }
    }

    fn get_or_gen_type(&mut self, expr: &Expr) -> GosValue {
        match expr {
            Expr::Ident(ikey) => {
                let ident = &self.ast_objs.idents[*ikey.as_ref()];
                match self.objects.basic_type(&ident.name) {
                    Some(val) => val.clone(),
                    None => {
                        let i = self.resolve_ident(ikey);
                        match i {
                            EntIndex::Const(i) => {
                                let func = current_func_mut!(self);
                                func.const_val(i.into()).clone()
                            }
                            EntIndex::PackageMember(i) => {
                                let pkg = &self.objects.packages[self.current_pkg];
                                pkg.member(i)
                            }
                            _ => unreachable!(),
                        }
                    }
                }
            }
            Expr::Array(atype) => {
                let vtype = self.get_or_gen_type(&atype.elt);
                GosType::new_slice(vtype, &mut self.objects)
            }
            Expr::Map(mtype) => {
                let ktype = self.get_or_gen_type(&mtype.key);
                let vtype = self.get_or_gen_type(&mtype.val);
                GosType::new_map(ktype, vtype, &mut self.objects)
            }
            Expr::Struct(stype) => {
                let mut fields = Vec::new();
                let mut map = HashMap::<String, usize>::new();
                let mut i = 0;
                for f in stype.fields.list.iter() {
                    let field = &self.ast_objs.fields[*f];
                    let typ = self.get_or_gen_type(&field.typ);
                    for name in &field.names {
                        fields.push(typ);
                        map.insert(self.ast_objs.idents[*name].name.clone(), i);
                        i += 1;
                    }
                }
                GosType::new_struct(fields, map, &mut self.objects)
            }
            Expr::Interface(itype) => {
                let methods = itype
                    .methods
                    .list
                    .iter()
                    .map(|x| {
                        let field = &self.ast_objs.fields[*x];
                        self.get_or_gen_type(&field.typ)
                    })
                    .collect();
                GosType::new_interface(methods, &mut self.objects)
            }
            Expr::Func(ftype) => {
                let params: Vec<GosValue> = ftype
                    .params
                    .list
                    .iter()
                    .map(|x| {
                        let field = &self.ast_objs.fields[*x];
                        self.get_or_gen_type(&field.typ)
                    })
                    .collect();
                let results: Vec<GosValue> = match &ftype.results {
                    Some(re) => re
                        .list
                        .iter()
                        .map(|x| {
                            let field = &self.ast_objs.fields[*x];
                            self.get_or_gen_type(&field.typ)
                        })
                        .collect(),
                    None => Vec::new(),
                };
                GosType::new_closure(params, results, &mut self.objects)
            }
            Expr::Chan(_ctype) => unimplemented!(),
            _ => unreachable!(),
        }
    }

    fn get_type_default(&mut self, expr: &Expr) -> GosValue {
        let typ = self.get_or_gen_type(expr);
        let typ_val = &self.objects.types[*typ.get_type()];
        dbg!(typ_val);
        typ_val.zero_val().clone()
    }

    fn value_from_comp_literal(&mut self, typ: &Expr, expr: &Expr) -> GosValue {
        match expr {
            Expr::CompositeLit(lit) => self.get_comp_value(typ, lit),
            _ => unreachable!(),
        }
    }

    fn get_comp_value(&mut self, typ: &Expr, literal: &CompositeLit) -> GosValue {
        match typ {
            Expr::Array(arr) => {
                if arr.as_ref().len.is_some() {
                    // array is not supported yet
                    unimplemented!()
                }
                let mut vals: Vec<GosValue> = literal
                    .elts
                    .iter()
                    .map(|elt| self.value_from_literal(Some(&arr.as_ref().elt), elt))
                    .collect();
                let slice = self.objects.new_slice(literal.elts.len());
                let slice_val = &mut self.objects.slices[*slice.get_slice()];
                slice_val.append(&mut vals);
                slice
            }
            Expr::Map(map) => {
                let key_vals: Vec<(GosValue, GosValue)> = literal
                    .elts
                    .iter()
                    .map(|etl| {
                        if let Expr::KeyValue(kv) = etl {
                            (
                                self.value_from_literal(Some(&map.key), &kv.as_ref().key),
                                self.value_from_literal(Some(&map.val), &kv.as_ref().val),
                            )
                        } else {
                            unreachable!()
                        }
                    })
                    .collect();
                let val = self.get_type_default(&map.val);
                let map = self.objects.new_map(val);
                for kv in key_vals.iter() {
                    self.objects.maps[*map.get_map()].insert(kv.0, kv.1);
                }
                map
            }
            _ => unimplemented!(),
        }
    }

    fn gen_push_ident_str(&mut self, ident: &IdentKey) {
        let name = self.ast_objs.idents[*ident].name.clone();
        let gosval = GosValue::new_str(name, &mut self.objects);
        let func = current_func_mut!(self);
        let index = func.add_const(None, gosval);
        func.emit_load(index);
    }

    fn error_mismatch(&self, pos: position::Pos, l: usize, r: usize) {
        self.errors.add(
            pos,
            format!("assignment mismatch: {} variables but {} values", l, r),
        )
    }

    fn error_type(&self, pos: position::Pos, msg: &str) {
        self.errors.add(
            pos,
            format!(
                "type error(should be caught by Type Checker when it's in place): {}",
                msg
            ),
        )
    }

    fn gen(&mut self, f: File) {
        let pkg_name = &self.ast_objs.idents[f.name].name;
        let pkg_val = Package::new(pkg_name.clone());
        let pkey = self.objects.packages.insert(pkg_val);
        let ftype = self.objects.default_closure_type.unwrap();
        let ctor_func = Function::new(pkey, *ftype.get_type(), true);
        let fkey = self.objects.functions.insert(ctor_func);
        // the 0th member is the constructor
        self.objects.packages[pkey].add_member(slotmap::Key::null(), GosValue::Function(fkey));

        self.packages.push(pkey);
        let index = self.packages.len() as i16 - 1;
        self.package_indices.insert(pkg_name.clone(), index);
        self.current_pkg = pkey;

        self.func_stack.push(fkey.clone());
        for d in f.decls.iter() {
            self.visit_decl(d)
        }
        let func = &mut self.objects.functions[fkey];
        func.emit_return();
        // set the return count as pkg's variable count, so that when the construction
        // is finished running, the values are left on the stack
        func.ret_count = self.objects.packages[pkey].var_count();
        self.func_stack.pop();
    }

    // generate the entry function for ByteCode
    fn gen_entry(&mut self) -> FunctionKey {
        // import the 0th pkg and call the main function of the pkg
        let ftype = self.objects.default_closure_type.unwrap();
        let mut func = Function::new(slotmap::Key::null(), *ftype.get_type(), false);
        func.emit_import(0);
        func.emit_code(Opcode::PUSH_IMM);
        func.emit_data(-1);
        func.emit_load_field();
        func.emit_pre_call();
        func.emit_call();
        func.emit_return();
        self.objects.functions.insert(func)
    }

    pub fn into_byte_code(mut self) -> ByteCode {
        let entry = self.gen_entry();
        ByteCode {
            objects: self.objects,
            package_indices: self.package_indices,
            packages: self.packages,
            entry: entry,
        }
    }

    pub fn load_parse_gen(path: &str, trace: bool) -> ByteCode {
        let mut astobjs = AstObjects::new();
        let mut fset = FileSet::new();
        let el = ErrorList::new();
        let src = fs::read_to_string(path).expect("read file err: ");
        let pfile = fset.add_file(path, None, src.chars().count());
        let afile = {
            let mut p = Parser::new(&mut astobjs, pfile, &el, &src, trace);
            let f = p.parse_file();
            print!("\n<- {} ->\n", el);
            f
        };
        let pos_err = FilePosErrors::new(pfile, &el);
        let mut code_gen = CodeGen::new(&astobjs, &pos_err);
        code_gen.gen(afile.unwrap());
        print!("\n<- {} ->\n", el);
        code_gen.into_byte_code()
    }
}

// ----------------------------------------------------------------------------
// ByteCode
#[derive(Clone, Debug)]
pub struct ByteCode {
    pub objects: VMObjects,
    pub package_indices: HashMap<String, OpIndex>,
    pub packages: Vec<PackageKey>,
    pub entry: FunctionKey,
}

#[cfg(test)]
mod test {
    //use super::*;
}
