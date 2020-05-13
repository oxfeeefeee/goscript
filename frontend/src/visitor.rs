use super::ast::*;
use super::ast_objects::*;
use super::token::Token;

pub trait Visitor {
    fn visit_expr(&mut self, expr: &Expr);

    fn visit_stmt(&mut self, stmt: &Stmt);

    fn visit_decl(&mut self, decl: &Decl);

    fn visit_expr_ident(&mut self, ident: &IdentKey);

    fn visit_expr_option(&mut self, op: &Option<Expr>);

    fn visit_expr_ellipsis(&mut self);

    fn visit_expr_basic_lit(&mut self, blit: &BasicLit);

    fn visit_expr_func_lit(&mut self, flit: &FuncLit);

    fn visit_expr_composit_lit(&mut self, clit: &CompositeLit);

    fn visit_expr_paren(&mut self);

    fn visit_expr_selector(&mut self, ident: &IdentKey); //add: lvalue

    fn visit_expr_index(&mut self);

    fn visit_expr_slice(&mut self);

    fn visit_expr_type_assert(&mut self);

    fn visit_expr_call(&mut self, func: &Expr, args: &Vec<Expr>, ellipsis: bool);

    fn visit_expr_star(&mut self);

    fn visit_expr_unary(&mut self, op: &Token);

    fn visit_expr_binary(&mut self, op: &Token);

    fn visit_expr_key_value(&mut self);

    fn visit_expr_array_type(&mut self);

    fn visit_expr_slice_type(&mut self);

    fn visit_expr_struct_type(&mut self, s: &StructType);

    fn visit_expr_func_type(&mut self, s: &FuncType);

    fn visit_expr_interface_type(&mut self, s: &InterfaceType);

    fn visit_map_type(&mut self);

    fn visit_chan_type(&mut self, dir: &ChanDir);

    fn visit_stmt_decl_gen(&mut self, gdecl: &GenDecl);

    fn visit_stmt_decl_func(&mut self, fdecl: &FuncDeclKey);

    fn visit_stmt_labeled(&mut self, lstmt: &LabeledStmtKey);

    fn visit_stmt_send(&mut self, sstmt: &SendStmt);

    fn visit_stmt_incdec(&mut self, idcstmt: &IncDecStmt);

    fn visit_stmt_assign(&mut self, astmt: &AssignStmtKey);

    fn visit_stmt_go(&mut self, gostmt: &GoStmt);

    fn visit_stmt_defer(&mut self, dstmt: &DeferStmt);

    fn visit_stmt_return(&mut self, rstmt: &ReturnStmt);

    fn visit_stmt_branch(&mut self, bstmt: &BranchStmt);

    fn visit_stmt_block(&mut self, bstmt: &BlockStmt);

    fn visit_stmt_if(&mut self, ifstmt: &IfStmt);

    fn visit_stmt_case(&mut self, cclause: &CaseClause);

    fn visit_stmt_switch(&mut self, sstmt: &SwitchStmt);

    fn visit_stmt_type_switch(&mut self, tstmt: &TypeSwitchStmt);

    fn visit_stmt_comm(&mut self, cclause: &CommClause);

    fn visit_stmt_select(&mut self, sstmt: &SelectStmt);

    fn visit_stmt_for(&mut self, fstmt: &ForStmt);

    fn visit_stmt_range(&mut self, rstmt: &RangeStmt);
}

pub fn walk_expr(v: &mut dyn Visitor, expr: &Expr) {
    match expr {
        Expr::Bad(_) => {}
        Expr::Ident(e) => {
            v.visit_expr_ident(e.as_ref());
        }
        Expr::Ellipsis(e) => {
            v.visit_expr_option(&e.as_ref().elt);
            v.visit_expr_ellipsis();
        }
        Expr::BasicLit(e) => {
            v.visit_expr_basic_lit(e.as_ref());
        }
        Expr::FuncLit(e) => {
            v.visit_expr_func_lit(e.as_ref());
        }
        Expr::CompositeLit(e) => {
            v.visit_expr_composit_lit(e.as_ref());
        }
        Expr::Paren(e) => {
            v.visit_expr(&e.as_ref().expr);
            v.visit_expr_paren();
        }
        Expr::Selector(e) => {
            let selexp = e.as_ref();
            v.visit_expr(&selexp.expr);
            v.visit_expr_selector(&selexp.sel);
        }
        Expr::Index(e) => {
            let indexp = e.as_ref();
            v.visit_expr(&indexp.expr);
            v.visit_expr(&indexp.index);
            v.visit_expr_index();
        }
        Expr::Slice(e) => {
            let slexp = e.as_ref();
            v.visit_expr(&slexp.expr);
            v.visit_expr_option(&slexp.low);
            v.visit_expr_option(&slexp.high);
            v.visit_expr_option(&slexp.max);
            v.visit_expr_slice();
        }
        Expr::TypeAssert(e) => {
            let taexp = e.as_ref();
            v.visit_expr(&taexp.expr);
            v.visit_expr_option(&taexp.typ);
            v.visit_expr_type_assert();
        }
        Expr::Call(e) => {
            let callexp = e.as_ref();
            v.visit_expr_call(&callexp.func, &callexp.args, callexp.ellipsis.is_some());
        }
        Expr::Star(e) => {
            let starexp = e.as_ref();
            v.visit_expr(&starexp.expr);
            v.visit_expr_star();
        }
        Expr::Unary(e) => {
            let uexp = e.as_ref();
            v.visit_expr(&uexp.expr);
            v.visit_expr_unary(&uexp.op);
        }
        Expr::Binary(e) => {
            let bexp = e.as_ref();
            v.visit_expr(&bexp.expr_a);
            v.visit_expr(&bexp.expr_b);
            v.visit_expr_binary(&bexp.op);
        }
        Expr::KeyValue(e) => {
            let kvexp = e.as_ref();
            v.visit_expr(&kvexp.key);
            v.visit_expr(&kvexp.val);
            v.visit_expr_key_value();
        }
        Expr::Array(e) => {
            let aexp = e.as_ref();
            v.visit_expr(&aexp.elt);
            match &aexp.len {
                Some(exp) => {
                    v.visit_expr(&exp);
                    v.visit_expr_array_type();
                }
                None => {
                    v.visit_expr_slice_type();
                }
            }
        }
        Expr::Struct(e) => {
            v.visit_expr_struct_type(e.as_ref());
        }
        Expr::Func(e) => {
            v.visit_expr_func_type(e.as_ref());
        }
        Expr::Interface(e) => {
            v.visit_expr_interface_type(e.as_ref());
        }
        Expr::Map(e) => {
            let mexp = e.as_ref();
            v.visit_expr(&mexp.key);
            v.visit_expr(&mexp.val);
            v.visit_map_type();
        }
        Expr::Chan(e) => {
            let cexp = e.as_ref();
            v.visit_expr(&cexp.val);
            v.visit_chan_type(&cexp.dir);
        }
    };
}

pub fn walk_stmt(v: &mut dyn Visitor, stmt: &Stmt) {
    match stmt {
        Stmt::Bad(_) => {}
        Stmt::Decl(decl) => v.visit_decl(decl),
        Stmt::Empty(_) => {}
        Stmt::Labeled(lstmt) => v.visit_stmt_labeled(lstmt),
        Stmt::Expr(expr) => v.visit_expr(expr),
        Stmt::Send(sstmt) => v.visit_stmt_send(sstmt),
        Stmt::IncDec(idstmt) => v.visit_stmt_incdec(idstmt),
        Stmt::Assign(astmt) => v.visit_stmt_assign(astmt.as_ref()),
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

pub fn walk_decl(v: &mut dyn Visitor, decl: &Decl) {
    match decl {
        Decl::Bad(_) => {}
        Decl::Gen(gdecl) => v.visit_stmt_decl_gen(gdecl),
        Decl::Func(fdecl) => v.visit_stmt_decl_func(fdecl.as_ref()),
    }
}
