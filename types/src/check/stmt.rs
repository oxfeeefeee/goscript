#![allow(dead_code)]
use super::super::constant;
use super::super::obj::{EntityType, LangObj};
use super::super::objects::{DeclInfoKey, ScopeKey, TypeKey};
use super::super::operand::{Operand, OperandMode};
use super::super::typ::{self, BasicInfo, BasicType, ChanDir, Type};
use super::super::universe::ExprKind;
use super::check::{Checker, FilesContext, ObjContext};
use constant::Value;
use goscript_parser::ast::{
    BasicLit, BlockStmt, CaseClause, CommClause, Expr, Ident, Node, Stmt, TypeAssertExpr,
};
use goscript_parser::objects::{FuncDeclKey, IdentKey, Objects as AstObjects};
use goscript_parser::{Pos, Token};
use ordered_float;
use std::collections::HashMap;
use std::rc::Rc;

type F64 = ordered_float::OrderedFloat<f64>;

#[derive(Clone, Copy, Debug)]
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

#[derive(Clone, Eq, PartialEq, Hash)]
enum GoVal {
    Int64(i64),
    Uint64(u64),
    Float64(F64),
    Str(String),
    Invalid,
}

impl GoVal {
    fn with_const(v: &Value) -> GoVal {
        match v {
            Value::Int(_) => match v.int_as_i64() {
                (int, true) => GoVal::Int64(int),
                _ => match v.int_as_u64() {
                    (uint, true) => GoVal::Uint64(uint),
                    _ => GoVal::Invalid,
                },
            },
            Value::Float(_) => match v.num_as_f64() {
                (f, true) => GoVal::Float64(f),
                _ => GoVal::Invalid,
            },
            Value::Str(_) => GoVal::Str(v.str_as_string()),
            _ => GoVal::Invalid,
        }
    }
}

struct PosType {
    pos: Pos,
    typ: TypeKey,
}

type ValueMap = HashMap<GoVal, Vec<PosType>>;

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
        self.stmt_list(&block2.list, &sctx, fctx);

        if self.octx.has_label {
            self.labels(&block2);
        }

        let ret_pos = block2.r_brace;
        let stmt = Stmt::Block(block2);
        let sig_val = self.otype(sig).try_as_signature().unwrap();
        if sig_val.results_count(self.tc_objs) > 0 && !self.is_terminating(&stmt, None) {
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
        let mut unused: Vec<&LangObj> = sval
            .elems()
            .iter()
            .filter_map(|(_, &okey)| {
                let lobj = &self.tc_objs.lobjs[okey];
                match lobj.entity_type() {
                    EntityType::Var(var) => {
                        if !var.used {
                            Some(lobj)
                        } else {
                            None
                        }
                    }
                    _ => None,
                }
            })
            .collect();
        unused.sort_by(|a, b| a.pos().cmp(&b.pos()));

        for lo in unused.iter() {
            self.soft_error(lo.pos(), format!("{} declared but not used", lo.name()));
        }
        for skey in sval.children().iter() {
            self.usage(*skey);
        }
    }

    fn simple_stmt(&mut self, s: Option<&Stmt>, fctx: &mut FilesContext) {
        if let Some(s) = s {
            let sctx = StmtContext::new();
            self.stmt(s, &sctx, fctx);
        }
    }

    fn stmt_list(&mut self, list: &Vec<Stmt>, sctx: &StmtContext, fctx: &mut FilesContext) {
        // trailing empty statements are "invisible" to fallthrough analysis
        let index = list
            .iter()
            .enumerate()
            .rev()
            .find_map(|(i, x)| match x {
                Stmt::Empty(_) => None,
                _ => Some(i + 1),
            })
            .unwrap_or(list.len());
        for (i, s) in list[0..index].iter().enumerate() {
            let mut inner = *sctx;
            inner.fallthrough_ok = sctx.fallthrough_ok && i + 1 == index;
            self.stmt(s, &inner, fctx);
        }
    }

    fn multiple_defaults(&self, list: &Vec<Stmt>) {
        let mut first: Option<&Stmt> = None;
        for s in list.iter() {
            let d_op = match s {
                Stmt::Case(cc) => cc.list.as_ref().map_or(Some(s), |_| None),
                Stmt::Comm(cc) => cc.comm.as_ref().map_or(Some(s), |_| None),
                _ => {
                    self.invalid_ast(s.pos(self.ast_objs), "case/communication clause expected");
                    None
                }
            };
            if let Some(d) = d_op {
                match first {
                    Some(f) => self.error(
                        d.pos(self.ast_objs),
                        format!(
                            "multiple defaults (first at {})",
                            self.position(f.pos(self.ast_objs))
                        ),
                    ),
                    None => first = Some(d),
                }
            }
        }
    }

    fn open_scope(&mut self, s: &Stmt, comment: String) {
        let scope = self.tc_objs.new_scope(
            self.octx.scope,
            s.pos(self.ast_objs),
            s.end(self.ast_objs),
            comment,
            false,
        );
        self.result.record_scope(s, scope);
        self.octx.scope = Some(scope);
    }

    fn close_scope(&mut self) {
        self.octx.scope = *self.tc_objs.scopes[self.octx.scope.unwrap()].parent();
    }

    fn assign_op(op: &Token) -> Option<Token> {
        match op {
            Token::ADD_ASSIGN => Some(Token::ADD),
            Token::SUB_ASSIGN => Some(Token::SUB),
            Token::MUL_ASSIGN => Some(Token::MUL),
            Token::QUO_ASSIGN => Some(Token::QUO),
            Token::REM_ASSIGN => Some(Token::REM),
            Token::AND_ASSIGN => Some(Token::AND),
            Token::OR_ASSIGN => Some(Token::OR),
            Token::XOR_ASSIGN => Some(Token::XOR),
            Token::SHL_ASSIGN => Some(Token::SHL),
            Token::SHR_ASSIGN => Some(Token::SHR),
            Token::AND_NOT_ASSIGN => Some(Token::AND_NOT),
            _ => None,
        }
    }

    fn suspended_call(&mut self, kw: &str, call: &Expr, fctx: &mut FilesContext) {
        let x = &mut Operand::new();
        let msg = match self.raw_expr(x, call, None, fctx) {
            ExprKind::Conversion => "requires function call, not conversion",
            ExprKind::Expression => "discards result of",
            ExprKind::Statement => return,
        };
        let xd = self.new_dis(x);
        self.error(xd.pos(), format!("{} {} {}", kw, msg, xd));
    }

    fn case_values(
        &mut self,
        x: &mut Operand,
        values: &Option<Vec<Expr>>,
        seen: &mut ValueMap,
        fctx: &mut FilesContext,
    ) {
        if values.is_none() {
            return;
        }
        for e in values.as_ref().unwrap().iter() {
            let v = &mut Operand::new();
            self.expr(v, e, fctx);
            if x.invalid() || v.invalid() {
                continue;
            }
            self.convert_untyped(v, x.typ.unwrap(), fctx);
            if v.invalid() {
                continue;
            }
            // Order matters: By comparing v against x, error positions are at the case values.
            let res = &mut v.clone();
            self.comparison(res, x, &Token::EQL, fctx);
            if res.invalid() {
                continue;
            }
            if let OperandMode::Constant(val) = &v.mode {
                // look for duplicate values
                match GoVal::with_const(val) {
                    GoVal::Invalid => {}
                    gov => {
                        let entry = seen.entry(gov).or_insert(vec![]);
                        if let Some(pt) = entry
                            .iter()
                            .find(|x| typ::identical(v.typ.unwrap(), x.typ, self.tc_objs))
                        {
                            let vd = self.new_dis(v);
                            self.error(
                                vd.pos(),
                                format!("duplicate case {} in expression switch", vd),
                            );
                            self.error_str(pt.pos, "\tprevious case");
                            continue;
                        }
                        entry.push(PosType {
                            pos: v.pos(self.ast_objs),
                            typ: v.typ.unwrap(),
                        });
                    }
                }
            }
        }
    }

    fn case_types(
        &mut self,
        x: &mut Operand,
        xtype: TypeKey,
        types: &Option<Vec<Expr>>,
        seen: &mut HashMap<Option<TypeKey>, Pos>,
        fctx: &mut FilesContext,
    ) -> Option<TypeKey> {
        if types.is_none() {
            return None;
        }
        types
            .as_ref()
            .unwrap()
            .iter()
            .filter_map(|e| {
                let t = self.type_or_nil(e, fctx);
                if t == Some(self.invalid_type()) {
                    return None;
                }
                if let Some((_, &pos)) = seen
                    .iter()
                    .find(|(&t2, _)| typ::identical_option(t, t2, self.tc_objs))
                {
                    let ts = t.map_or("nil".to_string(), |x| self.new_dis(&x).to_string());
                    self.error(
                        e.pos(self.ast_objs),
                        format!("duplicate case {} in type switch", ts),
                    );
                    self.error_str(pos, "\tprevious case");
                    return None;
                }
                let pos = e.pos(self.ast_objs);
                seen.insert(t, pos);
                if let Some(t) = t {
                    self.type_assertion(Some(pos), x, xtype, t);
                }
                Some(t)
            })
            .last()
            .flatten()
    }

    fn stmt(&mut self, stmt: &Stmt, ctx: &StmtContext, fctx: &mut FilesContext) {
        let begin_scope = self.octx.scope;
        let begin_delayed_count = fctx.delayed_count();

        let mut inner_ctx = ctx.clone();
        inner_ctx.fallthrough_ok = false;
        inner_ctx.final_switch_case = false;
        match stmt {
            Stmt::Bad(_) | Stmt::Empty(_) => {} //ignore
            Stmt::Decl(d) => self.decl_stmt((**d).clone(), fctx),
            Stmt::Labeled(lkey) => {
                self.octx.has_label = true;
                let s = &self.ast_objs.l_stmts[*lkey].stmt.clone();
                self.stmt(&s, ctx, fctx);
            }
            Stmt::Expr(e) => {
                // spec: "With the exception of specific built-in functions,
                // function and method calls and receive operations can appear
                // in statement context. Such statements may be parenthesized."
                let x = &mut Operand::new();
                let kind = self.raw_expr(x, e, None, fctx);
                let msg = match &x.mode {
                    OperandMode::Builtin(_) => "must be called",
                    OperandMode::TypeExpr => "is not an expression",
                    _ => {
                        if kind == ExprKind::Statement {
                            return;
                        }
                        "is not used"
                    }
                };
                let xd = self.new_dis(x);
                self.error(xd.pos(), format!("{} {}", xd, msg));
            }
            Stmt::Send(ss) => {
                let (ch, x) = (&mut Operand::new(), &mut Operand::new());
                self.expr(ch, &ss.chan, fctx);
                self.expr(x, &ss.val, fctx);
                if ch.invalid() || x.invalid() {
                    return;
                }
                let chtype = ch.typ.unwrap();
                let under_chtype = typ::underlying_type(chtype, self.tc_objs);
                if let Some(chan) = self.otype(under_chtype).try_as_chan() {
                    if chan.dir() == typ::ChanDir::RecvOnly {
                        let td = self.new_dis(&under_chtype);
                        self.invalid_op(
                            ss.arrow,
                            &format!("cannot send to receive-only type {}", td),
                        );
                    } else {
                        let ty = Some(chan.elem());
                        self.assignment(x, ty, "send", fctx);
                    }
                } else {
                    let td = self.new_dis(&chtype);
                    self.invalid_op(ss.arrow, &format!("cannot send to non-chan type {}", td));
                }
            }
            Stmt::IncDec(ids) => {
                let op = match &ids.token {
                    Token::INC => Token::ADD,
                    Token::DEC => Token::SUB,
                    _ => {
                        self.invalid_ast(
                            ids.token_pos,
                            &format!("unknown inc/dec operation {}", ids.token),
                        );
                        return;
                    }
                };
                let x = &mut Operand::new();
                self.expr(x, &ids.expr, fctx);
                if x.invalid() {
                    return;
                }
                if !typ::is_numeric(x.typ.unwrap(), self.tc_objs) {
                    let ed = self.new_dis(&ids.expr);
                    let td = self.new_dis(x.typ.as_ref().unwrap());
                    self.invalid_op(
                        ed.pos(),
                        &format!("{}{} (non-numeric type {})", ed, ids.token, td),
                    );
                    return;
                }
                let one = Expr::BasicLit(Rc::new(BasicLit {
                    pos: x.pos(self.ast_objs),
                    token: Token::int1(),
                }));
                self.binary(x, None, &ids.expr, &one, &op, fctx);
                if x.invalid() {
                    return;
                }
                self.assign_var(&ids.expr, x, fctx);
            }
            Stmt::Assign(askey) => {
                let astmt = &self.ast_objs.a_stmts[*askey];
                match &astmt.token {
                    Token::ASSIGN | Token::DEFINE => {
                        if astmt.lhs.len() == 0 {
                            let pos = astmt.pos(self.ast_objs);
                            self.invalid_ast(pos, "missing lhs in assignment");
                            return;
                        }
                        let (lhs, rhs, pos) =
                            (astmt.lhs.clone(), astmt.rhs.clone(), astmt.token_pos);
                        if astmt.token == Token::DEFINE {
                            self.short_var_decl(&lhs, &rhs, pos, fctx);
                        } else {
                            self.assign_vars(&lhs, &rhs, fctx);
                        }
                    }
                    _ => {
                        // assignment operations
                        if astmt.lhs.len() != 1 || astmt.rhs.len() != 1 {
                            self.error(
                                astmt.token_pos,
                                format!(
                                    "assignment operation {} requires single-valued expressions",
                                    astmt.token
                                ),
                            );
                            return;
                        }
                        let op = Checker::assign_op(&astmt.token);
                        if op.is_none() {
                            self.invalid_ast(
                                astmt.token_pos,
                                &format!("unknown assignment operation {}", astmt.token),
                            );
                            return;
                        }
                        let (lhs, rhs, op) =
                            (astmt.lhs[0].clone(), astmt.rhs[0].clone(), op.unwrap());
                        let x = &mut Operand::new();
                        self.binary(x, None, &lhs, &rhs, &op, fctx);
                        if x.invalid() {
                            return;
                        }
                        self.assign_var(&lhs, x, fctx);
                    }
                }
            }
            Stmt::Go(gs) => self.suspended_call("go", &gs.call, fctx),
            Stmt::Defer(ds) => self.suspended_call("defer", &ds.call, fctx),
            Stmt::Return(rs) => {
                let reskey = self
                    .otype(self.octx.sig.unwrap())
                    .try_as_signature()
                    .unwrap()
                    .results();
                let res = self.otype(reskey).try_as_tuple().unwrap();
                if res.vars().len() > 0 {
                    // function returns results
                    // (if one, say the first, result parameter is named, all of them are named)
                    if rs.results.len() == 0 && self.lobj(res.vars()[0]).name() != "" {
                        // spec: "Implementation restriction: A compiler may disallow an empty expression
                        // list in a "return" statement if a different entity (constant, type, or variable)
                        // with the same name as a result parameter is in scope at the place of the return."
                        for &okey in res.vars().iter() {
                            let lobj = self.lobj(okey);
                            if let Some(alt) = self.lookup(lobj.name()) {
                                if alt == okey {
                                    continue;
                                }
                                self.error(
                                    stmt.pos(self.ast_objs),
                                    format!(
                                        "result parameter {} not in scope at return",
                                        lobj.name()
                                    ),
                                );
                                let (altd, objd) = (self.new_dis(&alt), self.new_dis(&okey));
                                self.error(altd.pos(), format!("\tinner declaration of {}", objd));
                                // ok to continue
                            }
                        }
                    } else {
                        // return has results or result parameters are unnamed
                        let vars = res.vars().clone();
                        self.init_vars(&vars, &rs.results, Some(rs.ret), fctx);
                    }
                } else if rs.results.len() > 0 {
                    self.error_str(
                        rs.results[0].pos(self.ast_objs),
                        "no result values expected",
                    );
                    self.use_exprs(&rs.results, fctx);
                }
            }
            Stmt::Branch(bs) => {
                if bs.label.is_some() {
                    self.octx.has_label = true;
                    return; //checked in 2nd pass (Check::label)
                }
                let spos = stmt.pos(self.ast_objs);
                match &bs.token {
                    Token::BREAK => {
                        if !ctx.break_ok {
                            self.error_str(spos, "break not in for, switch, or select statement");
                        }
                    }
                    Token::CONTINUE => {
                        if !ctx.continue_ok {
                            self.error_str(spos, "continue not in for statement");
                        }
                    }
                    Token::FALLTHROUGH => {
                        if !ctx.fallthrough_ok {
                            let msg = if ctx.final_switch_case {
                                "cannot fallthrough final case in switch"
                            } else {
                                "fallthrough statement out of place"
                            };
                            self.error_str(spos, msg);
                        }
                    }
                    _ => {
                        self.invalid_ast(spos, &format!("branch statement: {}", bs.token));
                    }
                }
            }
            Stmt::Block(bs) => {
                self.open_scope(stmt, "block".to_string());

                self.stmt_list(&bs.list, &inner_ctx, fctx);

                self.close_scope();
            }
            Stmt::If(ifs) => {
                self.open_scope(stmt, "if".to_string());

                self.simple_stmt(ifs.init.as_ref(), fctx);
                let x = &mut Operand::new();
                self.expr(x, &ifs.cond, fctx);
                if !x.invalid() && !typ::is_boolean(x.typ.unwrap(), self.tc_objs) {
                    self.error_str(
                        ifs.cond.pos(self.ast_objs),
                        "non-boolean condition in if statement",
                    );
                }
                self.stmt(&Stmt::Block(ifs.body.clone()), &inner_ctx, fctx);
                // The parser produces a correct AST but if it was modified
                // elsewhere the else branch may be invalid. Check again.
                if let Some(s) = &ifs.els {
                    match s {
                        Stmt::Bad(_) => {} //error already reported
                        Stmt::If(_) | Stmt::Block(_) => {
                            self.stmt(s, &inner_ctx, fctx);
                        }
                        _ => {
                            let pos = s.pos(self.ast_objs);
                            self.error_str(pos, "invalid else branch in if statement");
                        }
                    }
                }

                self.close_scope();
            }
            Stmt::Switch(ss) => {
                inner_ctx.break_ok = true;
                self.open_scope(stmt, "switch".to_string());

                self.simple_stmt(ss.init.as_ref(), fctx);
                let x = &mut Operand::new();
                if let Some(tag) = &ss.tag {
                    self.expr(x, tag, fctx);
                    // By checking assignment of x to an invisible temporary
                    // (as a compiler would), we get all the relevant checks.
                    self.assignment(x, None, "switch expression", fctx);
                } else {
                    // spec: "A missing switch expression is
                    // equivalent to the boolean value true."
                    x.mode = OperandMode::Constant(Value::with_bool(true));
                    x.typ = Some(self.basic_type(BasicType::Bool));
                    x.expr = Some(Expr::Ident(
                        self.ast_objs.idents.insert(Ident::true_(ss.body.l_brace)),
                    ))
                }

                self.multiple_defaults(&ss.body.list);

                let mut seen: ValueMap = HashMap::new();
                for (i, c) in ss.body.list.iter().enumerate() {
                    if let Stmt::Case(cc) = c {
                        self.case_values(x, &cc.list, &mut seen, fctx);
                        self.open_scope(stmt, "case".to_string());
                        let mut inner2 = inner_ctx.clone();
                        if i + 1 < ss.body.list.len() {
                            inner2.fallthrough_ok = true;
                        } else {
                            inner2.final_switch_case = true;
                        }
                        self.stmt_list(&cc.body, &inner2, fctx);
                        self.close_scope();
                    } else {
                        self.invalid_ast(c.pos(self.ast_objs), "incorrect expression switch case");
                    }
                }

                self.close_scope();
            }
            Stmt::TypeSwitch(tss) => {
                inner_ctx.break_ok = true;
                self.open_scope(stmt, "type switch".to_string());

                self.simple_stmt(tss.init.as_ref(), fctx);
                // A type switch guard must be of the form:
                //
                //     TypeSwitchGuard = [ identifier ":=" ] PrimaryExpr "." "(" "type" ")" .
                //
                // The parser is checking syntactic correctness;
                // remaining syntactic errors are considered AST errors here.
                let invalid_ast = || {
                    let spos = stmt.pos(self.ast_objs);
                    self.invalid_ast(spos, "incorrect form of type switch guard");
                };
                let (lhs, rhs): (Option<IdentKey>, &Expr) = match &tss.assign {
                    Stmt::Expr(e) => (None, &*e),
                    Stmt::Assign(ass) => {
                        let assign = &self.ast_objs.a_stmts[*ass];
                        if assign.lhs.len() != 1
                            || assign.token != Token::DEFINE
                            || assign.rhs.len() != 1
                        {
                            invalid_ast();
                            return self.close_scope();
                        }
                        if let Expr::Ident(ikey) = assign.lhs[0] {
                            let ident = &self.ast_objs.idents[ikey];
                            let l = if ident.name == "_" {
                                // _ := x.(type) is an invalid short variable declaration
                                self.soft_error_str(
                                    ident.pos,
                                    "no new variable on left side of :=",
                                );
                                None // avoid declared but not used error below
                            } else {
                                self.result.record_def(ikey, None);
                                Some(ikey)
                            };
                            (l, &assign.rhs[0])
                        } else {
                            invalid_ast();
                            return self.close_scope();
                        }
                    }
                    _ => {
                        invalid_ast();
                        return self.close_scope();
                    }
                };

                // rhs must be of the form: expr.(type) and expr must be an interface
                let ta: &TypeAssertExpr = match rhs {
                    Expr::TypeAssert(e) => e,
                    _ => {
                        let spos = stmt.pos(self.ast_objs);
                        self.invalid_ast(spos, "incorrect form of type switch guard");
                        return self.close_scope();
                    }
                };
                let x = &mut Operand::new();
                let ta_expr = ta.expr.clone();
                self.expr(x, &ta_expr, fctx);
                if x.invalid() {
                    return self.close_scope();
                }
                let xtype = typ::underlying_type(x.typ.unwrap(), self.tc_objs);
                if self.otype(xtype).try_as_interface().is_none() {
                    let xd = self.new_dis(x);
                    self.error(xd.pos(), format!("{} is not an interface", xd));
                    return self.close_scope();
                }

                self.multiple_defaults(&tss.body.list);

                let mut seen = HashMap::new();
                let mut lhs_vars = Vec::new();
                for s in tss.body.list.iter() {
                    let clause: &CaseClause = match s {
                        Stmt::Case(cc) => cc,
                        _ => {
                            let spos = stmt.pos(self.ast_objs);
                            self.invalid_ast(spos, "incorrect type switch case");
                            continue;
                        }
                    };
                    // Check each type in this type switch case.
                    let mut t = self.case_types(x, xtype, &clause.list, &mut seen, fctx);
                    self.open_scope(stmt, "case".to_string());
                    // If lhs exists, declare a corresponding variable in the case-local scope.
                    if let Some(lhs) = lhs {
                        // spec: "The TypeSwitchGuard may include a short variable declaration.
                        // When that form is used, the variable is declared at the beginning of
                        // the implicit block in each clause. In clauses with a case listing
                        // exactly one type, the variable has that type; otherwise, the variable
                        // has the type of the expression in the TypeSwitchGuard."
                        if clause.list.as_ref().map_or(0, |x| x.len()) != 1 || t.is_none() {
                            t = x.typ;
                        }
                        let ident = self.ast_ident(lhs);
                        let (pos, name) = (ident.pos, ident.name.clone());
                        let okey = self.tc_objs.new_var(pos, Some(self.pkg), name, t);
                        let scope_pos = clause
                            .list
                            .as_ref()
                            .map_or(clause.case + "default".len(), |x| {
                                x[x.len() - 1].end(self.ast_objs)
                            });
                        self.declare(self.octx.scope.unwrap(), None, okey, scope_pos);
                        self.result.record_implicit(s, okey);
                        // For the "declared but not used" error, all lhs variables act as
                        // one; i.e., if any one of them is 'used', all of them are 'used'.
                        // Collect them for later analysis.
                        lhs_vars.push(okey);
                    }
                    self.stmt_list(&clause.body, &inner_ctx, fctx);
                    self.close_scope();
                }

                // If lhs exists, we must have at least one lhs variable that was used.
                if lhs.is_some() {
                    let used = lhs_vars.iter_mut().fold(false, |acc, x| {
                        let prop = self.tc_objs.lobjs[*x].entity_type_mut().var_property_mut();
                        let used = prop.used;
                        prop.used = true; // avoid usage error when checking entire function
                        acc || used
                    });
                    if !used {
                        let ident = self.ast_ident(lhs.unwrap());
                        let (pos, name) = (ident.pos, &ident.name);
                        self.soft_error(pos, format!("{} declared but not used", name));
                    }
                }

                self.close_scope();
            }
            Stmt::Select(ss) => {
                inner_ctx.break_ok = true;

                self.multiple_defaults(&ss.body.list);

                for s in ss.body.list.iter() {
                    let clause: &CommClause = match s {
                        Stmt::Comm(cc) => cc,
                        _ => continue, // error reported before
                    };
                    // clause.Comm must be a SendStmt, RecvStmt, or default case
                    let is_recv = |e: &Expr| match Checker::unparen(e) {
                        Expr::Unary(ue) => ue.op == Token::ARROW,
                        _ => false,
                    };
                    let valid = match &clause.comm {
                        None | Some(Stmt::Send(_)) => true,
                        Some(Stmt::Assign(ass)) => {
                            let assign = &self.ast_objs.a_stmts[*ass];
                            if assign.rhs.len() == 1 {
                                is_recv(&assign.rhs[0])
                            } else {
                                false
                            }
                        }
                        Some(Stmt::Expr(e)) => is_recv(e),
                        _ => false,
                    };
                    if !valid {
                        self.error_str(
                            clause.comm.as_ref().unwrap().pos(self.ast_objs),
                            "select case must be send or receive (possibly with assignment)",
                        );
                        continue;
                    }

                    self.open_scope(stmt, "case".to_string());
                    if let Some(cc) = &clause.comm {
                        self.stmt(cc, &inner_ctx, fctx);
                    }
                    self.stmt_list(&clause.body, &inner_ctx, fctx);
                    self.close_scope()
                }
            }
            Stmt::For(fs) => {
                inner_ctx.break_ok = true;
                inner_ctx.continue_ok = true;
                self.open_scope(stmt, "for".to_string());

                self.simple_stmt(fs.init.as_ref(), fctx);
                if let Some(cond) = &fs.cond {
                    let x = &mut Operand::new();
                    self.expr(x, cond, fctx);
                    if !x.invalid() && !typ::is_boolean(x.typ.unwrap(), self.tc_objs) {
                        self.error_str(
                            cond.pos(self.ast_objs),
                            "non-boolean condition in if statement",
                        );
                    }
                }
                self.simple_stmt(fs.post.as_ref(), fctx);
                // spec: "The init statement may be a short variable
                // declaration, but the post statement must not."
                match &fs.post {
                    Some(Stmt::Assign(ass)) => {
                        let assign = &self.ast_objs.a_stmts[*ass];
                        if assign.token == Token::DEFINE {
                            self.soft_error_str(
                                assign.pos(self.ast_objs),
                                "cannot declare in post statement",
                            );
                            // Don't call useLHS here because we want to use the lhs in
                            // this erroneous statement so that we don't get errors about
                            // these lhs variables being declared but not used.
                            let lhs = assign.lhs.clone();
                            self.use_exprs(&lhs, fctx); // avoid follow-up errors
                        }
                    }
                    _ => {}
                }
                self.stmt(&Stmt::Block(fs.body.clone()), &inner_ctx, fctx);

                self.close_scope()
            }
            Stmt::Range(rs) => {
                inner_ctx.break_ok = true;
                inner_ctx.continue_ok = true;
                self.open_scope(stmt, "for".to_string());

                // check expression to iterate over
                let x = &mut Operand::new();
                self.expr(x, &rs.expr, fctx);

                // determine key/value types
                let (key, val) = if x.invalid() {
                    (None, None)
                } else {
                    match self.otype(x.typ.unwrap()).underlying_val(self.tc_objs) {
                        Type::Basic(detail) if detail.info() == BasicInfo::IsString => (
                            Some(self.basic_type(BasicType::Int)),
                            Some(*self.tc_objs.universe().rune()),
                        ),
                        Type::Array(detail) => {
                            (Some(self.basic_type(BasicType::Int)), Some(detail.elem()))
                        }
                        Type::Slice(detail) => {
                            (Some(self.basic_type(BasicType::Int)), Some(detail.elem()))
                        }
                        Type::Pointer(detail) => {
                            if let Some(d) = self
                                .otype(detail.base())
                                .underlying_val(self.tc_objs)
                                .try_as_array()
                            {
                                (Some(self.basic_type(BasicType::Int)), Some(d.elem()))
                            } else {
                                (None, None)
                            }
                        }
                        Type::Map(detail) => (Some(detail.key()), Some(detail.elem())),
                        Type::Chan(detail) => {
                            if detail.dir() == ChanDir::SendOnly {
                                let xd = self.new_dis(x);
                                self.error(
                                    xd.pos(),
                                    format!("cannot range over send-only channel {}", xd),
                                );
                                // ok to continue
                            }
                            if let Some(v) = &rs.val {
                                self.error(
                                    v.pos(self.ast_objs),
                                    format!(
                                        "iteration over {} permits only one iteration variable",
                                        self.new_dis(x)
                                    ),
                                );
                                // ok to continue
                            }
                            (Some(detail.elem()), Some(self.invalid_type()))
                        }
                        _ => (None, None),
                    }
                };

                if key.is_none() {
                    let xd = self.new_dis(x);
                    self.error(xd.pos(), format!("cannot range over {}", xd));
                    // ok to continue
                }

                // check assignment to/declaration of iteration variables
                // (irregular assignment, cannot easily map to existing assignment checks)

                // lhs expressions and initialization value (rhs) types
                let lhs = [rs.key.as_ref(), rs.val.as_ref()];
                let rhs = [key, val];
                if rs.token == Token::DEFINE {
                    let mut vars = vec![];
                    for (i, lhs) in lhs.iter().enumerate() {
                        if lhs.is_none() {
                            continue;
                        }
                        // determine lhs variable
                        let okey = match lhs.unwrap() {
                            Expr::Ident(ikey) => {
                                let ident = self.ast_ident(*ikey);
                                let (pos, name) = (ident.pos, ident.name.clone());
                                let has_name = name != "_";
                                let o = self.tc_objs.new_var(pos, Some(self.pkg), name, None);
                                self.result.record_def(*ikey, Some(o));
                                if has_name {
                                    vars.push(o);
                                }
                                o
                            }
                            _ => {
                                let ed = self.new_dis(lhs.unwrap());
                                self.error(ed.pos(), format!("cannot declare {}", ed));
                                let (pos, name) = (ed.pos(), "_".to_string());
                                self.tc_objs.new_var(pos, Some(self.pkg), name, None)
                            }
                        };
                        // initialize lhs variable
                        if rhs[i].is_some() {
                            x.mode = OperandMode::Value;
                            x.expr = lhs.map(|x| x.clone());
                            x.typ = rhs[i];
                            self.init_var(okey, x, "range clause", fctx);
                        } else {
                            let invalid_type = self.invalid_type();
                            let oval = self.lobj_mut(okey);
                            oval.set_type(Some(invalid_type));
                            oval.entity_type_mut().var_property_mut().used = true;
                        }
                    }

                    // declare variables
                    if vars.len() > 0 {
                        let scope_pos = rs.expr.end(self.ast_objs);
                        for okey in vars.iter() {
                            // spec: "The scope of a constant or variable identifier declared inside
                            // a function begins at the end of the ConstSpec or VarSpec (ShortVarDecl
                            // for short variable declarations) and ends at the end of the innermost
                            // containing block."
                            self.declare(
                                self.octx.scope.unwrap(),
                                None, /* record_def already called */
                                *okey,
                                scope_pos,
                            );
                        }
                    } else {
                        self.error_str(rs.token_pos, "no new variables on left side of :=");
                    }
                } else {
                    // ordinary assignment
                    for (i, lhs) in lhs.iter().enumerate() {
                        if lhs.is_some() && rhs[i].is_some() {
                            x.mode = OperandMode::Value;
                            x.expr = lhs.map(|x| x.clone());
                            x.typ = rhs[i];
                            self.assign_var(lhs.unwrap(), x, fctx);
                        }
                    }
                }

                self.stmt(&Stmt::Block(rs.body.clone()), &inner_ctx, fctx);

                self.close_scope()
            }
            _ => self.error_str(stmt.pos(self.ast_objs), "invalid statement"),
        }

        fctx.process_delayed(begin_delayed_count, self);
        debug_assert_eq!(begin_scope, self.octx.scope);
    }
}
