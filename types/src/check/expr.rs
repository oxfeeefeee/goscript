#![allow(dead_code)]
use super::super::constant::Value;
use super::super::objects::{DeclInfoKey, ObjKey, PackageKey, ScopeKey, TCObjects, TypeKey};
use super::super::operand::{Operand, OperandMode};
use super::super::typ::{self, BasicType, Type};
use super::check::{Checker, FilesContext};
use goscript_parser::ast::Node;
use goscript_parser::ast::{self};
use goscript_parser::{Expr, Token};
use std::rc::Rc;

///Basic algorithm:
///
///Expressions are checked recursively, top down. Expression checker functions
///are generally of the form:
///
///  fn f(x &mut operand, e &Expr, ...)
///
///where e is the expression to be checked, and x is the result of the check.
///The check performed by f may fail in which case x.mode == OperandMode::invalid,
///and related error messages will have been issued by f.
///
///If a hint argument is present, it is the composite literal element type
///of an outer composite literal; it is used to type-check composite literal
///elements that have no explicit type specification in the source
///(e.g.: []T{{...}, {...}}, the hint is the type T in this case).
///
///All expressions are checked via raw_expr, which dispatches according
///to expression kind. Upon returning, raw_expr is recording the types and
///constant values for all expressions that have an untyped type (those types
///may change on the way up in the expression tree). Usually these are constants,
///but the results of comparisons or non-constant shifts of untyped constants
///may also be untyped, but not constant.
///
///Untyped expressions may eventually become fully typed (i.e., not untyped),
///typically when the value is assigned to a variable, or is used otherwise.
///The update_expr_type method is used to record this final type and update
///the recorded types: the type-checked expression tree is again traversed down,
///and the new type is propagated as needed. Untyped constant expression values
///that become fully typed must now be representable by the full type (constant
///sub-expression trees are left alone except for their roots). This mechanism
///ensures that a client sees the actual (run-time) type an untyped value would
///have. It also permits type-checking of lhs shift operands "as if the shift
///were not present": when update_expr_type visits an untyped lhs shift operand
///and assigns it it's final type, that type must be an integer type, and a
///constant lhs must be representable as an integer.
///
///When an expression gets its final type, either on the way out from raw_expr,
///on the way down in update_expr_type, or at the end of the type checker run,
///the type (and constant value, if any) is recorded via Info.Types, if present.

impl<'a> Checker<'a> {
    fn op_token(&self, x: &mut Operand, token: &Token, binary: bool) -> bool {
        let pred = |t: &Token, ty: &TypeKey| -> Option<bool> {
            if binary {
                match t {
                    Token::ADD => {
                        Some(typ::is_numeric(ty, self.tc_objs) || typ::is_string(ty, self.tc_objs))
                    }
                    Token::SUB => Some(typ::is_numeric(ty, self.tc_objs)),
                    Token::MUL => Some(typ::is_numeric(ty, self.tc_objs)),
                    Token::QUO => Some(typ::is_numeric(ty, self.tc_objs)),
                    Token::REM => Some(typ::is_integer(ty, self.tc_objs)),
                    Token::AND => Some(typ::is_integer(ty, self.tc_objs)),
                    Token::OR => Some(typ::is_integer(ty, self.tc_objs)),
                    Token::XOR => Some(typ::is_integer(ty, self.tc_objs)),
                    Token::AND_NOT => Some(typ::is_integer(ty, self.tc_objs)),
                    Token::LAND => Some(typ::is_boolean(ty, self.tc_objs)),
                    Token::LOR => Some(typ::is_boolean(ty, self.tc_objs)),
                    _ => None,
                }
            } else {
                match t {
                    Token::ADD => Some(typ::is_numeric(ty, self.tc_objs)),
                    Token::SUB => Some(typ::is_numeric(ty, self.tc_objs)),
                    Token::XOR => Some(typ::is_integer(ty, self.tc_objs)),
                    Token::NOT => Some(typ::is_boolean(ty, self.tc_objs)),
                    _ => None,
                }
            }
        };

        if let Some(ok) = pred(token, x.typ.as_ref().unwrap()) {
            if !ok {
                let xd = self.new_dis(x);
                self.invalid_op(
                    xd.pos(),
                    &format!("operator {} not defined for {}", token, xd),
                );
            }
            ok
        } else {
            self.invalid_ast(x.pos(self.ast_objs), &format!("unknown operator {}", token));
            false
        }
    }

    /// The unary expression e may be None. It's passed in for better error messages only.
    fn unary(&mut self, x: &mut Operand, e: Option<Expr>, op: &Token) {
        match op {
            Token::AND => {
                // spec: "As an exception to the addressability
                // requirement x may also be a composite literal."
                if let Expr::CompositeLit(_) = Checker::unparen(x.expr.as_ref().unwrap()) {
                    if x.mode != OperandMode::Variable {
                        let xd = self.new_dis(x);
                        self.invalid_op(xd.pos(), &format!("cannot take address of {}", xd));
                        return;
                    }
                }
                x.mode = OperandMode::Value;
                x.typ = Some(self.tc_objs.new_t_pointer(x.typ.unwrap()));
            }
            Token::ARROW => {
                if let Some(chan) = self
                    .otype(x.typ.unwrap())
                    .underlying_val(self.tc_objs)
                    .try_as_chan()
                {
                    if *chan.dir() == typ::ChanDir::SendOnly {
                        let xd = self.new_dis(x);
                        self.invalid_op(
                            xd.pos(),
                            &format!("cannot receive from send-only channel {}", xd),
                        );
                        return;
                    }
                    x.mode = OperandMode::CommaOk;
                    x.typ = Some(*chan.elem());
                    self.octx.has_call_or_recv = true;
                    return;
                } else {
                    let xd = self.new_dis(x);
                    self.invalid_op(xd.pos(), &format!("cannot receive from non-channel {}", xd));
                }
            }
            _ => {
                if self.op_token(x, op, false) {
                    x.mode = OperandMode::Invalid;
                    return;
                }
                if let OperandMode::Constant(v) = &mut x.mode {
                    let ty = *typ::underlying_type(&x.typ.unwrap(), self.tc_objs);
                    let tval = self.otype(ty);
                    let prec = if tval.is_unsigned(self.tc_objs) {
                        tval.try_as_basic().unwrap().size_of()
                    } else {
                        0
                    };
                    *v = Value::unary_op(op, v, prec);
                    // Typed constants must be representable in
                    // their type after each constant operation.
                    if tval.is_typed(self.tc_objs) {
                        if e.is_some() {
                            x.expr = e // for better error message
                        }
                        self.representable(x, ty);
                    }
                    return;
                }
                x.mode = OperandMode::Value;
                // x.typ remains unchanged
            }
        }
    }

    fn is_shift(op: &Token) -> bool {
        match op {
            Token::SHL | Token::SHR => true,
            _ => false,
        }
    }

    fn is_comparison(op: &Token) -> bool {
        match op {
            Token::EQL | Token::NEQ | Token::LSS | Token::LEQ | Token::GTR | Token::GEQ => true,
            _ => false,
        }
    }

    /// representable checks that a constant operand is representable in the given basic type.
    pub fn representable(&mut self, x: &mut Operand, t: TypeKey) {
        let tval = self.otype(t);
        let tbasic = tval.try_as_basic().unwrap();
        if let OperandMode::Constant(v) = &mut x.mode {
            let clone = v.clone();
            if clone.representable(tbasic, Some(v)) {
                let o = &self.tc_objs;
                let xtval = self.otype(x.typ.unwrap());
                let tval = self.otype(t);
                let xd = self.new_dis(x);
                let td = self.new_dis(&t);
                // numeric conversion : error msg
                //
                // integer -> integer : overflows
                // integer -> float   : overflows (actually not possible)
                // float   -> integer : truncated
                // float   -> float   : overflows
                let msg = if xtval.is_numeric(o) && tval.is_numeric(o) {
                    if !xtval.is_integer(o) && tval.is_integer(o) {
                        format!("{} truncated to {}", xd, td)
                    } else {
                        format!("{} overflows {}", xd, td)
                    }
                } else {
                    format!("cannot convert {} to {}", xd, td)
                };
                self.error(xd.pos(), msg);
                x.mode = OperandMode::Invalid;
            }
        } else {
            unreachable!()
        }
    }

    /// update_expr_type updates the type of x to typ and invokes itself
    /// recursively for the operands of x, depending on expression kind.
    /// If typ is still an untyped and not the final type, update_expr_type
    /// only updates the recorded untyped type for x and possibly its
    /// operands. Otherwise (i.e., typ is not an untyped type anymore,
    /// or it is the final type for x), the type and value are recorded.
    /// Also, if x is a constant, it must be representable as a value of typ,
    /// and if x is the (formerly untyped) lhs operand of a non-constant
    /// shift, it must be an integer value.
    pub fn update_expr_type(
        &mut self,
        e: &Expr,
        t: TypeKey,
        final_: bool,
        fctx: &mut FilesContext,
    ) {
        let old_opt = fctx.untyped.get(&e.id());
        if old_opt.is_none() {
            return; // nothing to do
        }
        let old = old_opt.unwrap();

        // update operands of x if necessary
        match e {
            Expr::Bad(_)
            | Expr::FuncLit(_)
            | Expr::CompositeLit(_)
            | Expr::Index(_)
            | Expr::Slice(_)
            | Expr::TypeAssert(_)
            | Expr::Star(_)
            | Expr::KeyValue(_)
            | Expr::Array(_)
            | Expr::Struct(_)
            | Expr::Func(_)
            | Expr::Interface(_)
            | Expr::Map(_)
            | Expr::Chan(_) => {
                if self.tc_objs.debug {
                    let ed = self.new_dis(e);
                    let (otd, td) = (self.new_td_o(&old.typ), self.new_dis(&t));
                    self.dump(
                        Some(ed.pos()),
                        &format!("found old type({}): {} (new: {})", ed, otd, td),
                    );
                    unreachable!()
                }
                return;
            }
            Expr::Call(_) => {
                // Resulting in an untyped constant (e.g., built-in complex).
                // The respective calls take care of calling update_expr_type
                // for the arguments if necessary.
            }
            Expr::Ident(_) | Expr::BasicLit(_) | Expr::Selector(_) => {
                // An identifier denoting a constant, a constant literal,
                // or a qualified identifier (imported untyped constant).
                // No operands to take care of.
            }
            Expr::Paren(p) => {
                self.update_expr_type(&p.expr, t, final_, fctx);
            }
            Expr::Unary(u) => {
                // If x is a constant, the operands were constants.
                // The operands don't need to be updated since they
                // never get "materialized" into a typed value. If
                // left in the untyped map, they will be processed
                // at the end of the type check.
                if old.typ.is_none() {
                    self.update_expr_type(&u.expr, t, final_, fctx);
                }
            }
            Expr::Binary(b) => {
                if old.typ.is_none() {
                    if Checker::is_comparison(&b.op) {
                        // The result type is independent of operand types
                        // and the operand types must have final types.
                    } else if Checker::is_shift(&b.op) {
                        // The result type depends only on lhs operand.
                        // The rhs type was updated when checking the shift.
                        self.update_expr_type(&b.expr_a, t, final_, fctx);
                    } else {
                        // The operand types match the result type.
                        self.update_expr_type(&b.expr_a, t, final_, fctx);
                        self.update_expr_type(&b.expr_b, t, final_, fctx);
                    }
                }
            }
            _ => unreachable!(),
        }

        // If the new type is not final and still untyped, just
        // update the recorded type.
        let o = &self.tc_objs;
        if !final_ && typ::is_typed(&t, o) {
            let old = fctx.untyped.get_mut(&e.id()).unwrap();
            old.typ = Some(*typ::underlying_type(&t, o));
            return;
        } else {
            // Otherwise we have the final (typed or untyped type).
            // Remove it from the map of yet untyped expressions.
            fctx.untyped.remove(&e.id());
        }

        let old = fctx.untyped.get(&e.id()).unwrap();
        if old.is_lhs {
            // If x is the lhs of a shift, its final type must be integer.
            // We already know from the shift check that it is representable
            // as an integer if it is a constant.
            if !typ::is_integer(&t, o) {
                let ed = self.new_dis(e);
                let td = self.new_dis(&t);
                self.invalid_op(
                    ed.pos(),
                    &format!("shifted operand {} (type {}) must be integer", ed, td),
                );
                return;
            }
            // Even if we have an integer, if the value is a constant we
            // still must check that it is representable as the specific
            // int type requested
        }
        let mode_clone = old.mode.clone();
        if let OperandMode::Constant(_) = &old.mode {
            // If x is a constant, it must be representable as a value of typ.
            let mut c = Operand::new_with(old.mode.clone(), Some(e.clone()), old.typ);
            self.convert_untyped(&mut c, t, fctx);
            if c.invalid() {
                return;
            }
        }

        // Everything's fine, record final type and value for x.
        self.result.record_type_and_value(e, mode_clone, t);
    }

    /// update_expr_val updates the value of x to val.
    fn update_expr_val(e: &Expr, val: Value, fctx: &mut FilesContext) {
        if let Some(info) = fctx.untyped.get_mut(&e.id()) {
            if let OperandMode::Constant(v) = &mut info.mode {
                *v = val;
            } else {
                unreachable!()
            }
        }
    }

    /// convert_untyped attempts to set the type of an untyped value to the target type.
    pub fn convert_untyped(&mut self, x: &mut Operand, target: TypeKey, fctx: &mut FilesContext) {
        let o = &self.tc_objs;
        if x.invalid() || typ::is_typed(&x.typ.unwrap(), o) || target == self.invalid_type() {
            return;
        }

        let on_err = |c: &mut Checker, x: &mut Operand| {
            let xd = c.new_dis(x);
            let td = c.new_dis(&target);
            c.error(xd.pos(), format!("cannot convert {} to {}", xd, td));
            x.mode = OperandMode::Invalid;
        };

        if typ::is_untyped(&target, o) {
            // both x and target are untyped
            let order = |bt: BasicType| -> usize {
                match bt {
                    BasicType::UntypedInt => 1,
                    BasicType::UntypedRune => 2,
                    BasicType::UntypedFloat => 3,
                    BasicType::UntypedComplex => 4,
                    _ => unreachable!(),
                }
            };
            let xtval = self.otype(x.typ.unwrap());
            let ttval = self.otype(target);
            let xbasic = xtval.try_as_basic().unwrap().typ();
            let tbasic = ttval.try_as_basic().unwrap().typ();
            if xbasic != tbasic {
                if xtval.is_numeric(o) && ttval.is_numeric(o) {
                    if order(xbasic) < order(tbasic) {
                        x.typ = Some(target);
                        self.update_expr_type(x.expr.as_ref().unwrap(), target, false, fctx);
                    }
                } else {
                    return on_err(self, x);
                }
            }
            return;
        }

        let t = *typ::underlying_type(&target, o);
        let xtype = x.typ.unwrap();
        let tval = self.otype(t);
        let final_target = match tval {
            Type::Basic(_) => {
                if let OperandMode::Constant(v) = &x.mode {
                    let v_clone = v.clone();
                    self.representable(x, t);
                    if x.invalid() {
                        return;
                    }
                    // expression value may have been rounded - update if needed
                    Checker::update_expr_val(x.expr.as_ref().unwrap(), v_clone, fctx);
                    Some(target)
                } else {
                    // Non-constant untyped values may appear as the
                    // result of comparisons (untyped bool), intermediate
                    // (delayed-checked) rhs operands of shifts, and as
                    // the value nil.
                    let ok = match self.otype(x.typ.unwrap()).try_as_basic().unwrap().typ() {
                        BasicType::Bool => tval.is_boolean(o),
                        BasicType::UntypedInt
                        | BasicType::UntypedRune
                        | BasicType::UntypedFloat
                        | BasicType::UntypedComplex => tval.is_numeric(o),
                        BasicType::UntypedString => unreachable!(),
                        BasicType::UntypedNil => tval.has_nil(o),
                        _ => false,
                    };
                    if ok {
                        Some(target)
                    } else {
                        None
                    }
                }
            }
            Type::Interface(detail) => {
                // Update operand types to the default type rather then
                // the target (interface) type: values must have concrete
                // dynamic types. If the value is nil, keep it untyped
                if x.is_nil(self.tc_objs.universe()) {
                    Some(self.basic_type(BasicType::UntypedNil))
                } else {
                    if detail.is_empty() {
                        Some(*typ::untyped_default_type(&xtype, o))
                    } else {
                        // cannot assign untyped values to non-empty interfaces
                        None
                    }
                }
            }
            Type::Pointer(_)
            | Type::Signature(_)
            | Type::Slice(_)
            | Type::Map(_)
            | Type::Chan(_) => {
                if x.is_nil(self.tc_objs.universe()) {
                    Some(self.basic_type(BasicType::UntypedNil))
                } else {
                    None
                }
            }
            _ => None,
        };

        if let Some(t) = final_target {
            x.typ = final_target;
            self.update_expr_type(x.expr.as_ref().unwrap(), t, true, fctx);
        } else {
            on_err(self, x);
        }
    }

    fn comparision(&mut self, x: &mut Operand, y: &Operand, op: &Token, fctx: &mut FilesContext) {
        // spec: "In any comparison, the first operand must be assignable
        // to the type of the second operand, or vice versa."
        let o = &self.tc_objs;
        let u = o.universe();
        let (xtype, ytype) = (x.typ.unwrap(), y.typ.unwrap());
        let (xtval, ytval) = (self.otype(xtype), self.otype(ytype));
        let emsg = if x.assignable_to(ytype, None, o) || x.assignable_to(xtype, None, o) {
            let defined = match op {
                Token::EQL | Token::NEQ => {
                    (xtval.comparable(o) && ytval.comparable(o))
                        || (x.is_nil(u) && ytval.has_nil(o))
                        || (y.is_nil(u) && xtval.has_nil(o))
                }
                Token::LSS | Token::LEQ | Token::GTR | Token::GEQ => {
                    xtval.is_ordered(o) && ytval.is_ordered(o)
                }
                _ => unreachable!(),
            };
            if !defined {
                let t = if x.is_nil(u) { ytype } else { xtype };
                let td = self.new_dis(&t);
                Some(format!("operator {} not defined for {}", op, td))
            } else {
                None
            }
        } else {
            let (xd, yd) = (self.new_dis(&xtype), self.new_dis(&ytype));
            Some(format!("mismatched types {} and {}", xd, yd))
        };
        if let Some(m) = emsg {
            let pos = x.pos(self.ast_objs);
            let xd = self.new_dis(x.expr.as_ref().unwrap());
            let yd = self.new_dis(y.expr.as_ref().unwrap());
            self.error(pos, format!("cannot compare {} {} {} ({})", xd, op, yd, m));
            return;
        }

        match (&mut x.mode, &y.mode) {
            (OperandMode::Constant(vx), OperandMode::Constant(vy)) => {
                *vx = Value::with_bool(Value::compare(vx, op, vy));
                // The operands are never materialized; no need to update
                // their types.
            }
            _ => {
                x.mode = OperandMode::Value;
                // The operands have now their final types, which at run-
                // time will be materialized. Update the expression trees.
                // If the current types are untyped, the materialized type
                // is the respective default type.
                self.update_expr_type(
                    x.expr.as_ref().unwrap(),
                    *typ::untyped_default_type(&xtype, self.tc_objs),
                    true,
                    fctx,
                );
                self.update_expr_type(
                    y.expr.as_ref().unwrap(),
                    *typ::untyped_default_type(&ytype, self.tc_objs),
                    true,
                    fctx,
                );
            }
        }
        // spec: "Comparison operators compare two operands and yield
        //        an untyped boolean value."
        x.typ = Some(self.basic_type(BasicType::UntypedBool));
    }

    fn shift(
        &mut self,
        x: &mut Operand,
        y: &mut Operand,
        op: &Token,
        e: Option<Expr>,
        fctx: &mut FilesContext,
    ) {
        let o = &self.tc_objs;
        let xtval = self.otype(x.typ.unwrap());
        let xt_untyped = xtval.is_untyped(o);
        let xt_integer = xtval.is_integer(o);
        let x_const = x.mode.constant_val().map(|x| x.to_int().into_owned());

        // The lhs is of integer type or an untyped constant representable
        // as an integer
        let lhs_ok =
            xt_integer || (xt_untyped && x_const.is_some() && x_const.as_ref().unwrap().is_int());
        if !lhs_ok {
            let xd = self.new_dis(x);
            self.error(xd.pos(), format!("shifted operand {} must be integer", xd));
        }

        // spec: "The right operand in a shift expression must have unsigned
        // integer type or be an untyped constant representable by a value of
        // type uint."
        let ytval = self.otype(y.typ.unwrap());
        if ytval.is_unsigned(o) {
            //ok
        } else if ytval.is_untyped(o) {
            self.convert_untyped(y, self.basic_type(BasicType::Uint), fctx);
            if y.invalid() {
                x.mode = OperandMode::Invalid;
                return;
            }
        } else {
            let yd = self.new_dis(y);
            self.error(
                yd.pos(),
                format!("shift count {} must be unsigned integer", yd),
            );
            x.mode = OperandMode::Invalid;
            return;
        }

        if let OperandMode::Constant(xv) = &mut x.mode {
            if let OperandMode::Constant(yv) = &y.mode {
                // rhs must be an integer value
                let yval = yv.to_int();
                if !yval.is_int() {
                    let yd = self.new_dis(y);
                    self.invalid_op(
                        yd.pos(),
                        &format!("shift count {} must be unsigned integer", yd),
                    );
                    x.mode = OperandMode::Invalid;
                    return;
                }
                // rhs must be within reasonable bounds
                let shift_bound = 1023 - 1 + 52; // so we can express smallestFloat64
                let (s, ok) = yval.int_as_u64();
                if !ok || s > shift_bound {
                    let yd = self.new_dis(y);
                    self.invalid_op(yd.pos(), &format!("invalid shift count {}", yd));
                    x.mode = OperandMode::Invalid;
                    return;
                }
                // The lhs is representable as an integer but may not be an integer
                // (e.g., 2.0, an untyped float) - this can only happen for untyped
                // non-integer numeric constants. Correct the type so that the shift
                // result is of integer type.
                if !xt_integer {
                    x.typ = Some(self.basic_type(BasicType::UntypedInt));
                }
                // x is a constant so xval != nil and it must be of Int kind.
                *xv = Value::shift(xv, op, s as usize);
                // Typed constants must be representable in
                // their type after each constant operation.
                if typ::is_typed(&x.typ.unwrap(), self.tc_objs) {
                    if e.is_some() {
                        x.expr = e
                    }
                    self.representable(x, x.typ.unwrap());
                }
                return;
            }

            if xt_untyped {}
            unimplemented!()
        }
        unimplemented!()
    }

    /// expr typechecks expression e and initializes x with the expression value.
    /// The result must be a single value.
    /// If an error occurred, x.mode is set to invalid.
    pub fn expr(&mut self, x: &mut Operand, e: &Expr) {
        unimplemented!()
    }

    /// raw_expr typechecks expression e and initializes x with the expression
    /// value or type. If an error occurred, x.mode is set to invalid.
    /// If hint is_some(), it is the type of a composite literal element.
    pub fn raw_expr(&mut self, x: &mut Operand, e: &Expr, hint: Option<TypeKey>) {
        unimplemented!()
    }

    /// multi_expr is like expr but the result may be a multi-value.
    pub fn multi_expr(&mut self, x: &mut Operand, e: &Expr) {
        unimplemented!()
    }

    /// expr_or_type typechecks expression or type e and initializes x with the expression
    /// value or type. If an error occurred, x.mode is set to invalid.
    pub fn expr_or_type(&mut self, x: &mut Operand, e: &Expr) {
        unimplemented!()
    }

    pub fn single_value(&mut self, x: &mut Operand) {
        unimplemented!()
    }

    /// index checks an index expression for validity.
    /// If max >= 0, it is the upper bound for index.
    /// If the result >= 0, then it is the constant value of index.
    pub fn index(&mut self, index: &Expr, max: Option<isize>) -> Option<isize> {
        unimplemented!()
    }
}
