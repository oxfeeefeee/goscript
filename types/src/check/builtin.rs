#![allow(dead_code)]
use super::super::constant::Value;
use super::super::display::{ExprCallDisplay, OperandDisplay, TypeDisplay};
use super::super::lookup;
use super::super::obj::EntityType;
use super::super::objects::{DeclInfoKey, ObjKey, PackageKey, ScopeKey, TCObjects, TypeKey};
use super::super::operand::{Operand, OperandMode};
use super::super::scope::Scope;
use super::super::typ::{
    self, untyped_default_type, BasicInfo, BasicType, SignatureDetail, TupleDetail, Type,
};
use super::super::universe::Builtin;
use super::check::{Checker, FilesContext};
use super::util::{UnpackResult, UnpackedResultLeftovers};
use goscript_parser::ast::{CallExpr, Expr, FieldList, Node};
use goscript_parser::objects::{FuncTypeKey, IdentKey};
use goscript_parser::Token;

impl<'a> Checker<'a> {
    /// builtin type-checks a call to the built-in specified by id and
    /// reports whether the call is valid, with *x holding the result;
    /// but x.expr is not set. If the call is invalid, the result is
    /// false, and *x is undefined.
    pub fn builtin(
        &mut self,
        x: &mut Operand,
        call: &CallExpr,
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
            self.use_exprs(&call.args);
            return false;
        }

        // For len(x) and cap(x) we need to know if x contains any function calls or
        // receive operations. Save/restore current setting and set has_call_or_recv to
        // false for the evaluation of x so that we can check it afterwards.
        // Note: We must do this _before_ calling unpack because unpack evaluates the
        //       first argument before we even call arg(x, 0)!
        if id == Builtin::Len || id == Builtin::Cap {
            let hcor = self.octx.has_call_or_recv;
            let f = move |checker: &mut Checker, _: &mut FilesContext| {
                checker.octx.has_call_or_recv = hcor;
            };
            fctx.later(Box::new(f));
            self.octx.has_call_or_recv = false;
        }

        // determine actual arguments
        let nargs = call.args.len();
        let unpack_result = match id {
            // arguments require special handling
            Builtin::Make | Builtin::New | Builtin::Offsetof | Builtin::Trace => None,
            _ => {
                let result = self.unpack(&call.args, binfo.arg_count, false);
                match result {
                    UnpackResult::Tuple(_, _)
                    | UnpackResult::Mutliple(_)
                    | UnpackResult::Single(_) => {
                        result.get(self, x, 0);
                        if x.invalid() {
                            return false;
                        }
                    }
                    UnpackResult::Nothing => {} // do nothing
                    UnpackResult::Mismatch(_) => {
                        let msg = if nargs < binfo.arg_count {
                            Some("not enough")
                        } else if nargs > binfo.arg_count && !binfo.variadic {
                            Some("too many")
                        } else {
                            None
                        };
                        if let Some(m) = msg {
                            let ed = ExprCallDisplay::new(call, self.ast_objs);
                            self.invalid_op(
                                call.r_paren,
                                &format!(
                                    "{} arguments for {} (expected {}, found {})",
                                    m, &ed, binfo.arg_count, nargs
                                ),
                            );
                        }
                    }
                    UnpackResult::CommaOk(_, _) => unreachable!(),
                    UnpackResult::Error => return false,
                }
                Some(result)
            }
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
                    .otype(*typ::underlying_type(&slice, self.tc_objs))
                    .try_as_slice()
                {
                    *detail.elem()
                } else {
                    let xd = self.new_xd(&x);
                    self.invalid_arg(x.pos(self.ast_objs), &format!("{} is not a slice", xd));
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
                        &Some(*self.tc_objs.universe().slice_of_bytes()),
                        None,
                        self.tc_objs,
                    )
                {
                    unpack_result.as_ref().unwrap().get(self, x, 1);
                    if x.invalid() {
                        return false;
                    }
                    let stype = x.typ.unwrap();
                    if typ::is_string(&stype, self.tc_objs) {
                        let sig = make_sig(self.tc_objs, Some(slice), &vec![slice, stype], true);
                        self.result
                            .record_builtin_type(&OperandMode::Builtin(id), &call.func, sig);
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
                    consumed: &alist,
                };
                self.arguments(x, call, sig, &re, nargs);
                // ok to continue even if check.arguments reported errors

                x.mode = OperandMode::Value;
                x.typ = Some(slice);
                self.result
                    .record_builtin_type(&OperandMode::Builtin(id), &call.func, sig);
            }
            Builtin::Cap | Builtin::Len => {
                let ty = typ::underlying_type(x.typ.as_ref().unwrap(), self.tc_objs);
                let ty = implicit_array_deref(*ty, self.tc_objs);
                let mode = match self.otype(ty) {
                    Type::Basic(detail) => {
                        if detail.info() == BasicInfo::IsString {
                            if let OperandMode::Constant(v) = &x.mode {
                                OperandMode::Constant(Value::with_u64(v.as_string().len() as u64))
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
                                Value::with_u64(*len)
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

                if mode == OperandMode::Invalid && ty != self.invalid_type() {
                    self.invalid_arg(
                        x.pos(self.ast_objs),
                        &format!("{} for {}", self.new_xd(x), binfo.name),
                    );
                    return false;
                }

                x.mode = mode;
                x.typ = Some(self.basic_type(BasicType::Int));
                match &x.mode {
                    OperandMode::Constant(_) => {}
                    _ => {
                        let sig = make_sig(self.tc_objs, x.typ, &vec![ty], false);
                        self.result
                            .record_builtin_type(&OperandMode::Builtin(id), &call.func, sig);
                    }
                }
            }
            Builtin::Close => {
                let tkey = *typ::underlying_type(x.typ.as_ref().unwrap(), self.tc_objs);
                if let Some(detail) = self.otype(tkey).try_as_chan() {
                    if *detail.dir() == typ::ChanDir::RecvOnly {
                        self.invalid_arg(
                            x.pos(self.ast_objs),
                            &format!("{} must not be a receive-only channel", self.new_xd(x)),
                        );
                        return false;
                    }
                    x.mode = OperandMode::Value;

                    let sig = make_sig(self.tc_objs, None, &vec![tkey], false);
                    self.result
                        .record_builtin_type(&OperandMode::Builtin(id), &call.func, sig);
                } else {
                    self.invalid_arg(
                        x.pos(self.ast_objs),
                        &format!("{} is not a channel", self.new_xd(x)),
                    );
                    return false;
                }
            }
            Builtin::Complex => {
                let mut y = Operand::new();
                unpack_result.as_ref().unwrap().get(self, &mut y, 1);
                if y.invalid() {
                    return false;
                }

                // convert or check untyped arguments
                let x_untyped = typ::is_untyped(x.typ.as_ref().unwrap(), self.tc_objs);
                let y_untyped = typ::is_untyped(y.typ.as_ref().unwrap(), self.tc_objs);
                match (x_untyped, y_untyped) {
                    (false, false) => {} // x and y are typed => nothing to do
                    // only x is untyped => convert to type of y
                    (true, false) => self.convert_untyped(x, y.typ.unwrap()),
                    // only y is untyped => convert to type of x
                    (false, true) => self.convert_untyped(&mut y, x.typ.unwrap()),
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
                                        if typ::is_numeric(xtype.as_ref().unwrap(), self.tc_objs)
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
                                self.convert_untyped(x, tf64);
                                self.convert_untyped(&mut y, tf64);
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
                if !typ::identical_option(&x.typ, &y.typ, self.tc_objs) {
                    self.invalid_arg(
                        x.pos(self.ast_objs),
                        &format!(
                            "mismatched types {} and {}",
                            self.new_td(x.typ.as_ref().unwrap()),
                            self.new_td(y.typ.as_ref().unwrap())
                        ),
                    );
                    return false;
                }

                // the argument types must be of floating-point type
                if !typ::is_float(x.typ.as_ref().unwrap(), self.tc_objs) {
                    self.invalid_arg(
                        x.pos(self.ast_objs),
                        &format!(
                            "arguments have type {}, expected floating-point",
                            self.new_td(x.typ.as_ref().unwrap())
                        ),
                    );
                    return false;
                }

                // if both arguments are constants, the result is a constant
                match (&mut x.mode, &y.mode) {
                    (OperandMode::Constant(vx), OperandMode::Constant(vy)) => {
                        *vx = Value::binary_op(vx, Token::ADD, vy);
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
                    _ => {
                        let sig = make_sig(
                            self.tc_objs,
                            Some(res_type),
                            &vec![x.typ.unwrap(), x.typ.unwrap()],
                            false,
                        );
                        self.result
                            .record_builtin_type(&OperandMode::Builtin(id), &call.func, sig);
                    }
                }

                x.typ = Some(res_type);
            }
            _ => unimplemented!(),
        }

        unimplemented!()
    }
}

/// make_sig makes a signature for the given argument and result types.
/// Default types are used for untyped arguments, and res may be nil.
fn make_sig(
    objs: &mut TCObjects,
    res: Option<TypeKey>,
    args: &Vec<TypeKey>,
    variadic: bool,
) -> TypeKey {
    let list: Vec<ObjKey> = args
        .iter()
        .map(|x| {
            let ty = Some(*untyped_default_type(x, objs));
            objs.new_var(0, None, "".to_string(), ty)
        })
        .collect();
    let params = objs.new_t_tuple(list);
    let rlist = res.map_or(vec![], |x| {
        vec![objs.new_var(0, None, "".to_string(), Some(x))]
    });
    let results = objs.new_t_tuple(rlist);
    objs.new_t_signature(None, params, results, variadic)
}

/// implicit_array_deref returns A if typ is of the form *A and A is an array;
/// otherwise it returns typ.
fn implicit_array_deref(t: TypeKey, objs: &TCObjects) -> TypeKey {
    let ty = &objs.types[t];
    if let Some(detail) = ty.try_as_pointer() {
        if let Some(under) = objs.types[*detail.base()].underlying() {
            if objs.types[*under].try_as_array().is_some() {
                return *under;
            }
        }
    }
    t
}
