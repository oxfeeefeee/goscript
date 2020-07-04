#![allow(dead_code)]
use super::super::display::{ExprCallDisplay, OperandDisplay, TypeDisplay};
use super::super::lookup;
use super::super::obj::EntityType;
use super::super::objects::{DeclInfoKey, ObjKey, PackageKey, ScopeKey, TCObjects, TypeKey};
use super::super::operand::{Operand, OperandMode};
use super::super::scope::Scope;
use super::super::typ::{untyped_default_type, SignatureDetail, TupleDetail, Type};
use super::super::universe::Builtin;
use super::check::{Checker, FilesContext};
use super::util::UnpackResult;
use goscript_parser::ast::{CallExpr, Expr, FieldList, Node};
use goscript_parser::objects::{FuncTypeKey, IdentKey};

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
        match id {
            // arguments require special handling
            Builtin::Make | Builtin::New | Builtin::Offsetof | Builtin::Trace => {}
            _ => {
                let result = self.unpack(&call.args, binfo.arg_count, false);
                match result {
                    UnpackResult::Tuple(_, _)
                    | UnpackResult::Mutliple(_)
                    | UnpackResult::Single(_) => {
                        result.get(self, x, 0);
                        if x.mode == OperandMode::Invalid {
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
            }
        }

        match id {
            _ => unimplemented!(),
        }

        unimplemented!()
    }
}

/// make_sig makes a signature for the given argument and result types.
/// Default types are used for untyped arguments, and res may be nil.
fn make_sig(objs: &mut TCObjects, res: Option<TypeKey>, args: &Vec<TypeKey>) -> TypeKey {
    let list: Vec<ObjKey> = args
        .iter()
        .map(|x| {
            let ty = Some(*untyped_default_type(x, objs));
            objs.new_var(0, None, "".to_string(), ty)
        })
        .collect();
    let params = objs.types.insert(Type::Tuple(TupleDetail::new(list)));
    let rlist = res.map_or(vec![], |x| {
        vec![objs.new_var(0, None, "".to_string(), Some(x))]
    });
    let results = objs.types.insert(Type::Tuple(TupleDetail::new(rlist)));
    objs.types.insert(Type::Signature(SignatureDetail::new(
        None, params, results, false, objs,
    )))
}

/// implicit_array_deref returns A if typ is of the form *A and A is an array;
/// otherwise it returns typ.
fn implicit_array_deref(objs: &TCObjects, t: TypeKey) -> TypeKey {
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
