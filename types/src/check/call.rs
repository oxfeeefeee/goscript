// Copyright 2022 The Goscript Authors. All rights reserved.
// Use of this source code is governed by a BSD-style
// license that can be found in the LICENSE file.
//
//
// This code is adapted from the offical Go code written in Go
// with license as follows:
// Copyright 2013 The Go Authors. All rights reserved.
// Use of this source code is governed by a BSD-style
// license that can be found in the LICENSE file.

#![allow(dead_code)]
use crate::SourceRead;

use super::super::lookup::{self, LookupResult, MethodSet};
use super::super::obj::EntityType;
use super::super::objects::TypeKey;
use super::super::operand::{Operand, OperandMode};
use super::super::selection::{Selection, SelectionKind};
use super::super::typ;
use super::super::universe::ExprKind;
use super::check::{Checker, FilesContext};
use super::util::{UnpackResult, UnpackedResultLeftovers};
use goscript_parser::ast::{CallExpr, Expr, Node, SelectorExpr};
use goscript_parser::Pos;
use std::rc::Rc;

impl<'a, S: SourceRead> Checker<'a, S> {
    pub fn call(
        &mut self,
        x: &mut Operand,
        e: &Rc<CallExpr>,
        fctx: &mut FilesContext<S>,
    ) -> ExprKind {
        self.expr_or_type(x, &e.func, fctx);

        let expr = Some(Expr::Call(e.clone()));
        match x.mode {
            OperandMode::Invalid => {
                self.use_exprs(&e.args, fctx);
                x.expr = expr;
                ExprKind::Statement
            }
            OperandMode::TypeExpr => {
                // conversion
                let t = x.typ.unwrap();
                x.mode = OperandMode::Invalid;
                match e.args.len() {
                    0 => self.error(
                        e.r_paren,
                        format!("missing argument in conversion to {}", self.new_dis(&t)),
                    ),
                    1 => {
                        self.expr(x, &e.args[0], fctx);
                        if !x.invalid() {
                            self.conversion(x, t, fctx);
                        }
                    }
                    _ => {
                        self.use_exprs(&e.args, fctx);
                        self.error(
                            e.args[e.args.len() - 1].pos(self.ast_objs),
                            format!("too many arguments in conversion to {}", self.new_dis(&t)),
                        )
                    }
                }
                x.expr = expr;
                ExprKind::Conversion
            }
            OperandMode::Builtin(id) => {
                if !self.builtin(x, e, id, fctx) {
                    x.mode = OperandMode::Invalid;
                }
                x.expr = expr;
                // a non-constant result implies a function call
                self.octx.has_call_or_recv = match x.mode {
                    OperandMode::Invalid | OperandMode::Constant(_) => false,
                    _ => true,
                };
                self.tc_objs.universe().builtins()[&id].kind
            }
            _ => {
                // function/method call
                let sig_key = typ::underlying_type(x.typ.unwrap(), self.tc_objs);
                if let Some(sig) = self.otype(sig_key).try_as_signature() {
                    let sig_results = sig.results();
                    let variadic = sig.variadic();
                    let pcount = sig.params_count(self.tc_objs);
                    let result = self.unpack(&e.args, pcount, false, variadic, fctx);
                    match result {
                        UnpackResult::Error => x.mode = OperandMode::Invalid,
                        _ => {
                            let (count, _) = result.rhs_count();
                            let re = UnpackedResultLeftovers::new(&result, None);
                            self.arguments(x, e, sig_key, &re, count, fctx);
                        }
                    }

                    // determine result
                    let sigre = self.tc_objs.types[sig_results].try_as_tuple().unwrap();
                    match sigre.vars().len() {
                        0 => x.mode = OperandMode::NoValue,
                        1 => {
                            x.mode = OperandMode::Value;
                            x.typ = self.lobj(sigre.vars()[0]).typ(); // unpack tuple
                        }
                        _ => {
                            x.mode = OperandMode::Value;
                            x.typ = Some(sig_results);
                        }
                    }
                    self.octx.has_call_or_recv = true;
                } else {
                    let dis = self.new_dis(x);
                    self.invalid_op(dis.pos(), &format!("cannot call non-function {}", dis));
                    x.mode = OperandMode::Invalid;
                }
                x.expr = expr;
                ExprKind::Statement
            }
        }
    }

    /// arguments checks argument passing for the call with the given signature.
    pub fn arguments(
        &mut self,
        x: &mut Operand,
        call: &CallExpr,
        sig: TypeKey,
        re: &UnpackedResultLeftovers,
        n: usize,
        fctx: &mut FilesContext<S>,
    ) {
        let sig_val = self.otype(sig).try_as_signature().unwrap();
        let variadic = sig_val.variadic();
        let params = self.otype(sig_val.params()).try_as_tuple().unwrap();
        let params_len = params.vars().len();
        if let Some(ell) = call.ellipsis {
            if !variadic {
                let dis = self.new_dis(&call.func);
                self.error(
                    ell,
                    format!("cannot use ... in call to non-variadic {}", dis),
                );
                re.use_all(self, fctx);
                return;
            }
            if call.args.len() == 1 && n > 1 {
                let dis = self.new_dis(&call.args[0]);
                self.error(ell, format!("cannot use ... with {}-valued {}", n, dis));
                re.use_all(self, fctx);
                return;
            }
        }

        // evaluate arguments
        let note = &format!("argument to {}", self.new_dis(&call.func));
        for i in 0..n {
            re.get(self, x, i, fctx);
            if !x.invalid() {
                let ellipsis = if i == n - 1 { call.ellipsis } else { None };
                self.argument(sig, i, x, ellipsis, note, fctx);
            }
        }

        // check argument count
        // a variadic function accepts an "empty"
        // last argument: count one extra
        let count = if variadic { n + 1 } else { n };
        if count < params_len {
            let dis = self.new_dis(&call.func);
            self.error(
                call.r_paren,
                format!("too few arguments in call to {}", dis),
            );
        }
    }

    /// argument checks passing of argument x to the i'th parameter of the given signature.
    /// If ellipsis is_some(), the argument is followed by ... at that position in the call.
    fn argument(
        &mut self,
        sig: TypeKey,
        i: usize,
        x: &mut Operand,
        ellipsis: Option<Pos>,
        note: &str,
        fctx: &mut FilesContext<S>,
    ) {
        self.single_value(x);
        if x.invalid() {
            return;
        }

        let sig_val = self.otype(sig).try_as_signature().unwrap();
        let params = self.otype(sig_val.params()).try_as_tuple().unwrap();
        let n = params.vars().len();

        let mut ty = if i < n {
            self.lobj(params.vars()[i]).typ().unwrap()
        } else if sig_val.variadic() {
            let t = self.lobj(params.vars()[n - 1]).typ().unwrap();
            if cfg!(debug_assertions) {
                if self.otype(t).try_as_slice().is_none() {
                    let pos = self.lobj(params.vars()[n - 1]).pos();
                    let td = self.new_dis(&t);
                    self.dump(
                        Some(pos),
                        &format!("expected unnamed slice type, got {}", td),
                    );
                }
            }
            t
        } else {
            self.error_str(x.pos(self.ast_objs), "too many arguments");
            return;
        };

        if let Some(pos) = ellipsis {
            // argument is of the form x... and x is single-valued
            if i != n - 1 {
                self.error_str(pos, "can only use ... with matching parameter");
                return;
            }
            let xtype = x.typ.unwrap();
            if self
                .otype(xtype)
                .underlying_val(self.tc_objs)
                .try_as_slice()
                .is_none()
                && xtype != self.basic_type(typ::BasicType::UntypedNil)
            {
                let xd = self.new_dis(x);
                let td = self.new_dis(&ty);
                self.error(
                    xd.pos(),
                    format!("cannot use {} as parameter of type {}", xd, td),
                );
                return;
            }
        } else if sig_val.variadic() && i >= n - 1 {
            ty = self.otype(ty).try_as_slice().unwrap().elem();
        }

        self.assignment(x, Some(ty), note, fctx);
    }

    pub fn selector(&mut self, x: &mut Operand, e: &Rc<SelectorExpr>, fctx: &mut FilesContext<S>) {
        let err_exit = |x: &mut Operand| {
            x.mode = OperandMode::Invalid;
            x.expr = Some(Expr::Selector(e.clone()));
        };

        // If the identifier refers to a package, handle everything here
        // so we don't need a "package" mode for operands: package names
        // can only appear in qualified identifiers which are mapped to
        // selector expressions.
        match &e.expr {
            Expr::Ident(ikey) => {
                let ident = self.ast_ident(*ikey);
                if let Some(okey) = self.lookup(&ident.name) {
                    let lobj = &mut self.tc_objs.lobjs[okey];
                    let lobj_pkg = lobj.pkg();
                    match lobj.entity_type_mut() {
                        EntityType::PkgName(imported, used) => {
                            debug_assert_eq!(self.pkg, lobj_pkg.unwrap());
                            self.result.record_use(*ikey, okey);
                            *used = true;
                            let pkg = &self.tc_objs.pkgs[*imported];
                            let sel_name = &self.ast_objs.idents[e.sel].name;
                            let exp_op = self.tc_objs.scopes[*pkg.scope()].lookup(sel_name);
                            if exp_op.is_none() {
                                if !pkg.fake() {
                                    let pos = self.ast_ident(e.sel).pos;
                                    let msg = format!(
                                        "{} not declared by package {}",
                                        sel_name,
                                        pkg.name().as_ref().unwrap()
                                    );
                                    self.error(pos, msg);
                                }
                                return err_exit(x);
                            }
                            let exp = self.lobj(*exp_op.unwrap());
                            if !exp.exported() {
                                let pos = self.ast_ident(e.sel).pos;
                                let msg = format!(
                                    "{} not declared by package {}",
                                    sel_name,
                                    pkg.name().as_ref().unwrap()
                                );
                                self.error(pos, msg);
                            }
                            self.result.record_use(e.sel, *exp_op.unwrap());

                            // Simplified version of the code for ast::Idents:
                            // - imported objects are always fully initialized
                            let exp = self.lobj(*exp_op.unwrap());
                            x.mode = match exp.entity_type() {
                                EntityType::Const(v) => OperandMode::Constant(v.clone()),
                                EntityType::TypeName => OperandMode::TypeExpr,
                                EntityType::Var(_) => OperandMode::Variable,
                                EntityType::Func(_) => OperandMode::Value,
                                EntityType::Builtin(id) => OperandMode::Builtin(*id),
                                _ => unreachable!(),
                            };
                            x.typ = exp.typ();
                            x.expr = Some(Expr::Selector(e.clone()));
                            return;
                        }
                        _ => {}
                    }
                }
            }
            _ => {}
        }

        self.expr_or_type(x, &e.expr, fctx);
        if x.invalid() {
            return err_exit(x);
        }

        let sel_name = &self.ast_objs.idents[e.sel].name;
        let result = lookup::lookup_field_or_method(
            x.typ.unwrap(),
            x.mode == OperandMode::Variable,
            Some(self.pkg),
            sel_name,
            self.tc_objs,
        );

        let (okey, indices, indirect) = match result {
            LookupResult::Entry(okey, indices, indirect) => (okey, indices, indirect),
            _ => {
                let pos = self.ast_ident(e.sel).pos;
                let msg = match &result {
                    LookupResult::Ambiguous(_) => format!("ambiguous selector {}", sel_name),
                    LookupResult::NotFound => {
                        let ed = self.new_dis(x.expr.as_ref().unwrap());
                        let td = self.new_td_o(&x.typ);
                        format!(
                            "{}.{} undefined (type {} has no field or method {})",
                            ed, sel_name, td, sel_name
                        )
                    }
                    LookupResult::BadMethodReceiver => {
                        let td = self.new_td_o(&x.typ);
                        format!("{} is not in method set of {}", sel_name, td)
                    }
                    LookupResult::Entry(_, _, _) => unreachable!(),
                };
                self.error(pos, msg);
                return err_exit(x);
            }
        };

        // methods may not have a fully set up signature yet
        if self.lobj(okey).entity_type().is_func() {
            self.obj_decl(okey, None, fctx);
        }

        let sel_name = &self.ast_objs.idents[e.sel].name;
        if x.mode == OperandMode::TypeExpr {
            // method expression
            match self.lobj(okey).entity_type() {
                EntityType::Func(_) => {
                    let selection = Selection::new(
                        SelectionKind::MethodExpr,
                        x.typ,
                        okey,
                        indices,
                        indirect,
                        self.tc_objs,
                    );
                    self.result.record_selection(e, selection);

                    // the receiver type becomes the type of the first function
                    // argument of the method expression's function type
                    let var = self
                        .tc_objs
                        .new_var(0, Some(self.pkg), "".to_owned(), x.typ);
                    let lobj = self.lobj(okey);
                    let sig = self.otype(lobj.typ().unwrap()).try_as_signature().unwrap();
                    let (p, r, v) = (sig.params(), sig.results(), sig.variadic());
                    let params_val = self.otype(p).try_as_tuple().unwrap();
                    let mut vars = vec![var];
                    vars.append(&mut params_val.vars().clone());
                    let params = self.tc_objs.new_t_tuple(vars);
                    let new_sig = self.tc_objs.new_t_signature(None, None, params, r, v);
                    x.mode = OperandMode::Value;
                    x.typ = Some(new_sig);

                    self.add_decl_dep(okey);
                }
                _ => {
                    let ed = self.new_dis(x.expr.as_ref().unwrap());
                    let td = self.new_td_o(&x.typ);
                    let msg = format!(
                        "{}.{} undefined (type {} has no method {})",
                        ed, sel_name, td, sel_name
                    );
                    self.error(self.ast_ident(e.sel).pos, msg);
                    return err_exit(x);
                }
            }
        } else {
            // regular selector
            let lobj = &self.tc_objs.lobjs[okey];
            match lobj.entity_type() {
                EntityType::Var(_) => {
                    let selection = Selection::new(
                        SelectionKind::FieldVal,
                        x.typ,
                        okey,
                        indices,
                        indirect,
                        self.tc_objs,
                    );
                    self.result.record_selection(e, selection);
                    x.mode = if x.mode == OperandMode::Variable || indirect {
                        OperandMode::Variable
                    } else {
                        OperandMode::Value
                    };
                    x.typ = lobj.typ();
                }
                EntityType::Func(_) => {
                    let selection = Selection::new(
                        SelectionKind::MethodVal,
                        x.typ,
                        okey,
                        indices,
                        indirect,
                        self.tc_objs,
                    );
                    self.result.record_selection(e, selection);

                    if cfg!(debug_assertions) {
                        // Verify that LookupFieldOrMethod and MethodSet.Lookup agree.
                        let mut typ = x.typ.unwrap();
                        if x.mode == OperandMode::Variable {
                            // If typ is not an (unnamed) pointer or an interface,
                            // use *typ instead, because the method set of *typ
                            // includes the methods of typ.
                            // Variables are addressable, so we can always take their
                            // address.
                            if self.otype(typ).try_as_pointer().is_none()
                                && !typ::is_interface(typ, self.tc_objs)
                            {
                                typ = self.tc_objs.new_t_pointer(typ);
                            }
                        }
                        let mset = MethodSet::new(&typ, self.tc_objs);
                        let re = mset.lookup(&self.pkg, sel_name, self.tc_objs);
                        if re.is_none() || re.unwrap().obj() != okey {
                            let obj_name = self.lobj(okey).name();
                            let expr = Expr::Selector(e.clone());
                            let ed = self.new_dis(&expr);
                            let td = self.new_dis(&typ);
                            let md = self.new_dis(re.unwrap());
                            self.dump(
                                None,
                                &format!("{}: ({}).{} -> {}", ed.pos(), td, obj_name, md),
                            );
                            self.dump(None, &format!("{}\n", self.new_dis(&mset)));
                            panic!("method sets and lookup don't agree");
                        }
                    }

                    x.mode = OperandMode::Value;

                    // remove receiver
                    let lobj = &self.tc_objs.lobjs[okey];
                    let sig = self.otype(lobj.typ().unwrap()).try_as_signature().unwrap();
                    let (p, r, v) = (sig.params(), sig.results(), sig.variadic());
                    let new_sig = self.tc_objs.new_t_signature(None, None, p, r, v);
                    x.typ = Some(new_sig);

                    self.add_decl_dep(okey);
                }
                _ => unreachable!(),
            }
        }
        x.expr = Some(Expr::Selector(e.clone()));
    }
}
