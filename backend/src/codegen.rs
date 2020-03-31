#![allow(dead_code)]
#[macro_use]
use super::opcode::CodeData;
use super::types::Objects as VMObjects;
use super::types::*;
use super::value::GosValue;
use std::borrow::BorrowMut;
use std::collections::HashMap;
use std::fs;
use std::sync::{Arc, Mutex};

use goscript_frontend::ast::*;
use goscript_frontend::ast_objects::Objects as AstObjects;
use goscript_frontend::ast_objects::*;
use goscript_frontend::token::Token;
use goscript_frontend::visitor::{walk_decl, walk_expr, walk_stmt, Visitor};
use goscript_frontend::{FileSet, Parser};

// ----------------------------------------------------------------------------
// FunctionVal
#[derive(Clone, Debug)]
pub struct FunctionVal {
    pub code: Vec<CodeData>,
    pub consts: Vec<GosValue>,
    pub arg_count: usize,
    pub ret_count: usize,
}

impl FunctionVal {
    fn new(args: usize, rets: usize) -> FunctionVal {
        FunctionVal {
            code: Vec::new(),
            consts: Vec::new(),
            arg_count: args,
            ret_count: rets,
        }
    }
}

// ----------------------------------------------------------------------------
// CodeGen
pub struct CodeGen {
    fset: FileSet,
    objects: VMObjects,
    ast_objs: AstObjects,
    funcs: Vec<FunctionKey>,
    symbols: SymbolTable,
}

impl Visitor for CodeGen {
    fn visit_expr(&mut self, expr: &Expr) {
        walk_expr(self, expr);
    }

    fn visit_stmt(&mut self, stmt: &Stmt) {
        unimplemented!();
    }

    fn visit_decl(&mut self, decl: &Decl) {
        walk_decl(self, decl);
    }

    fn visit_expr_ident(&mut self, ident: &IdentIndex) {
        unimplemented!();
    }

    fn visit_expr_option(&mut self, op: &Option<Expr>) {
        unimplemented!();
    }

    fn visit_expr_ellipsis(&mut self) {
        unimplemented!();
    }

    fn visit_expr_basic_lit(&mut self, blit: &BasicLit) {
        unimplemented!();
    }

    fn visit_expr_func_lit(&mut self, flit: &FuncLit) {
        unimplemented!();
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
        unimplemented!();
    }

    fn visit_expr_star(&mut self) {
        unimplemented!();
    }

    fn visit_expr_unary(&mut self, op: &Token) {
        unimplemented!();
    }

    fn visit_expr_binary(&mut self, op: &Token) {
        unimplemented!();
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
        dbg!(fdecl);
        let decl = &self.ast_objs.decls[*fdecl];
        let typ = &decl.typ;
        let fkey = self.objects.functions.insert(FunctionVal::new(0, 0));
        self.funcs.push(fkey);
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
        unimplemented!();
    }

    fn visit_stmt_go(&mut self, gostmt: &GoStmt) {
        unimplemented!();
    }

    fn visit_stmt_defer(&mut self, dstmt: &DeferStmt) {
        unimplemented!();
    }

    fn visit_stmt_return(&mut self, rstmt: &ReturnStmt) {
        unimplemented!();
    }

    fn visit_stmt_branch(&mut self, bstmt: &BranchStmt) {
        unimplemented!();
    }

    fn visit_stmt_block(&mut self, bstmt: &BlockStmt) {
        unimplemented!();
    }

    fn visit_stmt_if(&mut self, ifstmt: &IfStmt) {
        unimplemented!();
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

impl CodeGen {
    pub fn new() -> CodeGen {
        CodeGen {
            fset: FileSet::new(),
            objects: VMObjects::new(),
            ast_objs: AstObjects::new(),
            funcs: Vec::new(),
            symbols: SymbolTable::new(),
        }
    }

    pub fn gen(&mut self, f: File) {
        for d in f.decls.iter() {
            self.visit_decl(d)
        }
    }

    pub fn into_byte_code(self) -> (ByteCode, FunctionKey) {
        let fk = self.funcs[0].clone();
        (
            ByteCode {
                objects: self.objects,
                funcs: self.funcs,
                symbols: self.symbols,
            },
            fk,
        )
    }

    pub fn new_load_parse_gen(&mut self, path: &str, trace: bool) {
        let src = fs::read_to_string(path).expect("read file err: ");
        let f = self.fset.add_file(path, None, src.chars().count());
        let mut p = Parser::new(&mut self.ast_objs, f, &src, trace);
        let file = p.parse_file();

        print!("<- {} ->", p.get_errors());

        self.gen(file.unwrap());
    }
}

// ----------------------------------------------------------------------------
// ByteCode
#[derive(Clone, Debug)]
pub struct ByteCode {
    pub objects: VMObjects,
    pub funcs: Vec<FunctionKey>,
    pub symbols: SymbolTable,
}
// ----------------------------------------------------------------------------
// SymbolTable

#[derive(Debug)]
struct STData {
    index: usize,
    table: HashMap<String, usize>,
}

#[derive(Clone, Debug)]
pub struct SymbolTable {
    data: Arc<Mutex<STData>>,
}

impl SymbolTable {
    fn new() -> SymbolTable {
        SymbolTable {
            data: Arc::new(Mutex::new(STData {
                index: 1,
                table: HashMap::new(),
            })),
        }
    }

    fn get_index(&self, s: &String) -> usize {
        let mut d = self.data.lock().unwrap();
        if d.table.contains_key(s) {
            d.table[s]
        } else {
            let new_index = d.index;
            d.table.insert(s.clone(), new_index);
            d.index += 1;
            new_index
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_symbol_table() {
        let s = SymbolTable::new();
        dbg!(s.get_index(&"aaa".to_owned()));
        dbg!(s.get_index(&"bbb".to_owned()));
        dbg!(s.get_index(&"aaa".to_owned()));
        dbg!(s.get_index(&"ccc".to_owned()));
    }
}
