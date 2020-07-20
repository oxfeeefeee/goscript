#![allow(dead_code)]
use super::super::constant;
use super::super::obj::LangObj;
use super::super::objects::{DeclInfoKey, ObjKey, PackageKey, ScopeKey, TCObjects, TypeKey};
use super::super::operand::{Operand, OperandMode};
use super::super::typ;
use super::super::universe::ExprKind;
use super::check::{Checker, ExprInfo, FilesContext, ObjContext};
use constant::Value;
use goscript_parser::ast::{BlockStmt, Expr, Node, Stmt};
use goscript_parser::objects::{FuncDeclKey, Objects as AstObjects};
use goscript_parser::{Pos, Token};
use ordered_float;
use std::collections::HashMap;
use std::rc::Rc;

type F64 = ordered_float::OrderedFloat<f64>;

#[derive(Clone, Copy)]
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
        self.stmt_list(&block2, &sctx, fctx);

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

    fn simple_stmt(&mut self, s: Option<&Stmt>, fctx: &mut FilesContext) {
        if let Some(s) = s {
            let sctx = StmtContext::new();
            self.stmt(s, &sctx, fctx);
        }
    }

    fn stmt_list(&mut self, block: &Rc<BlockStmt>, sctx: &StmtContext, fctx: &mut FilesContext) {
        // trailing empty statements are "invisible" to fallthrough analysis
        let index = block
            .list
            .iter()
            .enumerate()
            .rev()
            .find_map(|(i, x)| match x {
                Stmt::Empty(_) => Some(i),
                _ => None,
            })
            .unwrap_or(0);
        for (i, s) in block.list[0..index].iter().enumerate() {
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
        values: &Vec<Expr>,
        seen: &mut ValueMap,
        fctx: &mut FilesContext,
    ) {
        for e in values.iter() {
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
        types: &Vec<Expr>,
        seen: &mut HashMap<Option<TypeKey>, Pos>,
        fctx: &mut FilesContext,
    ) -> Option<TypeKey> {
        types
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
                seen.insert(t, e.pos(self.ast_objs));
                if let Some(t) = t {
                    self.type_assertion(x, xtype, t);
                }
                Some(t)
            })
            .last()
            .flatten()
    }

    fn stmt(&mut self, stmt: &Stmt, sctx: &StmtContext, fctx: &mut FilesContext) {
        unimplemented!()
    }
}
