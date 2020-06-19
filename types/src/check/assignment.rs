#![allow(dead_code)]
use super::super::display::{ExprDisplay, OperandDisplay};
use super::super::obj::{EntityType, LangObj};
use super::super::objects::{ObjKey, TypeKey};
use super::super::operand::{Operand, OperandMode};
use super::super::typ;
use super::check::Checker;
use goscript_parser::position::Pos;
use goscript_parser::{ast::Expr, Parser};

impl<'a> Checker<'a> {
    pub fn assignment(&mut self, x: &mut Operand, tkey: Option<TypeKey>, note: &str) {
        unimplemented!()
    }

    pub fn init_const(&mut self, lhskey: ObjKey, x: &mut Operand) {
        let invalid_type = self.invalid_type();
        let lhs = self.lobj_mut(lhskey);
        if x.mode == OperandMode::Invalid || x.typ == Some(invalid_type) {
            lhs.set_type(Some(invalid_type));
        }
        if lhs.typ() == &Some(invalid_type) {
            return;
        }
        // rhs must be a constant
        if let OperandMode::Constant(_) = &x.mode {
            debug_assert!(typ::is_const_type(x.typ.as_ref().unwrap(), self.tc_objs));
            // If the lhs doesn't have a type yet, use the type of x.
            let lhs = self.lobj_mut(lhskey);
            if lhs.typ().is_none() {
                lhs.set_type(x.typ);
            }
            let t = lhs.typ().clone();
            self.assignment(x, t, "constant declaration");
            if x.mode != OperandMode::Invalid {
                self.lobj_mut(lhskey)
                    .set_const_val(x.mode.constant_val().clone());
            }
        } else {
            let dis = OperandDisplay::new(x, self.ast_objs, self.tc_objs);
            self.error(x.pos(self.ast_objs), format!("{} is not constant", dis));
        }
    }

    pub fn init_var(&mut self, lhskey: ObjKey, x: &mut Operand, msg: &str) -> Option<TypeKey> {
        let invalid_type = self.invalid_type();
        let lhs = self.lobj_mut(lhskey);
        if x.mode == OperandMode::Invalid || x.typ == Some(invalid_type) {
            lhs.set_type(Some(invalid_type));
        }
        if lhs.typ() == &Some(invalid_type) {
            return None;
        }
        // If the lhs doesn't have a type yet, use the type of x.
        if lhs.typ().is_none() {
            let xt = x.typ.unwrap();
            let lhs_type = if typ::is_untyped(&xt, self.tc_objs) {
                // convert untyped types to default types
                if xt == invalid_type {
                    self.error(
                        x.pos(self.ast_objs),
                        format!("use of untyped nil in {}", msg),
                    );
                    invalid_type
                } else {
                    *typ::untyped_default_type(&xt, self.tc_objs)
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
        self.assignment(x, t, msg);
        if x.mode != OperandMode::Invalid {
            x.typ
        } else {
            None
        }
    }

    pub fn assign_var(&mut self, lhs: &Expr, x: &mut Operand) -> Option<TypeKey> {
        let invalid_type = self.invalid_type();
        if x.mode == OperandMode::Invalid || x.typ == Some(invalid_type) {
            return None;
        }

        let mut v: Option<ObjKey> = None;
        let mut v_used = false;
        // determine if the lhs is a (possibly parenthesized) identifier.
        if let Expr::Ident(ikey) = Parser::unparen(lhs) {
            let name = &self.ident(*ikey).name;
            if name == "_" {
                self.result.record_def(*ikey, None);
                self.assignment(x, None, "assignment to _ identifier");
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
        self.expr(&mut z, lhs);
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
                        self.expr(&mut op, &sexpr.expr);
                        if op.mode == OperandMode::MapIndex {
                            self.error(
                                z.pos(self.ast_objs),
                                format!(
                                    "cannot assign to struct field {} in map",
                                    ExprDisplay::new(expr, self.ast_objs)
                                ),
                            );
                            return None;
                        }
                    }
                }
                let dis = OperandDisplay::new(&z, self.ast_objs, self.tc_objs);
                self.error(z.pos(self.ast_objs), format!("cannot assign to {}", dis));
                return None;
            }
        }

        self.assignment(x, z.typ, "assignment");
        if x.mode != OperandMode::Invalid {
            x.typ
        } else {
            None
        }
    }

    /// If return_pos is_some, init_vars is called to type-check the assignment of
    /// return expressions, and return_pos is the position of the return statement.
    pub fn init_vars(&mut self, lhs: &Vec<ObjKey>, rhs: &Vec<Expr>, return_pos: Option<Pos>) {
        unimplemented!()
    }
}
