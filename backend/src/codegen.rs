#![allow(dead_code)]
#[macro_use]
use super::opcode::*;
use super::primitive::Primitive;
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

macro_rules! current_pkg {
    ($owner:ident) => {
        &$owner.objects.packages[$owner.current_pkg]
    };
}

macro_rules! current_pkg_mut {
    ($owner:ident) => {
        &mut $owner.objects.packages[$owner.current_pkg]
    };
}

// ----------------------------------------------------------------------------
// package
#[derive(Clone, Debug)]
pub struct PackageVal {
    pub name: String,
    pub main_func: FunctionKey,
    pub imports: Vec<PackageKey>,
    pub members: Vec<GosValue>,
    pub look_up: HashMap<EntityKey, OpIndex>,
}

impl PackageVal {
    fn new(name: String) -> PackageVal {
        PackageVal {
            name: name,
            main_func: slotmap::Key::null(),
            imports: Vec::new(),
            members: Vec::new(),
            look_up: HashMap::new(),
        }
    }

    fn add_func(&mut self, entity: EntityKey, fkey: FunctionKey) {
        self.members.push(GosValue::Function(fkey));
        self.look_up
            .insert(entity, (self.members.len() - 1) as OpIndex);
    }
}

// ----------------------------------------------------------------------------
// FunctionVal

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum FnEntIndex {
    Const(OpIndex),
    LocalVar(OpIndex),
    UpValue(OpIndex),
    PackageMember(OpIndex),
    Blank,
}

impl From<FnEntIndex> for OpIndex {
    fn from(t: FnEntIndex) -> OpIndex {
        match t {
            FnEntIndex::Const(i) => i,
            FnEntIndex::LocalVar(i) => i,
            FnEntIndex::UpValue(i) => i,
            FnEntIndex::PackageMember(i) => i,
            FnEntIndex::Blank => unreachable!(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct FunctionVal {
    pub package: PackageKey,
    pub code: Vec<CodeData>,
    pub consts: Vec<GosValue>,
    pub up_ptrs: Vec<UpValue>,
    pub param_count: usize,
    pub ret_count: usize,
    pub entities: HashMap<EntityKey, FnEntIndex>,
    local_alloc: u16,
}

impl FunctionVal {
    fn new(package: PackageKey) -> FunctionVal {
        FunctionVal {
            package: package,
            code: Vec::new(),
            consts: Vec::new(),
            up_ptrs: Vec::new(),
            param_count: 0,
            ret_count: 0,
            entities: HashMap::new(),
            local_alloc: 0,
        }
    }

    pub fn local_count(&self) -> usize {
        self.local_alloc as usize - self.param_count - self.ret_count
    }

    fn add_local(&mut self, entity: Option<EntityKey>) -> FnEntIndex {
        let result = self.local_alloc as OpIndex;
        if let Some(key) = entity {
            let old = self.entities.insert(key, FnEntIndex::LocalVar(result));
            assert_eq!(old, None);
        };
        self.local_alloc += 1;
        FnEntIndex::LocalVar(result)
    }

    fn get_entity_index(&self, entity: &EntityKey) -> FnEntIndex {
        self.entities.get(entity).unwrap().clone()
    }

    fn add_const(&mut self, entity: Option<EntityKey>, cst: GosValue) -> FnEntIndex {
        self.consts.push(cst);
        let result = (self.consts.len() - 1).try_into().unwrap();
        if let Some(key) = entity {
            let old = self.entities.insert(key, FnEntIndex::Const(result));
            assert_eq!(old, None);
        }
        FnEntIndex::Const(result)
    }

    fn try_add_upvalue(&mut self, entity: &EntityKey, uv: UpValue) -> FnEntIndex {
        self.entities
            .get(entity)
            .map(|x| *x)
            .or_else(|| {
                self.up_ptrs.push(uv);
                let i = (self.up_ptrs.len() - 1).try_into().ok();
                let et = FnEntIndex::UpValue(i.unwrap());
                self.entities.insert(*entity, et);
                i.map(|x| FnEntIndex::UpValue(x))
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

    fn emit_load(&mut self, index: FnEntIndex) {
        match index {
            FnEntIndex::Const(i) => {
                self.code.push(CodeData::Code(Opcode::PUSH_CONST));
                self.code.push(CodeData::Data(i));
            }
            FnEntIndex::LocalVar(i) => {
                let code = Opcode::get_load_local(i);
                self.code.push(CodeData::Code(code));
                if let Opcode::LOAD_LOCAL = code {
                    self.code.push(CodeData::Data(i));
                }
            }
            FnEntIndex::UpValue(i) => {
                self.code.push(CodeData::Code(Opcode::LOAD_UPVALUE));
                self.code.push(CodeData::Data(i));
            }
            FnEntIndex::PackageMember(i) => {
                self.code.push(CodeData::Code(Opcode::LOAD_PKG_VAR));
                self.code.push(CodeData::Data(i));
            }
            FnEntIndex::Blank => unreachable!(),
        }
    }

    fn emit_store(&mut self, index: FnEntIndex) {
        match index {
            FnEntIndex::Blank => {}
            _ => {
                let (code, i) = match index {
                    FnEntIndex::Const(_) => unreachable!(),
                    FnEntIndex::LocalVar(i) => (Opcode::STORE_LOCAL, i),
                    FnEntIndex::UpValue(i) => (Opcode::STORE_UPVALUE, i),
                    FnEntIndex::PackageMember(_) => unimplemented!(),
                    FnEntIndex::Blank => unreachable!(),
                };
                self.code.push(CodeData::Code(code));
                self.code.push(CodeData::Data(i));
            }
        }
        self.code.push(CodeData::Code(Opcode::POP));
    }

    fn emit_binary_primi_call(&mut self, primi: Primitive) {
        self.code.push(CodeData::Code(Opcode::CALL_PRIMI_2_1));
        self.code.push(CodeData::Data(primi as OpIndex));
    }

    fn emit_return(&mut self) {
        self.code.push(CodeData::Code(Opcode::RETURN));
    }

    fn emit_pre_call(&mut self) {
        self.code.push(CodeData::Code(Opcode::PRE_CALL));
    }

    fn emit_call(&mut self, _args: usize) {
        self.code.push(CodeData::Code(Opcode::CALL));
    }

    fn emit_new_closure(&mut self) {
        self.code.push(CodeData::Code(Opcode::NEW_CLOSURE));
    }
}

// ----------------------------------------------------------------------------
// CodeGen
pub struct CodeGen<'a> {
    objects: VMObjects,
    ast_objs: &'a AstObjects,
    packages: HashMap<String, PackageKey>,
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
        let val = self.value_from_literal(&blit, None);
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
        let val = self.value_from_comp_literal(clit.typ.as_ref().unwrap(), &clit.elts);
        let func = current_func_mut!(self);
        let i = func.add_const(None, val);
        func.emit_load(i);
    }

    fn visit_expr_paren(&mut self) {
        unimplemented!();
    }

    fn visit_expr_selector(&mut self, ident: &IdentKey) {
        let sid = &self.ast_objs.idents[*ident];
        dbg!(sid);
    }

    fn visit_expr_index(&mut self) {
        current_func_mut!(self).emit_binary_primi_call(Primitive::Index);
    }

    fn visit_expr_slice(&mut self) {
        unimplemented!();
    }

    fn visit_expr_type_assert(&mut self) {
        unimplemented!();
    }

    fn visit_expr_call(&mut self, args: &Vec<Expr>) {
        current_func_mut!(self).emit_pre_call();
        let count = args.iter().map(|e| self.visit_expr(e)).count();
        current_func_mut!(self).emit_call(count);
    }

    fn visit_expr_star(&mut self) {
        unimplemented!();
    }

    fn visit_expr_unary(&mut self, op: &Token) {
        unimplemented!();
    }

    fn visit_expr_binary(&mut self, op: &Token) {
        let primi = match op {
            Token::ADD => Primitive::Add,
            Token::SUB => Primitive::Sub,
            _ => unimplemented!(),
        };
        current_func_mut!(self).emit_binary_primi_call(primi);
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
                Spec::Import(_is) => {}
                Spec::Type(_ts) => {}
                Spec::Value(vs) => {
                    if gdecl.token == Token::VAR {
                        self.gen_assign_def_var(&vs.names, &vs.values, &vs.typ, true);
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
        current_pkg_mut!(self).add_func(ident.entity_key().unwrap(), fkey);
        if ident.name == "main" {
            current_pkg_mut!(self).main_func = fkey;
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
        let names: Vec<IdentKey> = stmt
            .lhs
            .iter()
            .map(|expr| {
                if let Expr::Ident(ident) = expr {
                    *ident.as_ref()
                } else {
                    unreachable!();
                }
            })
            .collect();
        self.gen_assign_def_var(&names, &stmt.rhs, &None, is_def);
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
            current_func_mut!(self).emit_store(FnEntIndex::LocalVar(i as OpIndex));
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
            packages: HashMap::new(),
            current_pkg: slotmap::Key::null(),
            func_stack: Vec::new(),
            errors: err,
        }
    }

    fn resolve_ident(&mut self, ident: &IdentKey) -> FnEntIndex {
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
            .rev()
            .skip(1)
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
        FnEntIndex::PackageMember(current_pkg!(self).look_up[&entity_key])
    }

    fn gen_def_const(&mut self, names: &Vec<IdentKey>, values: &Vec<Expr>, typ: &Option<Expr>) {
        if names.len() != values.len() {
            let ident = &self.ast_objs.idents[names[0]];
            self.error_mismatch(ident.pos, names.len(), values.len());
            return;
        }
        for i in 0..names.len() {
            let ident = self.ast_objs.idents[names[i]].clone();
            let val = self.get_const_value(typ.as_ref(), &values[i]);
            current_func_mut!(self).add_const(ident.entity.into_key(), val);
        }
    }

    fn gen_assign_def_var(
        &mut self,
        names: &Vec<IdentKey>,
        values: &Vec<Expr>,
        typ: &Option<Expr>,
        is_def: bool,
    ) {
        // handle the left hand side
        let mut locals: Vec<FnEntIndex> = names
            .iter()
            .map(|ikey| {
                let id = self.ast_objs.idents[*ikey].clone();
                let func = current_func_mut!(self);
                if id.is_blank() {
                    FnEntIndex::Blank
                } else if is_def {
                    func.add_local(id.entity.into_key())
                } else {
                    self.resolve_ident(ikey)
                }
            })
            .collect();

        // handle the right hand side
        if values.len() == names.len() {
            for val in values.iter() {
                self.visit_expr(val);
            }
        } else if values.len() == 0 {
            let val = self.get_type_default(&typ.as_ref().unwrap());
            for _ in 0..names.len() {
                let func = current_func_mut!(self);
                let i = func.add_const(None, val);
                func.emit_load(i);
            }
        } else {
            let ident = &self.ast_objs.idents[names[0]];
            self.error_mismatch(ident.pos, names.len(), values.len());
        }

        // now the values should be on stack, generate code to set them to the vars
        let func = current_func_mut!(self);
        locals.reverse();
        for l in locals.iter() {
            func.emit_store(*l);
        }
    }

    fn error_mismatch(&self, pos: position::Pos, l: usize, r: usize) {
        self.errors.add(
            pos,
            format!("assignment mismatch: {} variables but {} values", l, r),
        )
    }

    fn gen_func_def(&mut self, typ: &FuncType, body: &BlockStmt) -> FunctionKey {
        let mut func = FunctionVal::new(self.current_pkg.clone());
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
        let func = &mut self.objects.functions[fkey];
        func.emit_return();

        self.func_stack.pop();
        fkey
    }

    pub fn get_type_default(&self, expr: &Expr) -> GosValue {
        match expr {
            Expr::Ident(idkey) => {
                let id = &self.ast_objs.idents[*idkey.as_ref()];
                GosValue::primitive_default(&id.name)
            }
            _ => unimplemented!(),
        }
    }

    pub fn get_const_value(&mut self, typ: Option<&Expr>, expr: &Expr) -> GosValue {
        match expr {
            Expr::BasicLit(lit) => {
                let t = typ.as_ref().map(|e| self.get_type_default(e));
                self.value_from_literal(lit, t.as_ref())
            }
            _ => {
                self.errors.add_str(
                    expr.pos(self.ast_objs),
                    "complex constant not supported yet",
                );
                GosValue::Nil
            }
        }
    }

    // this is a primitive version of Go's constant evaluation, which needs to be implemented
    // as part of the Type Checker
    pub fn value_from_literal(&mut self, blit: &BasicLit, typ: Option<&GosValue>) -> GosValue {
        if typ.is_none() {
            match &blit.token {
                Token::INT(i) => GosValue::Int(i.parse::<isize>().unwrap()),
                Token::FLOAT(f) => GosValue::Float64(f.parse::<f64>().unwrap()),
                Token::IMAG(_) => unimplemented!(),
                Token::CHAR(c) => GosValue::Int(c.chars().skip(1).next().unwrap() as isize),
                Token::STRING(s) => GosValue::new_str(s.to_string(), &mut self.objects),
                _ => unreachable!(),
            }
        } else {
            match (typ.unwrap(), &blit.token) {
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

    pub fn value_from_comp_literal(&mut self, typ: &Expr, elts: &Vec<Expr>) -> GosValue {
        match typ {
            Expr::Array(arr) => {
                if arr.as_ref().len.is_some() {
                    // array is not supported yet
                    unimplemented!()
                }
                let mut vals: Vec<GosValue> = match &arr.as_ref().elt {
                    Expr::Array(_) | Expr::Map(_) => elts
                        .iter()
                        .map(|elt| {
                            if let Expr::CompositeLit(clit) = elt {
                                self.value_from_comp_literal(&arr.as_ref().elt, &clit.elts)
                            } else {
                                unreachable!()
                            }
                        })
                        .collect(),
                    Expr::Ident(_) => elts
                        .iter()
                        .map(|elt| self.get_const_value(Some(&arr.as_ref().elt), elt))
                        .collect(),
                    _ => unimplemented!(),
                };
                let slice = self.objects.new_slice(elts.len());
                let slice_val = &mut self.objects.slices[*slice.get_slice()];
                slice_val.append(&mut vals);
                slice
            }
            Expr::Map(map) => unimplemented!(),
            _ => unimplemented!(),
        }
    }

    pub fn gen(&mut self, f: File) {
        let pkg = &self.ast_objs.idents[f.name];
        if !self.packages.contains_key(&pkg.name) {
            let pkgval = PackageVal::new(pkg.name.clone());
            let pkey = self.objects.packages.insert(pkgval);
            self.packages.insert(pkg.name.clone(), pkey);
            self.current_pkg = pkey;
        } else {
            // find package
        }
        for d in f.decls.iter() {
            self.visit_decl(d)
        }
    }

    pub fn into_byte_code(self) -> (ByteCode, FunctionKey) {
        let fk = current_pkg!(self).main_func;
        (
            ByteCode {
                objects: self.objects,
                packages: self.packages,
            },
            fk,
        )
    }

    pub fn load_parse_gen(path: &str, trace: bool) -> (ByteCode, FunctionKey) {
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
    pub packages: HashMap<String, PackageKey>,
}

#[cfg(test)]
mod test {
    //use super::*;
}
