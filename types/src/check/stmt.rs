use super::super::constant;
use super::super::objects::{DeclInfoKey, ObjKey, PackageKey, ScopeKey, TCObjects, TypeKey};
use super::super::operand::Operand;
use super::check::{Checker, ExprInfo, FilesContext};
use goscript_parser::ast::{BlockStmt, Expr, Stmt};
use goscript_parser::objects::{FuncDeclKey, Objects as AstObjects};

struct StmtContext {
    break_ok: bool,
    continue_ok: bool,
    fallthrough_ok: bool,
    final_switch_case: bool,
}

pub enum BodyContainer {
    FuncLitExpr(Expr),
    FuncDecl(FuncDeclKey),
}

impl BodyContainer {
    pub fn get_block<'a>(&'a self, objs: &'a AstObjects) -> &'a BlockStmt {
        match self {
            BodyContainer::FuncLitExpr(e) => match e {
                Expr::FuncLit(fl) => &fl.body,
                _ => unreachable!(),
            },
            BodyContainer::FuncDecl(key) => objs.fdecls[*key].body.as_ref().unwrap(),
        }
    }
}

impl<'a> Checker<'a> {
    pub fn func_body(
        &mut self,
        di: DeclInfoKey,
        name: &str,
        sig: TypeKey,
        body: BodyContainer,
        iota: Option<constant::Value>,
    ) {
        let stmts = body.get_block(self.ast_objs);
        unimplemented!()
    }

    fn stmt_list(&mut self, stmts: &Vec<Stmt>, sctx: StmtContext, fctx: &mut FilesContext) {
        unimplemented!()
    }

    fn stmt(&mut self, stmt: &Stmt, sctx: StmtContext, fctx: &mut FilesContext) {
        unimplemented!()
    }
}
