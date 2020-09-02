use super::ast::*;
use super::objects::*;
use super::token::Token;

pub trait ExprVisitor {
    type Result;

    fn visit_expr(&mut self, expr: &Expr) -> Self::Result;

    fn visit_expr_ident(&mut self, ident: &IdentKey) -> Self::Result;

    fn visit_expr_ellipsis(&mut self, els: &Option<Expr>) -> Self::Result;

    fn visit_expr_basic_lit(&mut self, blit: &BasicLit, id: NodeId) -> Self::Result;

    fn visit_expr_func_lit(&mut self, flit: &FuncLit, id: NodeId) -> Self::Result;

    fn visit_expr_composit_lit(&mut self, clit: &CompositeLit) -> Self::Result;

    fn visit_expr_paren(&mut self, expr: &Expr) -> Self::Result;

    fn visit_expr_selector(&mut self, expr: &Expr, ident: &IdentKey, id: NodeId) -> Self::Result; //add: lvalue

    fn visit_expr_index(&mut self, expr: &Expr, index: &Expr) -> Self::Result;

    fn visit_expr_slice(
        &mut self,
        expr: &Expr,
        low: &Option<Expr>,
        high: &Option<Expr>,
        max: &Option<Expr>,
    ) -> Self::Result;

    fn visit_expr_type_assert(&mut self, expr: &Expr, typ: &Option<Expr>) -> Self::Result;

    fn visit_expr_call(&mut self, func: &Expr, args: &Vec<Expr>, ellipsis: bool) -> Self::Result;

    fn visit_expr_star(&mut self, expr: &Expr) -> Self::Result;

    fn visit_expr_unary(&mut self, expr: &Expr, op: &Token) -> Self::Result;

    fn visit_expr_binary(&mut self, left: &Expr, op: &Token, right: &Expr) -> Self::Result;

    fn visit_expr_key_value(&mut self, key: &Expr, val: &Expr) -> Self::Result;

    /// codegen needs the unwraped expr
    fn visit_expr_array_type(&mut self, len: &Option<Expr>, elm: &Expr, arr: &Expr)
        -> Self::Result;

    fn visit_expr_struct_type(&mut self, s: &StructType) -> Self::Result;

    fn visit_expr_func_type(&mut self, s: &FuncTypeKey) -> Self::Result;

    fn visit_expr_interface_type(&mut self, s: &InterfaceType) -> Self::Result;

    /// codegen needs the unwraped expr
    fn visit_map_type(&mut self, key: &Expr, val: &Expr, map: &Expr) -> Self::Result;

    fn visit_chan_type(&mut self, chan: &Expr, dir: &ChanDir) -> Self::Result;

    fn visit_bad_expr(&mut self, e: &BadExpr) -> Self::Result;
}

pub trait StmtVisitor {
    type Result;

    fn visit_stmt(&mut self, stmt: &Stmt) -> Self::Result;

    fn visit_decl(&mut self, decl: &Decl) -> Self::Result;

    fn visit_stmt_decl_gen(&mut self, gdecl: &GenDecl) -> Self::Result;

    fn visit_stmt_decl_func(&mut self, fdecl: &FuncDeclKey) -> Self::Result;

    fn visit_stmt_labeled(&mut self, lstmt: &LabeledStmtKey) -> Self::Result;

    fn visit_stmt_send(&mut self, sstmt: &SendStmt) -> Self::Result;

    fn visit_stmt_incdec(&mut self, idcstmt: &IncDecStmt) -> Self::Result;

    fn visit_stmt_assign(&mut self, astmt: &AssignStmtKey) -> Self::Result;

    fn visit_stmt_go(&mut self, gostmt: &GoStmt) -> Self::Result;

    fn visit_stmt_defer(&mut self, dstmt: &DeferStmt) -> Self::Result;

    fn visit_stmt_return(&mut self, rstmt: &ReturnStmt) -> Self::Result;

    fn visit_stmt_branch(&mut self, bstmt: &BranchStmt) -> Self::Result;

    fn visit_stmt_block(&mut self, bstmt: &BlockStmt) -> Self::Result;

    fn visit_stmt_if(&mut self, ifstmt: &IfStmt) -> Self::Result;

    fn visit_stmt_case(&mut self, cclause: &CaseClause) -> Self::Result;

    fn visit_stmt_switch(&mut self, sstmt: &SwitchStmt) -> Self::Result;

    fn visit_stmt_type_switch(&mut self, tstmt: &TypeSwitchStmt) -> Self::Result;

    fn visit_stmt_comm(&mut self, cclause: &CommClause) -> Self::Result;

    fn visit_stmt_select(&mut self, sstmt: &SelectStmt) -> Self::Result;

    fn visit_stmt_for(&mut self, fstmt: &ForStmt) -> Self::Result;

    fn visit_stmt_range(&mut self, rstmt: &RangeStmt) -> Self::Result;

    fn visit_empty_stmt(&mut self, e: &EmptyStmt) -> Self::Result;

    fn visit_bad_stmt(&mut self, b: &BadStmt) -> Self::Result;

    fn visit_bad_decl(&mut self, b: &BadDecl) -> Self::Result;
}

pub fn walk_expr<R>(v: &mut dyn ExprVisitor<Result = R>, expr: &Expr) -> R {
    match expr {
        Expr::Bad(e) => v.visit_bad_expr(e.as_ref()),
        Expr::Ident(e) => v.visit_expr_ident(e),
        Expr::Ellipsis(e) => v.visit_expr_ellipsis(&e.as_ref().elt),
        Expr::BasicLit(e) => v.visit_expr_basic_lit(e.as_ref(), expr.id()),
        Expr::FuncLit(e) => v.visit_expr_func_lit(e.as_ref(), expr.id()),
        Expr::CompositeLit(e) => v.visit_expr_composit_lit(e.as_ref()),
        Expr::Paren(e) => v.visit_expr_paren(&e.as_ref().expr),
        Expr::Selector(e) => {
            let selexp = e.as_ref();
            v.visit_expr_selector(&selexp.expr, &selexp.sel, expr.id())
        }
        Expr::Index(e) => {
            let indexp = e.as_ref();
            v.visit_expr_index(&indexp.expr, &indexp.index)
        }
        Expr::Slice(e) => {
            let slexp = e.as_ref();
            v.visit_expr_slice(&slexp.expr, &slexp.low, &slexp.high, &slexp.max)
        }
        Expr::TypeAssert(e) => {
            let taexp = e.as_ref();
            v.visit_expr_type_assert(&taexp.expr, &taexp.typ)
        }
        Expr::Call(e) => {
            let callexp = e.as_ref();
            v.visit_expr_call(&callexp.func, &callexp.args, callexp.ellipsis.is_some())
        }
        Expr::Star(e) => v.visit_expr_star(&e.as_ref().expr),
        Expr::Unary(e) => {
            let uexp = e.as_ref();
            v.visit_expr_unary(&uexp.expr, &uexp.op)
        }
        Expr::Binary(e) => {
            let bexp = e.as_ref();
            v.visit_expr_binary(&bexp.expr_a, &bexp.op, &bexp.expr_b)
        }
        Expr::KeyValue(e) => {
            let kvexp = e.as_ref();
            v.visit_expr_key_value(&kvexp.key, &kvexp.val)
        }
        Expr::Array(e) => v.visit_expr_array_type(&e.as_ref().len, &e.as_ref().elt, expr),
        Expr::Struct(e) => v.visit_expr_struct_type(e.as_ref()),
        Expr::Func(e) => v.visit_expr_func_type(e),
        Expr::Interface(e) => v.visit_expr_interface_type(e.as_ref()),
        Expr::Map(e) => {
            let mexp = e.as_ref();
            v.visit_map_type(&mexp.key, &mexp.val, expr)
        }
        Expr::Chan(e) => {
            let cexp = e.as_ref();
            v.visit_chan_type(&cexp.val, &cexp.dir)
        }
    }
}

pub fn walk_stmt<V: StmtVisitor<Result = R> + ExprVisitor<Result = R>, R>(
    v: &mut V,
    stmt: &Stmt,
) -> R {
    match stmt {
        Stmt::Bad(b) => v.visit_bad_stmt(b),
        Stmt::Decl(decl) => v.visit_decl(decl),
        Stmt::Empty(e) => v.visit_empty_stmt(e),
        Stmt::Labeled(lstmt) => v.visit_stmt_labeled(lstmt),
        Stmt::Expr(expr) => v.visit_expr(expr),
        Stmt::Send(sstmt) => v.visit_stmt_send(sstmt),
        Stmt::IncDec(idstmt) => v.visit_stmt_incdec(idstmt),
        Stmt::Assign(astmt) => v.visit_stmt_assign(astmt),
        Stmt::Go(gostmt) => v.visit_stmt_go(gostmt),
        Stmt::Defer(dstmt) => v.visit_stmt_defer(dstmt),
        Stmt::Return(rstmt) => v.visit_stmt_return(rstmt),
        Stmt::Branch(bstmt) => v.visit_stmt_branch(bstmt),
        Stmt::Block(bstmt) => v.visit_stmt_block(bstmt),
        Stmt::If(ifstmt) => v.visit_stmt_if(ifstmt),
        Stmt::Case(cclause) => v.visit_stmt_case(cclause),
        Stmt::Switch(sstmt) => v.visit_stmt_switch(sstmt),
        Stmt::TypeSwitch(tsstmt) => v.visit_stmt_type_switch(tsstmt),
        Stmt::Comm(cclause) => v.visit_stmt_comm(cclause),
        Stmt::Select(sstmt) => v.visit_stmt_select(sstmt),
        Stmt::For(forstmt) => v.visit_stmt_for(forstmt),
        Stmt::Range(rstmt) => v.visit_stmt_range(rstmt),
    }
}

pub fn walk_decl<R>(v: &mut dyn StmtVisitor<Result = R>, decl: &Decl) -> R {
    match decl {
        Decl::Bad(b) => v.visit_bad_decl(b),
        Decl::Gen(gdecl) => v.visit_stmt_decl_gen(gdecl),
        Decl::Func(fdecl) => v.visit_stmt_decl_func(fdecl),
    }
}
