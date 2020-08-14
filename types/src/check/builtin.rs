#![allow(dead_code)]
use super::super::constant::Value;
use super::super::lookup::{self, LookupResult};
use super::super::objects::{ObjKey, TCObjects, TypeKey};
use super::super::operand::{Operand, OperandMode};
use super::super::selection::{Selection, SelectionKind};
use super::super::typ::{self, untyped_default_type, BasicInfo, BasicType, Type};
use super::super::universe::Builtin;
use super::check::{Checker, FilesContext};
use super::util::{UnpackResult, UnpackedResultLeftovers};
use goscript_parser::ast::{CallExpr, Expr, Node};
use goscript_parser::Token;
use std::cmp::Ordering;
use std::collections::HashSet;
use std::rc::Rc;

impl<'a> Checker<'a> {
    /// builtin type-checks a call to the built-in specified by id and
    /// reports whether the call is valid, with *x holding the result;
    /// but x.expr is not set. If the call is invalid, the result is
    /// false, and *x is undefined.
    pub fn builtin(
        &mut self,
        x: &mut Operand,
        call: &Rc<CallExpr>,
        id: Builtin,
        fctx: &mut FilesContext,
    ) -> bool {
        // append is the only built-in that permits the use of ... for the last argument
        let binfo = self.tc_objs.universe().builtins()[&id];
        if call.ellipsis.is_some() && id != Builtin::Append {
            self.invalid_op(
                call.ellipsis.unwrap(),
                &format!("invalid use of ... with built-in {}", binfo.name),
            );
            self.use_exprs(&call.args, fctx);
            return false;
        }

        // For len(x) and cap(x) we need to know if x contains any function calls or
        // receive operations. Save/restore current setting and set has_call_or_recv to
        // false for the evaluation of x so that we can check it afterwards.
        // Note: We must do this _before_ calling unpack because unpack evaluates the
        //       first argument before we even call arg(x, 0)!
        let hcor_backup = if id == Builtin::Len || id == Builtin::Cap {
            Some(self.octx.has_call_or_recv)
        } else {
            None
        };
        self.octx.has_call_or_recv = false;

        let report_mismatch = |checker: &Checker, ord, rcount| {
            let msg = match ord {
                std::cmp::Ordering::Less => "not enough",
                std::cmp::Ordering::Greater => "too many",
                std::cmp::Ordering::Equal => return,
            };

            let expr = Expr::Call(call.clone());
            let ed = checker.new_dis(&expr);
            checker.invalid_op(
                call.r_paren,
                &format!(
                    "{} arguments for {} (expected {}, found {})",
                    msg, &ed, binfo.arg_count, rcount
                ),
            );
        };

        // determine actual arguments
        let mut nargs = call.args.len();
        let unpack_result = match id {
            // arguments require special handling
            Builtin::Make | Builtin::New | Builtin::Offsetof | Builtin::Trace => {
                let ord = nargs.cmp(&binfo.arg_count);
                let ord = if binfo.variadic && ord == Ordering::Greater {
                    Ordering::Equal
                } else {
                    ord
                };
                if ord != std::cmp::Ordering::Equal {
                    report_mismatch(self, ord, nargs);
                    return false;
                }
                None
            }
            _ => {
                let result = self.unpack(&call.args, binfo.arg_count, false, binfo.variadic, fctx);
                if result.is_err() {
                    return false;
                }
                let (count, ord) = result.rhs_count();
                nargs = count;
                if ord != std::cmp::Ordering::Equal {
                    report_mismatch(self, ord, count);
                    return false;
                }
                match result {
                    UnpackResult::Tuple(_, _, _)
                    | UnpackResult::Mutliple(_, _)
                    | UnpackResult::Single(_, _) => {
                        result.get(self, x, 0, fctx);
                        if x.invalid() {
                            return false;
                        }
                    }
                    UnpackResult::Nothing(_) => {} // do nothing
                    UnpackResult::CommaOk(_, _) => unreachable!(),
                    UnpackResult::Error => unreachable!(),
                }
                Some(result)
            }
        };

        let invalid_type = self.invalid_type();
        let om_builtin = &OperandMode::Builtin(id);
        let record = |c: &mut Checker, res: Option<TypeKey>, args: &[TypeKey], variadic: bool| {
            let sig = make_sig(c.tc_objs, res, args, variadic);
            c.result.record_builtin_type(om_builtin, &call.func, sig);
        };
        let record_with_sig = |c: &mut Checker, sig: TypeKey| {
            c.result.record_builtin_type(om_builtin, &call.func, sig);
        };
        match id {
            Builtin::Append => {
                // append(s S, x ...T) S, where T is the element type of S
                // spec: "The variadic function append appends zero or more values x to s of type
                // S, which must be a slice type, and returns the resulting slice, also of type S.
                // The values x are passed to a parameter of type ...T where T is the element type
                // of S and the respective parameter passing rules apply."
                let slice = x.typ.unwrap();
                let telem = if let Some(detail) = self
                    .otype(typ::underlying_type(slice, self.tc_objs))
                    .try_as_slice()
                {
                    detail.elem()
                } else {
                    let xd = self.new_dis(x);
                    self.invalid_arg(xd.pos(), &format!("{} is not a slice", xd));
                    return false;
                };

                // the first arg is already evaluated
                let mut alist = vec![x.clone()];

                // spec: "As a special case, append also accepts a first argument assignable
                // to type []byte with a second argument of string type followed by ... .
                // This form appends the bytes of the string.
                if nargs == 2
                    && call.ellipsis.is_some()
                    && x.assignable_to(
                        *self.tc_objs.universe().slice_of_bytes(),
                        None,
                        self.tc_objs,
                    )
                {
                    unpack_result.as_ref().unwrap().get(self, x, 1, fctx);
                    if x.invalid() {
                        return false;
                    }
                    let stype = x.typ.unwrap();
                    if typ::is_string(stype, self.tc_objs) {
                        record(self, Some(slice), &vec![slice, stype], true);

                        x.mode = OperandMode::Value;
                        x.typ = Some(slice);
                        return true;
                    }
                    alist.push(x.clone());
                }

                // check general case by creating custom signature
                let tslice = self.tc_objs.new_t_slice(telem);
                let sig = make_sig(self.tc_objs, Some(slice), &vec![slice, tslice], true);
                let re = UnpackedResultLeftovers {
                    leftovers: unpack_result.as_ref().unwrap(),
                    consumed: Some(&alist),
                };
                self.arguments(x, call, sig, &re, nargs, fctx);
                // ok to continue even if check.arguments reported errors

                x.mode = OperandMode::Value;
                x.typ = Some(slice);
                record_with_sig(self, sig);
            }
            Builtin::Cap | Builtin::Len => {
                // cap(x)
                // len(x)
                let ty = typ::underlying_type(x.typ.unwrap(), self.tc_objs);
                let ty = implicit_array_deref(ty, self.tc_objs);
                let mode = match self.otype(ty) {
                    Type::Basic(detail) => {
                        if detail.info() == BasicInfo::IsString {
                            if let OperandMode::Constant(v) = &x.mode {
                                OperandMode::Constant(Value::with_u64(
                                    v.str_as_string().len() as u64
                                ))
                            } else {
                                OperandMode::Value
                            }
                        } else {
                            OperandMode::Invalid
                        }
                    }
                    Type::Array(detail) => {
                        if self.octx.has_call_or_recv {
                            OperandMode::Value
                        } else {
                            // spec: "The expressions len(s) and cap(s) are constants
                            // if the type of s is an array or pointer to an array and
                            // the expression s does not contain channel receives or
                            // function calls; in this case s is not evaluated."
                            OperandMode::Constant(if let Some(len) = detail.len() {
                                Value::with_u64(len)
                            } else {
                                Value::Unknown
                            })
                        }
                    }
                    Type::Slice(_) | Type::Chan(_) => OperandMode::Value,
                    Type::Map(_) => {
                        if id == Builtin::Len {
                            OperandMode::Value
                        } else {
                            OperandMode::Invalid
                        }
                    }
                    _ => OperandMode::Invalid,
                };

                self.octx.has_call_or_recv = hcor_backup.unwrap();

                if mode == OperandMode::Invalid && ty != invalid_type {
                    let dis = self.new_dis(x);
                    self.invalid_arg(dis.pos(), &format!("{} for {}", dis, binfo.name));
                    return false;
                }

                x.mode = mode;
                x.typ = Some(self.basic_type(BasicType::Int));
                match &x.mode {
                    OperandMode::Constant(_) => {}
                    _ => record(self, x.typ, &vec![ty], false),
                }
            }
            Builtin::Close => {
                // close(c)
                let tkey = typ::underlying_type(x.typ.unwrap(), self.tc_objs);
                if let Some(detail) = self.otype(tkey).try_as_chan() {
                    if detail.dir() == typ::ChanDir::RecvOnly {
                        let dis = self.new_dis(x);
                        self.invalid_arg(
                            dis.pos(),
                            &format!("{} must not be a receive-only channel", dis),
                        );
                        return false;
                    }
                    x.mode = OperandMode::NoValue;

                    record(self, None, &vec![tkey], false);
                } else {
                    let dis = self.new_dis(x);
                    self.invalid_arg(dis.pos(), &format!("{} is not a channel", dis));
                    return false;
                }
            }
            Builtin::Complex => {
                // complex(x, y floatT) complexT
                let mut y = Operand::new();
                unpack_result.as_ref().unwrap().get(self, &mut y, 1, fctx);
                if y.invalid() {
                    return false;
                }

                // convert or check untyped arguments
                let x_untyped = typ::is_untyped(x.typ.unwrap(), self.tc_objs);
                let y_untyped = typ::is_untyped(y.typ.unwrap(), self.tc_objs);
                match (x_untyped, y_untyped) {
                    (false, false) => {} // x and y are typed => nothing to do
                    // only x is untyped => convert to type of y
                    (true, false) => self.convert_untyped(x, y.typ.unwrap(), fctx),
                    // only y is untyped => convert to type of x
                    (false, true) => self.convert_untyped(&mut y, x.typ.unwrap(), fctx),
                    (true, true) => {
                        // x and y are untyped =>
                        // 1) if both are constants, convert them to untyped
                        //    floating-point numbers if possible,
                        // 2) if one of them is not constant (possible because
                        //    it contains a shift that is yet untyped), convert
                        //    both of them to float64 since they must have the
                        //    same type to succeed (this will result in an error
                        //    because shifts of floats are not permitted)
                        match (&x.mode, &y.mode) {
                            (OperandMode::Constant(vx), OperandMode::Constant(vy)) => {
                                let to_float =
                                    |xtype: &mut Option<TypeKey>, v: &Value, objs: &TCObjects| {
                                        if typ::is_numeric(xtype.unwrap(), objs)
                                            && v.imag().sign() == 0
                                        {
                                            *xtype = Some(self.basic_type(BasicType::UntypedFloat));
                                        }
                                    };
                                to_float(&mut x.typ, vx, self.tc_objs);
                                to_float(&mut y.typ, vy, self.tc_objs);
                            }
                            _ => {
                                let tf64 = self.basic_type(BasicType::Float64);
                                self.convert_untyped(x, tf64, fctx);
                                self.convert_untyped(&mut y, tf64, fctx);
                                // x and y should be invalid now, but be conservative
                                // and check below
                            }
                        }
                    }
                }
                if x.invalid() || y.invalid() {
                    return false;
                }

                // both argument types must be identical
                if !typ::identical_option(x.typ, y.typ, self.tc_objs) {
                    self.invalid_arg(
                        x.pos(self.ast_objs),
                        &format!(
                            "mismatched types {} and {}",
                            self.new_dis(x.typ.as_ref().unwrap()),
                            self.new_dis(y.typ.as_ref().unwrap())
                        ),
                    );
                    return false;
                }

                // the argument types must be of floating-point type
                if !typ::is_float(x.typ.unwrap(), self.tc_objs) {
                    self.invalid_arg(
                        x.pos(self.ast_objs),
                        &format!(
                            "arguments have type {}, expected floating-point",
                            self.new_dis(x.typ.as_ref().unwrap())
                        ),
                    );
                    return false;
                }

                // if both arguments are constants, the result is a constant
                match (&mut x.mode, &y.mode) {
                    (OperandMode::Constant(vx), OperandMode::Constant(vy)) => {
                        *vx = Value::binary_op(vx, &Token::ADD, &vy.to_float().make_imag());
                    }
                    _ => {
                        x.mode = OperandMode::Value;
                    }
                }

                // determine result type
                let res = match self
                    .otype(x.typ.unwrap())
                    .underlying_val(self.tc_objs)
                    .try_as_basic()
                    .unwrap()
                    .typ()
                {
                    BasicType::Float32 => BasicType::Complex64,
                    BasicType::Float64 => BasicType::Complex128,
                    BasicType::UntypedFloat => BasicType::UntypedComplex,
                    _ => unreachable!(),
                };
                let res_type = self.basic_type(res);

                match &x.mode {
                    OperandMode::Constant(_) => {}
                    _ => record(
                        self,
                        Some(res_type),
                        &vec![x.typ.unwrap(), x.typ.unwrap()],
                        false,
                    ),
                }

                x.typ = Some(res_type);
            }
            Builtin::Copy => {
                // copy(x, y []T) int
                let dst = self
                    .otype(x.typ.unwrap())
                    .underlying_val(self.tc_objs)
                    .try_as_slice()
                    .map(|x| x.elem());

                let mut y = Operand::new();
                unpack_result.as_ref().unwrap().get(self, &mut y, 1, fctx);
                if y.invalid() {
                    return false;
                }
                let ytype = self.otype(y.typ.unwrap());
                let src = match ytype.underlying_val(self.tc_objs) {
                    Type::Basic(detail) => {
                        if detail.info() == BasicInfo::IsString {
                            Some(*self.tc_objs.universe().byte())
                        } else {
                            None
                        }
                    }
                    Type::Slice(detail) => Some(detail.elem()),
                    _ => None,
                };

                if dst.is_none() || src.is_none() {
                    let (xd, yd) = (self.new_dis(x), self.new_dis(&y));
                    self.invalid_arg(
                        xd.pos(),
                        &format!("copy expects slice arguments; found {} and {}", xd, yd),
                    );
                    return false;
                }

                if !typ::identical_option(dst, src, self.tc_objs) {
                    let (xd, yd) = (self.new_dis(x), self.new_dis(&y));
                    let (txd, tyd) = (self.new_td_o(&dst), self.new_td_o(&src));
                    self.invalid_arg(
                        xd.pos(),
                        &format!(
                            "arguments to copy {} and {} have different element types {} and {}",
                            xd, yd, txd, tyd
                        ),
                    );
                    return false;
                }

                record(
                    self,
                    Some(self.basic_type(BasicType::Int)),
                    &vec![x.typ.unwrap(), y.typ.unwrap()],
                    false,
                );

                x.mode = OperandMode::Value;
                x.typ = Some(self.basic_type(BasicType::Int));
            }
            Builtin::Delete => {
                // delete(m, k)
                let mtype = x.typ.unwrap();
                match self.otype(mtype).underlying_val(self.tc_objs).try_as_map() {
                    Some(detail) => {
                        let key = detail.key();
                        unpack_result.as_ref().unwrap().get(self, x, 1, fctx);
                        if x.invalid() {
                            return false;
                        }
                        if !x.assignable_to(key, None, self.tc_objs) {
                            let xd = self.new_dis(x);
                            let td = self.new_dis(&key);
                            self.invalid_arg(
                                xd.pos(),
                                &format!("{} is not assignable to {}", xd, td),
                            );
                            return false;
                        }
                        x.mode = OperandMode::NoValue;
                        record(self, None, &vec![mtype, key], false);
                    }
                    None => {
                        let xd = self.new_dis(x);
                        self.invalid_arg(xd.pos(), &format!("{} is not a map", xd));
                        return false;
                    }
                }
            }
            Builtin::Imag | Builtin::Real => {
                // imag(complexT) floatT
                // real(complexT) floatT

                // convert or check untyped argument
                if typ::is_untyped(x.typ.unwrap(), self.tc_objs) {
                    if let OperandMode::Constant(_) = &x.mode {
                        // an untyped constant number can alway be considered
                        // as a complex constant
                        if typ::is_numeric(x.typ.unwrap(), self.tc_objs) {
                            x.typ = Some(self.basic_type(BasicType::UntypedComplex));
                        }
                    } else {
                        // an untyped non-constant argument may appear if
                        // it contains a (yet untyped non-constant) shift
                        // expression: convert it to complex128 which will
                        // result in an error (shift of complex value)
                        self.convert_untyped(x, self.basic_type(BasicType::Complex128), fctx);
                        // x should be invalid now, but be conservative and check
                        if x.invalid() {
                            return false;
                        }
                    }
                }

                // the argument must be of complex type
                if !typ::is_complex(x.typ.unwrap(), self.tc_objs) {
                    let xd = self.new_dis(x);
                    self.invalid_arg(
                        xd.pos(),
                        &format!("argument has type {}, expected complex type", xd),
                    );
                    return false;
                }

                // if the argument is a constant, the result is a constant
                if let OperandMode::Constant(v) = &mut x.mode {
                    *v = match id {
                        Builtin::Real => v.real(),
                        Builtin::Imag => v.imag(),
                        _ => unreachable!(),
                    };
                } else {
                    x.mode = OperandMode::Value;
                }

                // determine result type
                let res = match self
                    .otype(x.typ.unwrap())
                    .underlying_val(self.tc_objs)
                    .try_as_basic()
                    .unwrap()
                    .typ()
                {
                    BasicType::Complex64 => BasicType::Float32,
                    BasicType::Complex128 => BasicType::Float64,
                    BasicType::UntypedComplex => BasicType::UntypedFloat,
                    _ => unreachable!(),
                };
                let res_type = self.basic_type(res);

                match &x.mode {
                    OperandMode::Constant(_) => {}
                    _ => record(self, Some(res_type), &vec![x.typ.unwrap()], false),
                }

                x.typ = Some(res_type);
            }
            Builtin::Make => {
                // make(T, n)
                // make(T, n, m)
                // (no argument evaluated yet)
                let arg0 = &call.args[0];
                let arg0t = self.type_expr(arg0, fctx);
                if arg0t == invalid_type {
                    return false;
                }

                let min = match self.otype(arg0t).underlying_val(self.tc_objs) {
                    Type::Slice(_) => 2,
                    Type::Map(_) | Type::Chan(_) => 1,
                    _ => {
                        let ed = self.new_dis(arg0);
                        self.invalid_arg(
                            ed.pos(),
                            &format!("cannot make {}; type must be slice, map, or channel", ed),
                        );
                        return false;
                    }
                };
                if nargs < min || min + 1 < nargs {
                    let expr = Expr::Call(call.clone());
                    let ed = self.new_dis(&expr);
                    self.error(
                        ed.pos(),
                        format!(
                            "{} expects {} or {} arguments; found {}",
                            ed,
                            min,
                            min + 1,
                            nargs
                        ),
                    );
                    return false;
                }

                // constant integer arguments, if any
                let sizes: Vec<u64> = call.args[1..]
                    .iter()
                    .filter_map(|x| {
                        if let Ok(i) = self.index(x, None, fctx) {
                            return i;
                        }
                        None
                    })
                    .collect();
                if sizes.len() == 2 && sizes[0] > sizes[1] {
                    let pos = call.args[1].pos(self.ast_objs);
                    self.invalid_arg(pos, "length and capacity swapped");
                    // safe to continue
                }
                x.mode = OperandMode::Value;
                x.typ = Some(arg0t);

                let int_type = self.basic_type(BasicType::Int);
                record(
                    self,
                    x.typ,
                    &[arg0t, int_type, int_type][..1 + sizes.len()],
                    false,
                );
            }
            Builtin::New => {
                // new(T)
                // (no argument evaluated yet)
                let arg0 = &call.args[0];
                let argt = self.type_expr(arg0, fctx);
                if argt == invalid_type {
                    return false;
                }

                x.mode = OperandMode::Value;
                x.typ = Some(self.tc_objs.new_t_pointer(argt));
                record(self, x.typ, &vec![argt], false);
            }
            Builtin::Panic => {
                // panic(x)
                // record panic call if inside a function with result parameters
                // (for use in Checker.isTerminating)
                if let Some(sig) = self.octx.sig {
                    if self
                        .otype(sig)
                        .try_as_signature()
                        .unwrap()
                        .results_count(self.tc_objs)
                        > 0
                    {
                        if self.octx.panics.is_none() {
                            self.octx.panics = Some(HashSet::new());
                        }
                        self.octx.panics.as_mut().unwrap().insert(call.id());
                    }
                }

                let iempty = self.tc_objs.new_t_empty_interface();
                self.assignment(x, Some(iempty), "argument to panic", fctx);
                if x.invalid() {
                    return false;
                }

                x.mode = OperandMode::NoValue;
                record(self, None, &vec![iempty], false);
            }
            Builtin::Print | Builtin::Println => {
                // print(x, y, ...)
                // println(x, y, ...)
                let mut params = vec![];
                for i in 0..nargs {
                    if i > 0 {
                        // first argument already evaluated
                        unpack_result.as_ref().unwrap().get(self, x, i, fctx);
                    }
                    let msg = format!("argument to {}", self.builtin_info(id).name);
                    self.assignment(x, None, &msg, fctx);
                    if x.invalid() {
                        return false;
                    }
                    params.push(x.typ.unwrap());
                }

                x.mode = OperandMode::NoValue;
                // note: not variadic
                record(self, None, &params, false);
            }
            Builtin::Recover => {
                // recover() interface{}
                x.mode = OperandMode::Value;
                x.typ = Some(self.tc_objs.new_t_empty_interface());
                record(self, x.typ, &vec![], false);
            }
            Builtin::Alignof => {
                // unsafe.Alignof(x T) uintptr
                self.assignment(x, None, "argument to unsafe.Alignof", fctx);
                if x.invalid() {
                    return false;
                }
                // todo: not sure if Alignof will ever be used in goscript
                let align = Value::with_i64(0); // set Alignof to zero
                x.mode = OperandMode::Constant(align);
                x.typ = Some(self.basic_type(BasicType::Uintptr));
            }
            Builtin::Offsetof => {
                // unsafe.Offsetof(x T) uintptr, where x must be a selector
                // (no argument evaluated yet)
                let arg0 = &call.args[0];
                if let Expr::Selector(selx) = Checker::unparen(arg0) {
                    self.expr(x, &selx.expr, fctx);
                    if x.invalid() {
                        return false;
                    }
                    let base = lookup::deref_struct_ptr(x.typ.unwrap(), self.tc_objs);
                    let sel = &self.ast_ident(selx.sel).name;
                    let result = lookup::lookup_field_or_method(
                        base,
                        false,
                        Some(self.pkg),
                        sel,
                        self.tc_objs,
                    );
                    let (obj, indices) = match result {
                        LookupResult::Ambiguous(_)
                        | LookupResult::NotFound
                        | LookupResult::BadMethodReceiver => {
                            let td = self.new_dis(&base);
                            let msg = if result == LookupResult::BadMethodReceiver {
                                format!("field {} is embedded via a pointer in {}", sel, td)
                            } else {
                                format!("{} has no single field {}", td, sel)
                            };
                            self.invalid_arg(x.pos(self.ast_objs), &msg);
                            return false;
                        }
                        LookupResult::Entry(okey, indices, _) => {
                            if self.lobj(okey).entity_type().is_func() {
                                let ed = self.new_dis(arg0);
                                self.invalid_arg(ed.pos(), &format!("{} is a method value", ed));
                            }
                            (okey, indices)
                        }
                    };

                    let selection = Selection::new(
                        SelectionKind::FieldVal,
                        Some(base),
                        obj,
                        indices,
                        false,
                        self.tc_objs,
                    );
                    self.result.record_selection(selx, selection);

                    // todo: not sure if Offsetof will ever be used in goscript
                    let offs = Value::with_i64(0); // set Offsetof to zero
                    x.mode = OperandMode::Constant(offs);
                    x.typ = Some(self.basic_type(BasicType::Uintptr));
                } else {
                    let ed = self.new_dis(arg0);
                    self.invalid_arg(ed.pos(), &format!("{} is not a selector expression", ed));
                    self.use_exprs(&vec![arg0.clone()], fctx);
                    return false;
                }
                // result is constant - no need to record signature
            }
            Builtin::Sizeof => {
                // unsafe.Sizeof(x T) uintptr
                self.assignment(x, None, "argument to unsafe.Sizeof", fctx);
                if x.invalid() {
                    return false;
                }
                let size = Value::with_u64(typ::size_of(&x.typ.unwrap(), self.tc_objs) as u64);
                x.mode = OperandMode::Constant(size);
                x.typ = Some(self.basic_type(BasicType::Uintptr));
                // result is constant - no need to record signature
            }
            Builtin::Assert => {
                // assert(pred) causes a typechecker error if pred is false.
                // The result of assert is the value of pred if there is no error.
                // todo: make it work in runtime
                let default_err = || {
                    let xd = self.new_dis(x);
                    self.invalid_arg(xd.pos(), &format!("{} is not a boolean constant", xd));
                    false
                };
                match &x.mode {
                    OperandMode::Constant(v) => {
                        if !typ::is_boolean(x.typ.unwrap(), self.tc_objs) {
                            return default_err();
                        }
                        match v {
                            Value::Bool(b) => {
                                if !*b {
                                    let expr = Expr::Call(call.clone());
                                    let ed = self.new_dis(&expr);
                                    self.error(ed.pos(), format!("{} failed", ed))
                                    // compile-time assertion failure - safe to continue
                                }
                            }
                            _ => {
                                let xd = self.new_dis(x);
                                let msg = format!(
                                    "internal error: value of {} should be a boolean constant",
                                    xd
                                );
                                self.error(xd.pos(), msg);
                                return false;
                            }
                        }
                    }
                    _ => {
                        return default_err();
                    }
                }
                // result is constant - no need to record signature
            }
            Builtin::Trace => {
                // trace(x, y, z, ...) dumps the positions, expressions, and
                // values of its arguments. The result of trace is the value
                // of the first argument.
                // Note: trace is only available in self-test mode.
                // (no argument evaluated yet)
                if nargs == 0 {
                    let expr = Expr::Call(call.clone());
                    let ed = self.new_dis(&expr);
                    self.dump(Some(ed.pos()), "trace() without arguments");
                    x.mode = OperandMode::NoValue;
                    return true;
                }
                let mut x_temp = Operand::new(); // only used for dumping
                let mut cur_x = x;
                for arg in call.args.iter() {
                    self.raw_expr(cur_x, arg, None, fctx); // permit trace for types, e.g.: new(trace(T))
                    let xd = self.new_dis(cur_x);
                    self.dump(Some(xd.pos()), &format!("{}", xd));
                    cur_x = &mut x_temp;
                }
                // x contains info of the first argument
                // trace is only available in test mode - no need to record signature
            }
        }
        true
    }
}

/// make_sig makes a signature for the given argument and result types.
/// Default types are used for untyped arguments, and res may be nil.
fn make_sig(
    objs: &mut TCObjects,
    res: Option<TypeKey>,
    args: &[TypeKey],
    variadic: bool,
) -> TypeKey {
    let list: Vec<ObjKey> = args
        .iter()
        .map(|&x| {
            let ty = Some(untyped_default_type(x, objs));
            objs.new_var(0, None, "".to_string(), ty)
        })
        .collect();
    let params = objs.new_t_tuple(list);
    let rlist = res.map_or(vec![], |x| {
        vec![objs.new_var(0, None, "".to_string(), Some(x))]
    });
    let results = objs.new_t_tuple(rlist);
    objs.new_t_signature(None, None, params, results, variadic)
}

/// implicit_array_deref returns A if typ is of the form *A and A is an array;
/// otherwise it returns typ.
fn implicit_array_deref(t: TypeKey, objs: &TCObjects) -> TypeKey {
    let ty = &objs.types[t];
    if let Some(detail) = ty.try_as_pointer() {
        let base = typ::underlying_type(detail.base(), objs);
        if objs.types[base].try_as_array().is_some() {
            return base;
        }
    }
    t
}
