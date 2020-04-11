#![allow(dead_code)]
#[macro_use]
use super::opcode::*;
use super::primitive::Primitive;
use super::types::Objects as VMObjects;
use super::types::*;
use super::value::GosValue;
use std::collections::HashMap;
use std::convert::TryInto;
use std::fs;

use goscript_frontend::ast::*;
use goscript_frontend::ast_objects::Objects as AstObjects;
use goscript_frontend::ast_objects::*;
use goscript_frontend::errors::{ErrorList, FilePosErrors};
use goscript_frontend::scope::*;
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
#[derive(Clone, Debug)]
pub struct FunctionVal {
    pub package: PackageKey,
    pub code: Vec<CodeData>,
    pub consts: Vec<GosValue>,
    pub up_ptrs: Vec<UpValue>,
    pub param_count: usize,
    pub ret_count: usize,
    pub entities: HashMap<EntityKey, OpIndex>,
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

    fn add_local(&mut self, entity: Option<EntityKey>) -> OpIndex {
        let result = self.local_alloc as OpIndex;
        if let Some(key) = entity {
            let old = self.entities.insert(key, result);
            assert_eq!(old, None);
        };
        self.local_alloc += 1;
        result
    }

    fn get_entity_index(&self, entity: &EntityKey) -> &OpIndex {
        self.entities.get(entity).unwrap()
    }

    fn add_const(&mut self, entity: Option<EntityKey>, cst: GosValue) -> OpIndex {
        self.consts.push(cst);
        let result = (self.consts.len() - 1).try_into().unwrap();
        if let Some(key) = entity {
            let old = self.entities.insert(key, result);
            assert_eq!(old, None);
        }
        result
    }

    fn try_add_upvalue(&mut self, entity: &EntityKey, uv: UpValue) -> OpIndex {
        self.entities
            .get(entity)
            .map(|x| *x)
            .or_else(|| {
                self.up_ptrs.push(uv);
                (self.up_ptrs.len() - 1).try_into().ok()
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

    fn emit_load_pkg_member(&mut self, i: OpIndex) {
        self.code.push(CodeData::Code(Opcode::LOAD_PKG_VAR));
        self.code.push(CodeData::Data(i));
    }

    fn emit_load_local(&mut self, local: OpIndex) {
        let code = Opcode::get_load_local(local);
        self.code.push(CodeData::Code(code));
        if let Opcode::LOAD_LOCAL = code {
            self.code.push(CodeData::Data(local));
        }
    }

    fn emit_store_local(&mut self, local: OpIndex) {
        self.code.push(CodeData::Code(Opcode::STORE_LOCAL));
        self.code.push(CodeData::Data(local));
        self.code.push(CodeData::Code(Opcode::POP));
    }

    fn emit_push_const(&mut self, i: OpIndex) {
        self.code.push(CodeData::Code(Opcode::PUSH_CONST));
        self.code.push(CodeData::Data(i));
    }

    fn emit_load_upvalue(&mut self, i: OpIndex) {
        self.code.push(CodeData::Code(Opcode::LOAD_UPVALUE));
        self.code.push(CodeData::Data(i));
    }

    fn emit_binary_primi_call(&mut self, token: &Token) {
        match token {
            Token::ADD => {
                self.code.push(CodeData::Code(Opcode::CALL_PRIMI_2_1));
                self.code.push(CodeData::Data(Primitive::Add as OpIndex));
            }
            Token::SUB => {
                self.code.push(CodeData::Code(Opcode::CALL_PRIMI_2_1));
                self.code.push(CodeData::Data(Primitive::Sub as OpIndex));
            }
            _ => unimplemented!(),
        }
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
        // 1. try local frist
        let id = &self.ast_objs.idents[*ident];
        let entity = id.entity_obj(self.ast_objs).unwrap();
        let entity_key = id.entity_key().unwrap();
        let local = current_func_mut!(self)
            .entities
            .get(&entity_key)
            .map(|x| *x);
        if local.is_some() {
            let func = current_func_mut!(self);
            match entity.kind {
                EntityKind::Var => func.emit_load_local(local.unwrap()),
                EntityKind::Con => func.emit_push_const(local.unwrap()),
                _ => unreachable!(),
            }
            return;
        }
        // 2. try upvalue
        let upvalue = self
            .func_stack
            .clone()
            .iter()
            .enumerate()
            .rev()
            .skip(1)
            .find_map(|(level, ifunc)| {
                let f = &mut self.objects.functions[*ifunc];
                let index = f.entities.get(&entity_key).map(|x| *x);
                if let Some(ind) = index {
                    Some(UpValue::Open(level as OpIndex, ind as OpIndex))
                } else {
                    None
                }
            });
        if let Some(uv) = upvalue {
            let func = current_func_mut!(self);
            let index = func.try_add_upvalue(&entity_key, uv);
            func.emit_load_upvalue(index);
            return;
        }
        // 3. try the package level
        let index = current_pkg!(self).look_up[&entity_key];
        current_func_mut!(self).emit_load_pkg_member(index);
    }

    fn visit_expr_option(&mut self, op: &Option<Expr>) {
        unimplemented!();
    }

    fn visit_expr_ellipsis(&mut self) {
        unimplemented!();
    }

    fn visit_expr_basic_lit(&mut self, blit: &BasicLit) {
        let val = match &blit.token {
            Token::INT(i) => GosValue::Int(i.parse::<i64>().unwrap()),
            Token::FLOAT(f) => GosValue::Float(f.parse::<f64>().unwrap()),
            Token::IMAG(_) => unimplemented!(),
            Token::CHAR(_) => unimplemented!(),
            Token::STRING(s) => {
                let val = StringVal {
                    dark: false,
                    data: s.clone(),
                };
                GosValue::Str(self.objects.strings.insert(val))
            }
            _ => unreachable!(),
        };
        let func = current_func_mut!(self);
        let i = func.add_const(None, val);
        func.emit_push_const(i);
    }

    /// Add function as a const and then generate a closure of it
    fn visit_expr_func_lit(&mut self, flit: &FuncLit) {
        dbg!(flit);
        let fkey = self.gen_func_def(&flit.typ, &flit.body);
        let func = current_func_mut!(self);
        let i = func.add_const(None, GosValue::Function(fkey));
        func.emit_push_const(i);
        func.emit_new_closure();
    }

    fn visit_expr_composit_lit(&mut self, clit: &CompositeLit) {
        unimplemented!();
    }

    fn visit_expr_paren(&mut self) {
        unimplemented!();
    }

    fn visit_expr_selector(&mut self, ident: &IdentKey) {
        let sid = &self.ast_objs.idents[*ident];
        dbg!(sid);
    }

    fn visit_expr_index(&mut self) {
        unimplemented!();
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
        current_func_mut!(self).emit_binary_primi_call(op);
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
        unimplemented!();
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

        // handle the left hand side
        let mut locals: Vec<OpIndex> = stmt
            .lhs
            .iter()
            .map(|expr| {
                if let Expr::Ident(ident) = expr {
                    let ident = self.ast_objs.idents[*ident.as_ref()].clone();
                    let func = current_func_mut!(self);
                    if is_def {
                        func.add_local(ident.entity.into_key())
                    } else {
                        *func.get_entity_index(&ident.entity.clone().into_key().unwrap())
                    }
                } else {
                    unreachable!();
                }
            })
            .collect();

        // handle the right hand side
        for val in stmt.rhs.iter() {
            self.visit_expr(val);
        }

        // now the values should be on stack, generate code to set them to the vars
        let func = current_func_mut!(self);
        locals.reverse();
        for l in locals.iter() {
            func.emit_store_local(*l);
        }
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
            current_func_mut!(self).emit_store_local(i as OpIndex);
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
