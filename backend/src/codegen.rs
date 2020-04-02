#![allow(dead_code)]
#[macro_use]
use super::opcode::*;
use super::types::Objects as VMObjects;
use super::types::*;
use super::value::GosValue;
use std::collections::HashMap;
use std::convert::TryInto;
use std::fs;
use std::sync::{Arc, Mutex};

use goscript_frontend::ast::*;
use goscript_frontend::ast_objects::Objects as AstObjects;
use goscript_frontend::ast_objects::*;
use goscript_frontend::errors::{ErrorList, FilePosErrors};
use goscript_frontend::position;
use goscript_frontend::token::Token;
use goscript_frontend::visitor::{walk_decl, walk_expr, walk_stmt, Visitor};
use goscript_frontend::{FileSet, Parser};

const UNNAMED_PARAM_PRE: &'static str = "__unnamed_param__";
const UNNAMED_RET_PARAM_PRE: &'static str = "__unnamed_return_param__";
const INVALID_LOCAL_INDEX: OpIndex = std::i16::MIN;

// ----------------------------------------------------------------------------
// Module
#[derive(Clone, Debug)]
pub struct PackageVal {
    pub name: String,
    pub funcs: Vec<FunctionKey>,
    pub structs: Vec<StructKey>,
    pub consts: Vec<GosValue>,
    pub vars: Vec<GosValue>,
    pub symbols: SymbolTable,
}

impl PackageVal {
    fn new(name: String) -> PackageVal {
        PackageVal {
            name: name,
            funcs: Vec::new(),
            structs: Vec::new(),
            consts: Vec::new(),
            vars: Vec::new(),
            symbols: SymbolTable::new(),
        }
    }

    fn add_func(&mut self, fkey: FunctionKey) {}
}

// ----------------------------------------------------------------------------
// FunctionVal
#[derive(Clone, Debug)]
pub struct FunctionVal {
    pub package: PackageKey,
    pub code: Vec<CodeData>,
    pub consts: Vec<GosValue>,
    pub param_count: usize,
    pub ret_count: usize,
    pub symbols: SymbolTable,
}

impl FunctionVal {
    fn new(package: PackageKey) -> FunctionVal {
        FunctionVal {
            package: package,
            code: Vec::new(),
            consts: Vec::new(),
            param_count: 0,
            ret_count: 0,
            symbols: SymbolTable::new(),
        }
    }

    fn add_local<'a>(&self, s: &String, e: &'a FilePosErrors, pos: position::Pos) -> OpIndex {
        match self.symbols.insert(s) {
            Some(i) => i,
            None => {
                // should already cought by the parser
                e.add(pos, format!("Variable is already declared: {}", s));
                INVALID_LOCAL_INDEX
            }
        }
    }

    fn get_local_index<'a>(&self, s: &String, e: &'a FilePosErrors, pos: position::Pos) -> OpIndex {
        match self.symbols.get_index(s) {
            Some(i) => i,
            None => {
                e.add(pos, format!("Undeclared variable : {}", s));
                INVALID_LOCAL_INDEX
            }
        }
    }

    fn add_const(&mut self, cst: GosValue) -> OpIndex {
        self.consts.push(cst);
        (self.consts.len() - 1).try_into().unwrap()
    }

    fn add_params<'e>(
        &mut self,
        fl: &FieldList,
        o: &AstObjects,
        pre: &'static str,
        errors: &FilePosErrors<'e>,
    ) -> usize {
        let mut unnamed_index = 0;
        fl.list
            .iter()
            .map(|f| {
                let names = &o.fields[*f].names;
                if names.len() == 0 {
                    let name = format!("{}{}", pre, unnamed_index);
                    unnamed_index += 1;
                    self.add_local(&name, errors, 0);
                    1
                } else {
                    names
                        .iter()
                        .map(|n| {
                            let ident = &o.idents[*n];
                            self.add_local(&ident.name, errors, ident.pos);
                        })
                        .count()
                }
            })
            .sum()
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
}

// ----------------------------------------------------------------------------
// CodeGen
pub struct CodeGen<'a> {
    objects: VMObjects,
    ast_objs: &'a AstObjects,
    packages: Vec<PackageKey>,
    current_pkg: PackageKey,
    func_stack: Vec<FunctionKey>,
    errors: &'a FilePosErrors<'a>,
    symbols: SymbolTable,
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

    fn visit_expr_ident(&mut self, ident: &IdentIndex) {
        let id = &self.ast_objs.idents[*ident];
        dbg!(id);
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
        let func = self.current_func_mut();
        let i = func.add_const(val);
        func.emit_push_const(i);
    }

    fn visit_expr_func_lit(&mut self, flit: &FuncLit) {
        dbg!(flit);
    }

    fn visit_expr_composit_lit(&mut self, clit: &CompositeLit) {
        unimplemented!();
    }

    fn visit_expr_paren(&mut self) {
        unimplemented!();
    }

    fn visit_expr_selector(&mut self, ident: &IdentIndex) {
        unimplemented!();
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

    fn visit_expr_call(&mut self, args: usize) {
        dbg!(args);
    }

    fn visit_expr_star(&mut self) {
        unimplemented!();
    }

    fn visit_expr_unary(&mut self, op: &Token) {
        unimplemented!();
    }

    fn visit_expr_binary(&mut self, op: &Token) {
        dbg!(op);
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

    fn visit_stmt_decl_func(&mut self, fdecl: &FuncDeclIndex) {
        let mut func = FunctionVal::new(self.current_pkg.clone());
        let decl = &self.ast_objs.decls[*fdecl];
        let typ = &decl.typ;
        func.ret_count = match &typ.results {
            Some(fl) => func.add_params(&fl, self.ast_objs, UNNAMED_RET_PARAM_PRE, self.errors),
            None => 0,
        };
        func.param_count =
            func.add_params(&typ.params, self.ast_objs, UNNAMED_PARAM_PRE, self.errors);

        let fkey = self.objects.functions.insert(func);
        self.func_stack.push(fkey.clone());
        // process function body
        if let Some(stmt) = &decl.body {
            self.visit_stmt_block(stmt);
        }
        self.current_pkg_mut().add_func(fkey);
    }

    fn visit_stmt_labeled(&mut self, lstmt: &LabeledStmtIndex) {
        unimplemented!();
    }

    fn visit_stmt_send(&mut self, sstmt: &SendStmt) {
        unimplemented!();
    }

    fn visit_stmt_incdec(&mut self, idcstmt: &IncDecStmt) {
        unimplemented!();
    }

    fn visit_stmt_assign(&mut self, astmt: &AssignStmtIndex) {
        let stmt = &self.ast_objs.a_stmts[*astmt];
        let is_def = stmt.token == Token::DEFINE;

        // handle the left hand side
        let func = self.current_func();
        let mut locals: Vec<OpIndex> = stmt
            .lhs
            .iter()
            .map(|expr| {
                if let Expr::Ident(ident) = expr {
                    let ident = &self.ast_objs.idents[*ident.as_ref()];
                    if is_def {
                        func.add_local(&ident.name, self.errors, ident.pos)
                    } else {
                        func.get_local_index(&ident.name, self.errors, ident.pos)
                    }
                } else {
                    unreachable!();
                }
            })
            .collect();
        drop(func);

        // handle the right hand side
        for val in stmt.rhs.iter() {
            self.visit_expr(val);
        }

        // now the values should be on stack, generate code to set them to the vars
        let func = self.current_func_mut();
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
        //dbg!(rstmt);
    }

    fn visit_stmt_branch(&mut self, bstmt: &BranchStmt) {
        unimplemented!();
    }

    fn visit_stmt_block(&mut self, bstmt: &BlockStmt) {
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
            packages: Vec::new(),
            current_pkg: slotmap::Key::null(),
            func_stack: Vec::new(),
            symbols: SymbolTable::new(),
            errors: err,
        }
    }

    fn current_func(&self) -> &FunctionVal {
        &self.objects.functions[*self.func_stack.last().unwrap()]
    }

    fn current_func_mut(&mut self) -> &mut FunctionVal {
        &mut self.objects.functions[*self.func_stack.last().unwrap()]
    }

    fn current_pkg(&self) -> &PackageVal {
        &self.objects.packages[self.current_pkg]
    }

    fn current_pkg_mut(&mut self) -> &mut PackageVal {
        &mut self.objects.packages[self.current_pkg]
    }

    pub fn gen(&mut self, f: File) {
        let pkg = &self.ast_objs.idents[f.name];
        if !self.symbols.contains(&pkg.name) {
            let pkgval = PackageVal::new(pkg.name.clone());
            let pkey = self.objects.packages.insert(pkgval);
            self.packages.push(pkey.clone());
            self.current_pkg = pkey;
        } else {
            // find package
        }
        for d in f.decls.iter() {
            self.visit_decl(d)
        }
    }

    pub fn into_byte_code(self) -> (ByteCode, FunctionKey) {
        let fk = self.current_pkg().funcs[0].clone();
        (
            ByteCode {
                objects: self.objects,
                packages: self.packages,
                symbols: self.symbols,
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
    pub packages: Vec<PackageKey>,
    pub symbols: SymbolTable,
}
// ----------------------------------------------------------------------------
// SymbolTable

#[derive(Debug)]
struct STData {
    index: OpIndex,
    table: HashMap<String, OpIndex>,
}

#[derive(Clone, Debug)]
pub struct SymbolTable {
    data: Arc<Mutex<STData>>,
}

impl SymbolTable {
    fn new() -> SymbolTable {
        SymbolTable {
            data: Arc::new(Mutex::new(STData {
                index: 0,
                table: HashMap::new(),
            })),
        }
    }

    fn count(&self) -> usize {
        let d = self.data.lock().unwrap();
        d.index as usize
    }

    fn do_insert(&self, s: &String) -> OpIndex {
        let mut d = self.data.lock().unwrap();
        let new_index = d.index;
        d.table.insert(s.clone(), new_index);
        d.index += 1;
        new_index
    }

    fn insert_or_get_index(&self, s: &String) -> OpIndex {
        let d = self.data.lock().unwrap();
        if d.table.contains_key(s) {
            d.table[s]
        } else {
            drop(d);
            self.do_insert(s)
        }
    }

    fn contains(&self, s: &String) -> bool {
        let d = self.data.lock().unwrap();
        d.table.contains_key(s)
    }

    fn get_index(&self, s: &String) -> Option<OpIndex> {
        let d = self.data.lock().unwrap();
        if !d.table.contains_key(s) {
            None
        } else {
            Some(d.table[s])
        }
    }

    fn insert(&self, s: &String) -> Option<OpIndex> {
        let d = self.data.lock().unwrap();
        if d.table.contains_key(s) {
            None
        } else {
            drop(d);
            Some(self.do_insert(s))
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_symbol_table() {
        let s = SymbolTable::new();
        dbg!(s.insert_or_get_index(&"aaa".to_owned()));
        dbg!(s.insert_or_get_index(&"bbb".to_owned()));
        dbg!(s.insert_or_get_index(&"aaa".to_owned()));
        dbg!(s.insert_or_get_index(&"ccc".to_owned()));
    }
}
