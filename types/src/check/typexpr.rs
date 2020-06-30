#![allow(dead_code)]
use super::super::constant;
use super::super::display::{ExprDisplay, OperandDisplay, TypeDisplay};
use super::super::lookup;
use super::super::obj::EntityType;
use super::super::objects::{DeclInfoKey, ObjKey, PackageKey, ScopeKey, TCObjects, TypeKey};
use super::super::operand::{Operand, OperandMode};
use super::super::scope::Scope;
use super::super::typ;
use super::check::{Checker, FilesContext};
use goscript_parser::ast::{self, Expr, FieldList, Node};
use goscript_parser::objects::{FuncTypeKey, IdentKey};
use std::borrow::Borrow;

impl<'a> Checker<'a> {
    /// ident type-checks identifier ikey and initializes x with the value or type of ikey.
    /// If an error occurred, x.mode is set to invalid.
    /// For the meaning of def, see Checker.defined_type, below.
    /// If want_type is set, the identifier e is expected to denote a type.
    pub fn ident(
        &mut self,
        x: &mut Operand,
        ikey: IdentKey,
        def: Option<TypeKey>,
        want_type: bool,
        fctx: &mut FilesContext,
    ) {
        x.mode = OperandMode::Invalid;
        x.expr = Some(Expr::Ident(ikey));

        // Note that we cannot use check.lookup here because the returned scope
        // may be different from obj.parent(). See also Scope.lookup_parent doc.
        let name = &self.ast_ident(ikey).name;
        if let Some((skey, okey)) = Scope::lookup_parent(
            &self.octx.scope.unwrap(),
            name,
            Some(self.octx.pos),
            self.tc_objs,
        ) {
            self.result.record_use(ikey, okey);

            // Type-check the object.
            // Only call Checker.obj_decl if the object doesn't have a type yet
            // (in which case we must actually determine it) or the object is a
            // TypeName and we also want a type (in which case we might detect
            // a cycle which needs to be reported). Otherwise we can skip the
            // call and avoid a possible cycle error in favor of the more
            // informative "not a type/value" error that this function's caller
            // will issue
            let lobj = self.lobj(okey);
            let pkg = *lobj.pkg();
            let mut otype = *lobj.typ();
            if otype.is_none() || (lobj.entity_type().is_type_name() && want_type) {
                self.obj_decl(okey, def, fctx);
                // type must have been assigned by Checker.obj_decl
                otype = *self.lobj(okey).typ();
            }
            debug_assert!(otype.is_some());

            // The object may be dot-imported: If so, remove its package from
            // the map of unused dot imports for the respective file scope.
            // (This code is only needed for dot-imports. Without them,
            // we only have to mark variables, see Var case below).
            if pkg.is_some() && pkg != Some(self.pkg) {
                fctx.unused_dot_imports
                    .get_mut(&skey)
                    .unwrap()
                    .remove(&pkg.unwrap());
            }

            let lobj = self.lobj(okey);
            let invalid_type = self.invalid_type();
            match lobj.entity_type() {
                EntityType::PkgName(_, _) => {
                    let pos = self.ast_ident(ikey).pos;
                    let msg = format!("use of package {} not in selector", lobj.name());
                    self.error(pos, msg);
                    return;
                }
                EntityType::Const(_) => {
                    self.add_decl_dep(okey);
                    if otype == Some(invalid_type) {
                        return;
                    }
                    if okey == *self.tc_objs.universe().iota() {
                        if self.octx.iota.is_none() {
                            let pos = self.ast_ident(ikey).pos;
                            let msg = "cannot use iota outside constant declaration".to_string();
                            self.error(pos, msg);
                            return;
                        }
                        x.mode = OperandMode::Constant(self.octx.iota.clone().unwrap());
                    } else {
                        x.mode = OperandMode::Constant(self.lobj(okey).const_val().clone());
                    }
                }
                EntityType::TypeName => x.mode = OperandMode::TypeExpr,
                EntityType::Var(_) => {
                    // It's ok to mark non-local variables, but ignore variables
                    // from other packages to avoid potential race conditions with
                    // dot-imported variables.
                    if *lobj.pkg() == Some(self.pkg) {
                        self.lobj_mut(okey)
                            .entity_type_mut()
                            .var_property_mut()
                            .used = true;
                    }
                    self.add_decl_dep(okey);
                    if otype == Some(invalid_type) {
                        return;
                    }
                    x.mode = OperandMode::Variable;
                }
                EntityType::Func(_) => {
                    self.add_decl_dep(okey);
                    x.mode = OperandMode::Value;
                }
                EntityType::Label(_) => unreachable!(),
                EntityType::Builtin(id) => x.mode = OperandMode::Builtin(*id),
                EntityType::Nil => x.mode = OperandMode::Value,
            }
            x.typ = otype;
        } else {
            let pos = self.ast_ident(ikey).pos;
            if name == "_" {
                self.error(pos, "cannot use _ as value or type".to_string());
            } else {
                self.error(pos, format!("undeclared name: {}", name));
            }
        }
    }

    /// type_expr type-checks the type expression e and returns its type, or Invalid Type.
    pub fn type_expr(&mut self, e: &Expr, fctx: &mut FilesContext) -> TypeKey {
        self.defined_type(e, None, fctx)
    }

    /// defined_type is like type_expr but also accepts a type name def.
    /// If def is_some(), e is the type specification for the defined type def, declared
    /// in a type declaration, and def.underlying will be set to the type of e before
    /// any components of e are type-checked.
    pub fn defined_type(
        &mut self,
        e: &Expr,
        def: Option<TypeKey>,
        fctx: &mut FilesContext,
    ) -> TypeKey {
        if self.config().trace_checker {
            let pos = e.pos(self.ast_objs);
            let ed = ExprDisplay::new(e, self.ast_objs);
            self.trace_begin(pos, &format!("{}", ed));
        }

        let t = self.type_internal(e, def, fctx);
        debug_assert!(typ::is_typed(&t, self.tc_objs));
        self.result
            .record_type_and_value(e, OperandMode::TypeExpr, t);

        if self.config().trace_checker {
            let pos = e.pos(self.ast_objs);
            let td = TypeDisplay::new(&t, self.tc_objs);
            self.trace_begin(pos, &format!("=> {}", td));
        }
        t
    }

    /// indirect_type is like type_expr but it also breaks the (otherwise) infinite size of
    /// recursivetypes by introducing an indirection. It should be called for components of
    /// types thatare not laid out in place in memory, such as pointer base types, slice or
    /// map element types, function parameter types, etc.
    pub fn indirect_type(&mut self, e: &Expr, fctx: &mut FilesContext) -> TypeKey {
        fctx.push(*self.tc_objs.universe().indir());
        let t = self.defined_type(e, None, fctx);
        fctx.pop();
        t
    }

    /// func_type type-checks a function or method type.
    pub fn func_type(&mut self, recv: Option<&FieldList>, ftype: FuncTypeKey) -> TypeKey {
        let skey = self
            .tc_objs
            .new_scope(self.octx.scope, 0, 0, "function".to_string(), true);
        self.result.record_scope(&ftype, skey);

        let (recv_list, _) = self.collect_params(skey, recv, false);
        let ftype_val = &self.ast_objs.ftypes[ftype];
        let (p, r) = (ftype_val.params.clone(), ftype_val.results.clone());
        let (params, variadic) = self.collect_params(skey, Some(&p), true);
        let (results, _) = self.collect_params(skey, r.as_ref(), false);

        let mut recv_okey = None;
        if recv.is_some() {
            // recv parameter list present (may be empty)
            // spec: "The receiver is specified via an extra parameter section preceding the
            // method name. That parameter section must declare a single parameter, the receiver."
            let invalid_type = self.invalid_type();
            let recv_var = match recv_list.len() {
                x if x == 0 => {
                    let pos = recv.unwrap().pos(self.ast_objs);
                    self.error(pos, "method is missing receiver".to_string());
                    self.tc_objs
                        .new_param_var(0, None, "".to_string(), Some(invalid_type))
                }
                x if x > 1 => {
                    let pos = *self.lobj(recv_list[recv_list.len() - 1]).pos();
                    self.error(pos, "method must have exactly one receiver".to_string());
                    recv_list[0] // continue with first receiver
                }
                x if x == 1 => recv_list[0],
                _ => unreachable!(),
            };
            recv_okey = Some(recv_var);

            // spec: "The receiver type must be of the form T or *T where T is a type name."
            // (ignore invalid types - error was reported before)
            let recv_var_val = self.lobj(recv_var);
            let recv_type = recv_var_val.typ().unwrap();
            let (&t, _) = lookup::try_deref(&recv_type, self.tc_objs);
            if t != invalid_type {
                let err_msg = if let Some(n) = self.otype(t).try_as_named() {
                    // spec: "The type denoted by T is called the receiver base type; it must not
                    // be a pointer or interface type and it must be declared in the same package
                    // as the method."
                    if *self.lobj(n.obj().unwrap()).pkg() != Some(self.pkg) {
                        Some("type not defined in this package")
                    } else {
                        match self.otype(*n.underlying()) {
                            typ::Type::Basic(b) => {
                                if b.typ() == typ::BasicType::UnsafePointer {
                                    Some("unsafe.Pointer")
                                } else {
                                    None
                                }
                            }
                            typ::Type::Pointer(_) | typ::Type::Interface(_) => {
                                Some("pointer or interface type")
                            }
                            _ => None,
                        }
                    }
                } else {
                    Some("basic or unnamed type")
                };
                if let Some(err) = err_msg {
                    let pos = *recv_var_val.pos();
                    let td = TypeDisplay::new(&recv_type, self.tc_objs);
                    self.error(pos, format!("invalid receiver {} ({})", td, err));
                    // ok to continue
                }
            }
        }

        let params_tuple = self
            .tc_objs
            .types
            .insert(typ::Type::Tuple(typ::TupleDetail::new(params)));
        let results_tuple = self
            .tc_objs
            .types
            .insert(typ::Type::Tuple(typ::TupleDetail::new(results)));
        let sig = typ::Type::Signature(typ::SignatureDetail::new(
            recv_okey,
            params_tuple,
            results_tuple,
            variadic,
            self.tc_objs,
        ));
        self.tc_objs.types.insert(sig)
    }

    /// type_internal drives type checking of types.
    /// Must only be called by defined_type.
    fn type_internal(
        &mut self,
        e: &Expr,
        def: Option<TypeKey>,
        fctx: &mut FilesContext,
    ) -> TypeKey {
        let set_underlying = |typ: Option<TypeKey>, tc_objs: &mut TCObjects| {
            if let Some(d) = def {
                tc_objs.types[d]
                    .try_as_named_mut()
                    .unwrap()
                    .set_underlying(typ.unwrap());
            }
        };
        let pos = e.pos(self.ast_objs);
        let result_t: Option<TypeKey> = match e {
            Expr::Bad(_) => None,
            Expr::Ident(i) => {
                let mut x = Operand::new();
                self.ident(&mut x, *i, def, true, fctx);
                match x.mode {
                    OperandMode::TypeExpr => {
                        set_underlying(x.typ, self.tc_objs);
                        x.typ
                    }
                    OperandMode::Invalid => None, // ignore - error reported before
                    OperandMode::NoValue => {
                        error_operand!(x, "{} used as type", self);
                        None
                    }
                    _ => {
                        error_operand!(x, "{} is not a type", self);
                        None
                    }
                }
            }
            Expr::Selector(s) => {
                let mut x = Operand::new();
                self.selector(&mut x, s);
                match x.mode {
                    OperandMode::TypeExpr => {
                        set_underlying(x.typ, self.tc_objs);
                        x.typ
                    }
                    OperandMode::Invalid => None, // ignore - error reported before
                    OperandMode::NoValue => {
                        error_operand!(x, "{} used as type", self);
                        None
                    }
                    _ => {
                        error_operand!(x, "{} is not a type", self);
                        None
                    }
                }
            }
            Expr::Paren(p) => Some(self.defined_type(&p.expr, def, fctx)),
            Expr::Array(a) => {
                if let Some(l) = &a.len {
                    let len = self.array_len(&l);
                    let elem = self.type_expr(&a.elt, fctx);
                    let t = self
                        .tc_objs
                        .types
                        .insert(typ::Type::Array(typ::ArrayDetail::new(elem, len)));
                    set_underlying(Some(t), self.tc_objs);
                    Some(t)
                } else {
                    let elem = self.indirect_type(&a.elt, fctx);
                    let t = self
                        .tc_objs
                        .types
                        .insert(typ::Type::Slice(typ::SliceDetail::new(elem)));
                    set_underlying(Some(t), self.tc_objs);
                    Some(t)
                }
            }
            Expr::Struct(s) => {
                let t = self.struct_type(s);
                set_underlying(Some(t), self.tc_objs);
                Some(t)
            }
            Expr::Star(s) => {
                let base = self.indirect_type(&s.expr, fctx);
                let t = self
                    .tc_objs
                    .types
                    .insert(typ::Type::Pointer(typ::PointerDetail::new(base)));
                set_underlying(Some(t), self.tc_objs);
                Some(t)
            }
            Expr::Func(f) => {
                let t = self.func_type(None, *f);
                set_underlying(Some(t), self.tc_objs);
                Some(t)
            }
            Expr::Interface(iface) => {
                let t = self.interface_type(iface, def);
                set_underlying(Some(t), self.tc_objs);
                Some(t)
            }
            Expr::Map(m) => {
                let k = self.indirect_type(&m.key, fctx);
                let v = self.indirect_type(&m.val, fctx);
                let t = self
                    .tc_objs
                    .types
                    .insert(typ::Type::Map(typ::MapDetail::new(k, v)));
                set_underlying(Some(t), self.tc_objs);

                let pos = m.key.pos(self.ast_objs);
                let f = move |checker: &mut Checker| {
                    if !typ::comparable(&k, checker.tc_objs) {
                        let td = TypeDisplay::new(&k, checker.tc_objs);
                        checker.error(pos, format!("invalid map key type {}", td));
                    }
                };
                fctx.later(Box::new(f));

                Some(t)
            }
            Expr::Chan(chan) => {
                let dir = match chan.dir {
                    ast::ChanDir::Send => typ::ChanDir::SendOnly,
                    ast::ChanDir::Recv => typ::ChanDir::RecvOnly,
                    ast::ChanDir::SendRecv => typ::ChanDir::SendRecv,
                };
                let elem = self.indirect_type(&chan.val, fctx);
                let t = self
                    .tc_objs
                    .types
                    .insert(typ::Type::Chan(typ::ChanDetail::new(dir, elem)));
                set_underlying(Some(t), self.tc_objs);
                Some(t)
            }
            _ => {
                let ed = ExprDisplay::new(e, self.ast_objs);
                self.error(pos, format!("{} is not a type", ed));
                None
            }
        };

        if let Some(t) = result_t {
            t
        } else {
            let invalid_type = self.invalid_type();
            set_underlying(Some(invalid_type), self.tc_objs);
            invalid_type
        }
    }

    /// type_or_nil type-checks the type expression (or nil value) e
    /// and returns the typ of e, or None.
    /// If e is neither a type nor nil, typOrNil returns Typ[Invalid].
    pub fn type_or_nil(&mut self, e: &Expr) -> Option<TypeKey> {
        let mut x = Operand::new();
        self.raw_expr(&mut x, e, None);
        let invalid_type = self.invalid_type();
        match x.mode {
            OperandMode::Invalid => Some(invalid_type), // ignore - error reported before
            OperandMode::NoValue => {
                error_operand!(x, "{} used as type", self);
                Some(invalid_type)
            }
            OperandMode::TypeExpr => x.typ,
            _ => {
                if x.mode == OperandMode::Value && x.is_nil(self.tc_objs.universe()) {
                    None
                } else {
                    error_operand!(x, "{} is not a type", self);
                    Some(invalid_type)
                }
            }
        }
    }

    fn array_len(&mut self, e: &Expr) -> Option<u64> {
        let mut x = Operand::new();
        self.expr(&mut x, e);
        if let OperandMode::Constant(v) = &x.mode {
            let t = x.typ.as_ref().unwrap();
            if typ::is_untyped(t, self.tc_objs) || typ::is_integer(t, self.tc_objs) {
                let int = v.to_int();
                let int_type = self
                    .otype(self.basic_type(typ::BasicType::Int))
                    .try_as_basic()
                    .unwrap();
                if let constant::Value::Int(_) = int.borrow() {
                    if int.representable(int_type, None) {
                        let (n, exact) = int.int_as_u64();
                        if exact {
                            return Some(n);
                        } else {
                            error_operand!(x, "invalid array length {}", self);
                            return None;
                        }
                    }
                }
            }
            error_operand!(x, "array length {} must be integer", self);
        } else {
            if x.mode != OperandMode::Invalid {
                error_operand!(x, "array length {} must be constant", self);
            }
        }
        None
    }

    fn collect_params(
        &mut self,
        skey: ScopeKey,
        fl: Option<&FieldList>,
        variadic_ok: bool,
    ) -> (Vec<ObjKey>, bool) {
        unimplemented!()
    }

    fn struct_type(&mut self, st: &ast::StructType) -> TypeKey {
        unimplemented!()
    }

    fn interface_type(&mut self, iface: &ast::InterfaceType, def: Option<TypeKey>) -> TypeKey {
        unimplemented!()
    }
}
