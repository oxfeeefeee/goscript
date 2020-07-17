use super::super::constant;
use super::super::obj::LangObj;
use super::super::objects::{DeclInfoKey, ObjKey, PackageKey, ScopeKey, TCObjects, TypeKey};
use super::super::operand::Operand;
use super::check::{Checker, ExprInfo, FilesContext, ObjContext};
use goscript_parser::ast::{BlockStmt, Expr, Stmt};
use goscript_parser::objects::{FuncDeclKey, Objects as AstObjects};
use std::rc::Rc;

#[derive(Clone)]
struct StmtContext {
    break_ok: bool,
    continue_ok: bool,
    fallthrough_ok: bool,
    final_switch_case: bool,
}

impl StmtContext {
    fn new() -> StmtContext {
        StmtContext {
            break_ok: false,
            continue_ok: false,
            fallthrough_ok: false,
            final_switch_case: false,
        }
    }
}

pub enum BodyContainer {
    FuncLitExpr(Expr),
    FuncDecl(FuncDeclKey),
}

impl BodyContainer {
    pub fn get_block<'a>(&'a self, objs: &'a AstObjects) -> &'a Rc<BlockStmt> {
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
        fctx: &mut FilesContext,
    ) {
        let block = body.get_block(self.ast_objs);
        let (pos, end) = (block.pos(), block.end());
        if self.config().trace_checker {
            let td = self.new_dis(&sig);
            self.print_trace(pos, &format!("--- {}: {}", name, td));
        }
        // set function scope extent
        let scope_key = self.otype(sig).try_as_signature().unwrap().scope().unwrap();
        let scope = &mut self.tc_objs.scopes[scope_key];
        scope.set_pos(pos);
        scope.set_end(end);

        let mut octx = ObjContext::new();
        octx.decl = Some(di);
        octx.scope = Some(scope_key);
        octx.iota = iota;
        octx.sig = Some(sig);
        std::mem::swap(&mut self.octx, &mut octx);
        let old_indent = self.indent.replace(0);

        let sctx = StmtContext::new();
        let block2 = block.clone();
        self.stmt_list(&block2, sctx, fctx);

        if self.octx.has_label {
            self.labels(&block2);
        }

        let ret_pos = block2.r_brace;
        let stmt = Stmt::Block(block2);
        let sig_val = self.otype(sig).try_as_signature().unwrap();
        if sig_val.results_count(self.tc_objs) > 0 && self.is_terminating(&stmt, None) {
            self.error_str(ret_pos, "missing return");
        }

        // spec: "Implementation restriction: A compiler may make it illegal to
        // declare a variable inside a function body if the variable is never used."
        self.usage(scope_key);

        std::mem::swap(&mut self.octx, &mut octx); // restore octx
        self.indent.replace(old_indent); //restore indent
        if self.config().trace_checker {
            self.print_trace(end, "--- <end>");
        }
    }

    fn usage(&self, skey: ScopeKey) {
        let sval = &self.tc_objs.scopes[skey];
        let mut used: Vec<&LangObj> = sval
            .elems()
            .iter()
            .filter_map(|(_, &okey)| {
                let lobj = &self.tc_objs.lobjs[okey];
                if lobj.entity_type().is_var() {
                    Some(lobj)
                } else {
                    None
                }
            })
            .collect();
        used.sort_by(|a, b| a.pos().cmp(&b.pos()));

        for lo in used.iter() {
            self.soft_error(lo.pos(), format!("{} declared but not used", lo.name()));
        }
        for skey in sval.children().iter() {
            self.usage(*skey);
        }
    }

    fn stmt_list(&mut self, block: &Rc<BlockStmt>, sctx: StmtContext, fctx: &mut FilesContext) {
        unimplemented!()
    }

    fn stmt(&mut self, stmt: &Stmt, sctx: StmtContext, fctx: &mut FilesContext) {
        unimplemented!()
    }
}
