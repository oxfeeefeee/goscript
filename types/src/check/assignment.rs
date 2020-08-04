#![allow(dead_code)]
use super::super::obj::EntityType;
use super::super::objects::{ObjKey, TypeKey};
use super::super::operand::{Operand, OperandMode};
use super::super::typ;
use super::check::{Checker, FilesContext};
use super::util::UnpackResult;
use goscript_parser::ast::Expr;
use goscript_parser::ast::Node;
use goscript_parser::position::Pos;

impl<'a> Checker<'a> {
    /// assignment reports whether x can be assigned to a variable of type t,
    /// if necessary by attempting to convert untyped values to the appropriate
    /// type. context describes the context in which the assignment takes place.
    /// Use t == None to indicate assignment to an untyped blank identifier.
    /// x.mode is set to invalid if the assignment failed.
    pub fn assignment(
        &mut self,
        x: &mut Operand,
        t: Option<TypeKey>,
        note: &str,
        fctx: &mut FilesContext,
    ) {
        self.single_value(x);
        if x.invalid() {
            return;
        }

        match x.mode {
            OperandMode::Constant(_)
            | OperandMode::Variable
            | OperandMode::MapIndex
            | OperandMode::Value
            | OperandMode::CommaOk => {}
            _ => unreachable!(),
        }

        let xt = x.typ.unwrap();
        if typ::is_untyped(xt, self.tc_objs) {
            if t.is_none() && xt == self.basic_type(typ::BasicType::UntypedNil) {
                self.error(
                    x.pos(self.ast_objs),
                    format!("use of untyped nil in {}", note),
                );
                x.mode = OperandMode::Invalid;
                return;
            }
            // spec: "If an untyped constant is assigned to a variable of interface
            // type or the blank identifier, the constant is first converted to type
            // bool, rune, int, float64, complex128 or string respectively, depending
            // on whether the value is a boolean, rune, integer, floating-point, complex,
            // or string constant."
            let target = if t.is_none() || typ::is_interface(t.unwrap(), self.tc_objs) {
                typ::untyped_default_type(xt, self.tc_objs)
            } else {
                t.unwrap()
            };
            self.convert_untyped(x, target, fctx);
            if x.invalid() {
                return;
            }
        }
        // x.typ is typed

        // spec: "If a left-hand side is the blank identifier, any typed or
        // non-constant value except for the predeclared identifier nil may
        // be assigned to it."
        if t.is_none() {
            return;
        }

        let mut reason = String::new();
        if !x.assignable_to(t.unwrap(), Some(&mut reason), self.tc_objs) {
            let xd = self.new_dis(x);
            let td = self.new_dis(t.as_ref().unwrap());
            if reason.is_empty() {
                self.error(
                    xd.pos(),
                    format!("cannot use {} as {} value in {}", xd, td, note),
                );
            } else {
                self.error(
                    xd.pos(),
                    format!("cannot use {} as {} value in {}: {}", xd, td, note, reason),
                );
            }
            x.mode = OperandMode::Invalid;
        }
    }

    pub fn init_const(&mut self, lhskey: ObjKey, x: &mut Operand, fctx: &mut FilesContext) {
        let invalid_type = self.invalid_type();
        let lhs = self.lobj_mut(lhskey);
        if x.invalid() || x.typ == Some(invalid_type) {
            lhs.set_type(Some(invalid_type));
        }
        if lhs.typ() == Some(invalid_type) {
            return;
        }
        // rhs must be a constant
        if let OperandMode::Constant(_) = &x.mode {
            debug_assert!(typ::is_const_type(x.typ.unwrap(), self.tc_objs));
            // If the lhs doesn't have a type yet, use the type of x.
            let lhs = self.lobj_mut(lhskey);
            if lhs.typ().is_none() {
                lhs.set_type(x.typ);
            }
            let t = lhs.typ().clone();
            self.assignment(x, t, "constant declaration", fctx);
            if x.mode != OperandMode::Invalid {
                self.lobj_mut(lhskey)
                    .set_const_val(x.mode.constant_val().unwrap().clone());
            }
        } else {
            let dis = self.new_dis(x);
            self.error(dis.pos(), format!("{} is not constant", dis));
        }
    }

    pub fn init_var(
        &mut self,
        lhskey: ObjKey,
        x: &mut Operand,
        msg: &str,
        fctx: &mut FilesContext,
    ) -> Option<TypeKey> {
        let invalid_type = self.invalid_type();
        let lhs = self.lobj_mut(lhskey);
        if x.invalid() || x.typ == Some(invalid_type) {
            lhs.set_type(Some(invalid_type));
        }
        if lhs.typ() == Some(invalid_type) {
            return None;
        }
        // If the lhs doesn't have a type yet, use the type of x.
        if lhs.typ().is_none() {
            let xt = x.typ.unwrap();
            let lhs_type = if typ::is_untyped(xt, self.tc_objs) {
                // convert untyped types to default types
                if xt == invalid_type {
                    self.error(
                        x.pos(self.ast_objs),
                        format!("use of untyped nil in {}", msg),
                    );
                    invalid_type
                } else {
                    typ::untyped_default_type(xt, self.tc_objs)
                }
            } else {
                xt
            };

            self.lobj_mut(lhskey).set_type(Some(lhs_type));
            if lhs_type == invalid_type {
                return None;
            }
        }
        let t = self.lobj(lhskey).typ().clone();
        self.assignment(x, t, msg, fctx);
        if x.mode != OperandMode::Invalid {
            x.typ
        } else {
            None
        }
    }

    pub fn assign_var(
        &mut self,
        lhs: &Expr,
        x: &mut Operand,
        fctx: &mut FilesContext,
    ) -> Option<TypeKey> {
        let invalid_type = self.invalid_type();
        if x.invalid() || x.typ == Some(invalid_type) {
            return None;
        }

        let mut v: Option<ObjKey> = None;
        let mut v_used = false;
        // determine if the lhs is a (possibly parenthesized) identifier.
        if let Expr::Ident(ikey) = Checker::unparen(lhs) {
            let name = &self.ast_ident(*ikey).name;
            if name == "_" {
                self.result.record_def(*ikey, None);
                self.assignment(x, None, "assignment to _ identifier", fctx);
                return if x.mode != OperandMode::Invalid {
                    x.typ
                } else {
                    None
                };
            } else {
                // If the lhs is an identifier denoting a variable v, this assignment
                // is not a 'use' of v. Remember current value of v.used and restore
                // after evaluating the lhs via check.expr.
                if let Some(okey) = self.octx.lookup(name, self.tc_objs) {
                    // It's ok to mark non-local variables, but ignore variables
                    // from other packages to avoid potential race conditions with
                    // dot-imported variables.
                    if let EntityType::Var(prop) = self.lobj(*okey).entity_type() {
                        v = Some(*okey);
                        v_used = prop.used;
                    }
                }
            }
        }

        let mut z = Operand::new();
        self.expr(&mut z, lhs, fctx);
        if let Some(okey) = v {
            self.lobj_mut(okey)
                .entity_type_mut()
                .var_property_mut()
                .used = v_used; // restore v.used
        }

        if z.mode == OperandMode::Invalid || z.typ == Some(invalid_type) {
            return None;
        }

        // spec: "Each left-hand side operand must be addressable, a map index
        // expression, or the blank identifier. Operands may be parenthesized."
        match z.mode {
            OperandMode::Invalid => unreachable!(),
            OperandMode::Variable | OperandMode::MapIndex => {}
            _ => {
                if let Some(expr) = &z.expr {
                    if let Expr::Selector(sexpr) = expr {
                        let mut op = Operand::new();
                        self.expr(&mut op, &sexpr.expr, fctx);
                        if op.mode == OperandMode::MapIndex {
                            let ed = self.new_dis(expr);
                            self.error(
                                ed.pos(),
                                format!("cannot assign to struct field {} in map", ed),
                            );
                            return None;
                        }
                    }
                }
                let dis = self.new_dis(&z);
                self.error(dis.pos(), format!("cannot assign to {}", dis));
                return None;
            }
        }

        self.assignment(x, z.typ, "assignment", fctx);
        if x.mode != OperandMode::Invalid {
            x.typ
        } else {
            None
        }
    }

    /// If return_pos is_some, init_vars is called to type-check the assignment of
    /// return expressions, and return_pos is the position of the return statement.
    pub fn init_vars(
        &mut self,
        lhs: &Vec<ObjKey>,
        rhs: &Vec<Expr>,
        return_pos: Option<Pos>,
        fctx: &mut FilesContext,
    ) {
        let invalid_type = self.invalid_type();
        let ll = lhs.len();
        // requires return_pos.is_none for this:
        // func() (int, bool) {
        //    var m map[int]int
        //    return /* ERROR "wrong number of return values" */ m[0]
        // }
        let result = self.unpack(rhs, ll, ll == 2 && return_pos.is_none(), fctx);

        let mut invalidate_lhs = || {
            for okey in lhs.iter() {
                self.lobj_mut(*okey).set_type(Some(invalid_type));
            }
        };
        match result {
            UnpackResult::Error => invalidate_lhs(),
            UnpackResult::Mismatch(exprs, rl) => {
                invalidate_lhs();
                result.use_(self, 0, fctx);
                if let Some(p) = return_pos {
                    self.error(
                        p,
                        format!("wrong number of return values (want {}, got {})", ll, rl),
                    )
                } else {
                    self.error(
                        exprs[0].pos(self.ast_objs),
                        format!("cannot initialize {} variables with {} values", ll, rl),
                    )
                }
            }
            UnpackResult::Tuple(_, _)
            | UnpackResult::CommaOk(_, _)
            | UnpackResult::Mutliple(_)
            | UnpackResult::Single(_)
            | UnpackResult::Nothing => {
                let context = if return_pos.is_some() {
                    "return statement"
                } else {
                    "assignment"
                };
                for (i, l) in lhs.iter().enumerate() {
                    let mut x = Operand::new();
                    result.get(self, &mut x, i, fctx);
                    self.init_var(*l, &mut x, context, fctx);
                }
            }
        }
        if let UnpackResult::CommaOk(e, types) = result {
            self.result.record_comma_ok_types(
                e.as_ref().unwrap(),
                &types,
                self.tc_objs,
                self.ast_objs,
                self.pkg,
            );
        }
    }

    pub fn assign_vars(&mut self, lhs: &Vec<Expr>, rhs: &Vec<Expr>, fctx: &mut FilesContext) {
        let ll = lhs.len();
        let result = self.unpack(rhs, ll, ll == 2, fctx);
        match result {
            UnpackResult::Error => self.use_lhs(lhs, fctx),
            UnpackResult::Mismatch(rhs, rhs_count) => {
                result.use_(self, 0, fctx);
                self.error(
                    rhs[0].pos(self.ast_objs),
                    format!("cannot assign {} values to {} variables", rhs_count, ll),
                );
            }
            UnpackResult::Tuple(_, _)
            | UnpackResult::CommaOk(_, _)
            | UnpackResult::Mutliple(_)
            | UnpackResult::Single(_)
            | UnpackResult::Nothing => {
                for (i, l) in lhs.iter().enumerate() {
                    let mut x = Operand::new();
                    result.get(self, &mut x, i, fctx);
                    self.assign_var(l, &mut x, fctx);
                }
            }
        }
        if let UnpackResult::CommaOk(e, types) = result {
            self.result.record_comma_ok_types(
                e.as_ref().unwrap(),
                &types,
                self.tc_objs,
                self.ast_objs,
                self.pkg,
            );
        }
    }

    pub fn short_var_decl(
        &mut self,
        lhs: &Vec<Expr>,
        rhs: &Vec<Expr>,
        pos: Pos,
        fctx: &mut FilesContext,
    ) {
        let top = fctx.delayed_count();
        let scope_key = self.octx.scope.unwrap();
        let mut new_vars = Vec::new();
        let lhs_vars = lhs
            .iter()
            .map(|x| {
                if let Expr::Ident(ikey) = x {
                    // Use the correct obj if the ident is redeclared. The
                    // variable's scope starts after the declaration; so we
                    // must use Scope.lookup here and call Scope.Insert
                    // (via Check.declare) later.
                    let ident = self.ast_ident(*ikey);
                    if let Some(okey) = self.tc_objs.scopes[scope_key].lookup(&ident.name) {
                        self.result.record_use(*ikey, *okey);
                        if self.lobj(*okey).entity_type().is_var() {
                            *okey
                        } else {
                            let pos = x.pos(self.ast_objs);
                            self.error(pos, format!("cannot assign to {}", self.new_dis(x)));
                            // dummy variable
                            self.tc_objs
                                .new_var(pos, Some(self.pkg), "_".to_string(), None)
                        }
                    } else {
                        // declare new variable, possibly a blank (_) variable
                        let (pos, pkg, name) = (ident.pos, Some(self.pkg), ident.name.clone());
                        let okey = self.tc_objs.new_var(pos, pkg, name.clone(), None);
                        if name != "_" {
                            new_vars.push(okey);
                        }
                        okey
                    }
                } else {
                    self.use_lhs(&vec![x.clone()], fctx);
                    let pos = x.pos(self.ast_objs);
                    self.error(pos, format!("cannot declare {}", self.new_dis(x)));
                    // dummy variable
                    self.tc_objs
                        .new_var(pos, Some(self.pkg), "_".to_string(), None)
                }
            })
            .collect();

        self.init_vars(&lhs_vars, rhs, None, fctx);

        // process function literals in rhs expressions before scope changes
        fctx.process_delayed(top, self);

        // declare new variables
        if new_vars.len() > 0 {
            // spec: "The scope of a constant or variable identifier declared inside
            // a function begins at the end of the ConstSpec or VarSpec (ShortVarDecl
            // for short variable declarations) and ends at the end of the innermost
            // containing block."
            let scope_pos = rhs[rhs.len() - 1].end(self.ast_objs);
            for okey in new_vars.iter() {
                self.declare(scope_key, None, *okey, scope_pos);
            }
        } else {
            self.soft_error(pos, "no new variables on left side of :=".to_string());
        }
    }
}
