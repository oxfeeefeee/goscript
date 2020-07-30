#![allow(dead_code)]
use super::super::constant;
use super::super::obj::{type_name_is_alias, EntityType, ObjColor};
use super::super::objects::{DeclInfoKey, ObjKey, ScopeKey, TypeKey};
use super::super::operand::Operand;
use super::super::scope::Scope;
use super::super::typ::{self};
use super::check::{Checker, FilesContext, ObjContext};
use super::stmt::BodyContainer;
use goscript_parser::ast::{self, Expr, Node};
use goscript_parser::objects::IdentKey;
use goscript_parser::position::Pos;
use goscript_parser::Token;
use std::collections::HashMap;

impl<'a> Checker<'a> {
    pub fn report_alt_decl(&self, okey: ObjKey) {
        let lobj = self.lobj(okey);
        let pos = lobj.pos();
        if pos > 0 {
            self.error(pos, format!("\tother declaration of {}", lobj.name()));
        }
    }

    pub fn declare(&mut self, skey: ScopeKey, ikey: Option<IdentKey>, okey: ObjKey, pos: Pos) {
        // spec: "The blank identifier, represented by the underscore
        // character _, may be used in a declaration like any other
        // identifier but the declaration does not introduce a new
        // binding."
        if self.lobj(okey).name() != "_" {
            let alt = Scope::insert(skey, okey, self.tc_objs).map(|x| x.clone());
            if let Some(o) = alt {
                let lobj = self.lobj(okey);
                self.error(
                    lobj.pos(),
                    format!("{} redeclared in this block", lobj.name()),
                );
                self.report_alt_decl(o);
                return;
            }
            self.lobj_mut(okey).set_scope_pos(pos);
        }
        if ikey.is_some() {
            self.result.record_def(ikey.unwrap(), Some(okey));
        }
    }

    pub fn obj_decl(&mut self, okey: ObjKey, def: Option<TypeKey>, fctx: &mut FilesContext) {
        let trace_end_data = if self.config().trace_checker {
            let lobj = self.lobj(okey);
            let pos = lobj.pos();
            let obj_display = self.new_dis(&okey);
            let bmsg = format!(
                "-- checking {} {} (objPath = {})",
                lobj.color(),
                obj_display,
                self.obj_path_str(&fctx.obj_path)
            );
            let end = format!("=> {}", obj_display);
            self.trace_begin(pos, &bmsg);
            Some((pos, end))
        } else {
            None
        };

        // Checking the declaration of obj means inferring its type
        // (and possibly its value, for constants).
        // An object's type (and thus the object) may be in one of
        // three states which are expressed by colors:
        //
        // - an object whose type is not yet known is painted white (initial color)
        // - an object whose type is in the process of being inferred is painted grey
        // - an object whose type is fully inferred is painted black
        //
        // During type inference, an object's color changes from white to grey
        // to black (pre-declared objects are painted black from the start).
        // A black object (i.e., its type) can only depend on (refer to) other black
        // ones. White and grey objects may depend on white and black objects.
        // A dependency on a grey object indicates a cycle which may or may not be
        // valid.
        //
        // When objects turn grey, they are pushed on the object path (a stack);
        // they are popped again when they turn black. Thus, if a grey object (a
        // cycle) is encountered, it is on the object path, and all the objects
        // it depends on are the remaining objects on that path. Color encoding
        // is such that the color value of a grey object indicates the index of
        // that object in the object path.

        // During type-checking, white objects may be assigned a type without
        // traversing through objDecl; e.g., when initializing constants and
        // variables. Update the colors of those objects here (rather than
        // everywhere where we set the type) to satisfy the color invariants.

        let lobj = &mut self.tc_objs.lobjs[okey];
        if lobj.color() == ObjColor::White && lobj.typ().is_some() {
            lobj.set_color(ObjColor::Black);

            if let Some((p, m)) = &trace_end_data {
                self.trace_end(*p, m);
            }
            return;
        }

        match lobj.color() {
            ObjColor::White => {
                assert!(lobj.typ().is_none());
                lobj.set_color(ObjColor::Gray(fctx.push(okey)));

                let dkey = self.obj_map[&okey];
                let d = &self.tc_objs.decls[dkey];
                // create a new octx for the checker
                let mut octx = ObjContext::new();
                octx.scope = Some(*d.file_scope());
                std::mem::swap(&mut self.octx, &mut octx);

                let lobj = &self.tc_objs.lobjs[okey];
                match lobj.entity_type() {
                    EntityType::Const(_) => {
                        self.octx.decl = Some(dkey);
                        let cd = d.as_const();
                        let (typ, init) = (cd.typ.clone(), cd.init.clone());
                        self.const_decl(okey, &typ, &init, fctx);
                    }
                    EntityType::Var(_) => {
                        self.octx.decl = Some(dkey);
                        let cd = d.as_var();
                        let (lhs, typ, init) = (cd.lhs.clone(), cd.typ.clone(), cd.init.clone());
                        self.var_decl(okey, lhs.as_ref(), &typ, &init, fctx);
                    }
                    EntityType::TypeName => {
                        let cd = d.as_type();
                        let (typ, alias) = (cd.typ.clone(), cd.alias);
                        self.type_decl(okey, &typ, def, alias, fctx);
                    }
                    EntityType::Func(_) => {
                        self.func_decl(okey, dkey, fctx);
                    }
                    _ => unreachable!(),
                }

                // handled defered actions:
                std::mem::swap(&mut self.octx, &mut octx); // restore octx
                self.lobj_mut(fctx.pop()).set_color(ObjColor::Black);
            }
            ObjColor::Black => {
                assert!(lobj.typ().is_some());
            }
            ObjColor::Gray(_) => {
                // We have a cycle.
                // In the existing code, this is marked by a non-nil type
                // for the object except for constants and variables whose
                // type may be non-nil (known), or nil if it depends on the
                // not-yet known initialization value.
                // In the former case, set the type to Typ[Invalid] because
                // we have an initialization cycle. The cycle error will be
                // reported later, when determining initialization order.
                let lobj = &self.tc_objs.lobjs[okey];
                let invalid_type = self.invalid_type();
                match lobj.entity_type() {
                    EntityType::Const(_) | EntityType::Var(_) => {
                        if self.invalid_type_cycle(okey, fctx) || lobj.typ().is_none() {
                            self.tc_objs.lobjs[okey].set_type(Some(invalid_type));
                        }
                    }
                    EntityType::TypeName => {
                        if self.invalid_type_cycle(okey, fctx) {
                            self.tc_objs.lobjs[okey].set_type(Some(invalid_type));
                        }
                    }
                    EntityType::Func(_) => {
                        if self.invalid_type_cycle(okey, fctx) {
                            // Don't set obj.typ to Typ[Invalid] here
                            // because plenty of code type-asserts that
                            // functions have a *Signature type. Grey
                            // functions have their type set to an empty
                            // signature which makes it impossible to
                            // initialize a variable with the function.
                        }
                    }
                    _ => unreachable!(),
                }
                let lobj = self.lobj(okey); // make the borrow checker happy
                assert!(lobj.typ().is_some());
            }
        }

        if let Some((p, m)) = &trace_end_data {
            self.trace_end(*p, m);
        }
    }

    /// invalid_type_cycle returns true if the cycle starting with obj is invalid and
    /// reports an error.
    pub fn invalid_type_cycle(&self, okey: ObjKey, fctx: &mut FilesContext) -> bool {
        // Given the number of constants and variables (nval) in the cycle
        // and the cycle length (ncycle = number of named objects in the cycle),
        // we distinguish between cycles involving only constants and variables
        // (nval = ncycle), cycles involving types (and functions) only
        // (nval == 0), and mixed cycles (nval != 0 && nval != ncycle).
        // We ignore functions at the moment (taking them into account correctly
        // is complicated and it doesn't improve error reporting significantly).
        //
        // A cycle must have at least one indirection and one type definition
        // to be permitted: If there is no indirection, the size of the type
        // cannot be computed (it's either infinite or 0); if there is no type
        // definition, we have a sequence of alias type names which will expand
        // ad infinitum.
        let lobj = self.lobj(okey);
        let mut has_indir = false;
        let mut has_type_def = false;
        let mut nval = 0;
        let start = match lobj.color() {
            ObjColor::Gray(v) => v,
            _ => unreachable!(),
        };
        let cycle = &fctx.obj_path[start..];
        let mut ncycle = cycle.len(); // including indirections
        for o in cycle {
            let oval = self.lobj(*o);
            match oval.entity_type() {
                EntityType::Const(_) | EntityType::Var(_) => {
                    nval += 1;
                }
                EntityType::TypeName => {
                    if o == self.tc_objs.universe().indir() {
                        ncycle -= 1; // don't count (indirections are not objects)
                        has_indir = true;
                    } else {
                        // Determine if the type name is an alias or not. For
                        // package-level objects, use the object map which
                        // provides syntactic information (which doesn't rely
                        // on the order in which the objects are set up). For
                        // local objects, we can rely on the order, so use
                        // the object's predicate.
                        let alias = if let Some(d) = self.obj_map.get(o) {
                            // package-level object
                            self.decl_info(*d).as_type().alias
                        } else {
                            // function local object
                            type_name_is_alias(*o, self.tc_objs)
                        };
                        if alias {
                            has_type_def = true;
                        }
                    }
                }
                EntityType::Func(_) => {} // ignored for now
                _ => unreachable!(),
            }
        }

        // A cycle involving only constants and variables is invalid but we
        // ignore them here because they are reported via the initialization
        // cycle check.
        if nval == ncycle {
            return false;
        }

        // A cycle involving only types (and possibly functions) must have at
        // least one indirection and one type definition to be permitted: If
        // there is no indirection, the size of the type cannot be computed
        // (it's either infinite or 0); if there is no type definition, we
        // have a sequence of alias type names which will expand ad infinitum.
        if nval == 0 && has_indir && has_type_def {
            return false; // cycle is permitted
        }

        // report error
        let pos = lobj.pos();
        self.error(
            pos,
            format!("illegal cycle in declaration of {}", lobj.name()),
        );
        for o in cycle {
            if o == self.tc_objs.universe().indir() {
                continue;
            }
            self.error(pos, format!("\t{} refers to", self.lobj(*o).name()));
        }
        self.error(pos, format!("\t{} refers to", lobj.name()));

        true
    }

    pub fn const_decl(
        &mut self,
        okey: ObjKey,
        typ: &Option<Expr>,
        init: &Option<Expr>,
        fctx: &mut FilesContext,
    ) {
        let lobj = self.lobj(okey);
        assert!(lobj.typ().is_none());
        self.octx.iota = Some(lobj.const_val().clone());

        // provide valid constant value under all circumstances
        self.lobj_mut(okey).set_const_val(constant::Value::Unknown);
        // determine type, if any
        if let Some(e) = typ {
            let t = self.type_expr(e, fctx);
            let tval = &self.tc_objs.types[t];
            if !tval.is_const_type(self.tc_objs) {
                let invalid_type = self.invalid_type();
                if tval.underlying().unwrap_or(t) == invalid_type {
                    self.error(
                        e.pos(self.ast_objs),
                        format!("invalid constant type {}", self.new_dis(&t)),
                    );
                }
                self.lobj_mut(okey).set_type(Some(invalid_type));

                // clear iota
                self.octx.iota = None;
                return;
            }
        }

        let mut x = Operand::new();
        if let Some(expr) = init {
            self.expr(&mut x, expr, fctx);
        }
        self.init_const(okey, &mut x, fctx);

        // clear iota
        self.octx.iota = None;
    }

    pub fn var_decl(
        &mut self,
        okey: ObjKey,
        lhs: Option<&Vec<ObjKey>>,
        typ: &Option<Expr>,
        init: &Option<Expr>,
        fctx: &mut FilesContext,
    ) {
        debug_assert!(self.lobj(okey).typ().is_none());

        // determine type, if any
        if let Some(texpr) = typ {
            let t = self.type_expr(texpr, fctx);
            self.lobj_mut(okey).set_type(Some(t));
            // We cannot spread the type to all lhs variables if there
            // are more than one since that would mark them as checked
            // (see Checker::obj_decl) and the assignment of init exprs,
            // if any, would not be checked.
        }

        // check initialization
        if init.is_none() {
            if typ.is_none() {
                // error reported before by arityMatch
                let invalid = self.invalid_type();
                self.lobj_mut(okey).set_type(Some(invalid));
            }
            return;
        }

        if lhs.is_none() || lhs.as_ref().unwrap().len() == 1 {
            assert!(lhs.is_none() || lhs.as_ref().unwrap()[0] == okey);
            let mut x = Operand::new();
            self.expr(&mut x, init.as_ref().unwrap(), fctx);
            self.init_var(okey, &mut x, "variable declaration", fctx);
            return;
        }

        debug_assert!(lhs.as_ref().unwrap().iter().find(|&&x| x == okey).is_some());

        // We have multiple variables on the lhs and one init expr.
        // Make sure all variables have been given the same type if
        // one was specified, otherwise they assume the type of the
        // init expression values
        if typ.is_some() {
            let t = self.lobj(okey).typ();
            for o in lhs.as_ref().unwrap().iter() {
                self.lobj_mut(*o).set_type(t);
            }
        }

        self.init_vars(
            lhs.as_ref().unwrap(),
            &vec![init.clone().unwrap()],
            None,
            fctx,
        );
    }

    pub fn type_decl(
        &mut self,
        okey: ObjKey,
        typ: &Expr,
        def: Option<TypeKey>,
        alias: bool,
        fctx: &mut FilesContext,
    ) {
        debug_assert!(self.lobj(okey).typ().is_none());

        if alias {
            let invalid = self.invalid_type();
            self.lobj_mut(okey).set_type(Some(invalid));
            let t = self.type_expr(typ, fctx);
            self.lobj_mut(okey).set_type(Some(t));
        } else {
            let named_key = self.tc_objs.new_t_named(Some(okey), None, vec![]);
            if let Some(d) = def {
                self.tc_objs.types[d]
                    .try_as_named_mut()
                    .unwrap()
                    .set_underlying(named_key);
            }
            // make sure recursive type declarations terminate
            self.lobj_mut(okey).set_type(Some(named_key));

            // determine underlying type of named
            self.defined_type(typ, Some(named_key), fctx);

            // The underlying type of named may be itself a named type that is
            // incomplete:
            //
            //	type (
            //		A B
            //		B *C
            //		C A
            //	)
            //
            // The type of C is the (named) type of A which is incomplete,
            // and which has as its underlying type the named type B.
            // Determine the (final, unnamed) underlying type by resolving
            // any forward chain (they always end in an unnamed type).
            let underlying = typ::deep_underlying_type(named_key, self.tc_objs);
            self.tc_objs.types[named_key]
                .try_as_named_mut()
                .unwrap()
                .set_underlying(underlying);
        }
        self.add_method_decls(okey, fctx);
    }

    pub fn func_decl(&mut self, okey: ObjKey, dkey: DeclInfoKey, fctx: &mut FilesContext) {
        debug_assert!(self.lobj(okey).typ().is_none());
        // func declarations cannot use iota
        debug_assert!(self.octx.iota.is_none());

        let d = &self.tc_objs.decls[dkey].as_func();
        let fdecl_key = d.fdecl;
        let fdecl = &self.ast_objs.fdecls[fdecl_key];
        let (recv, typ) = (fdecl.recv.clone(), fdecl.typ);
        let sig_key = self.func_type(recv.as_ref(), typ, fctx);
        self.lobj_mut(okey).set_type(Some(sig_key));

        // check for 'init' func
        let fdecl = &self.ast_objs.fdecls[fdecl_key];
        let sig = &self.tc_objs.types[sig_key].try_as_signature().unwrap();
        let lobj = &self.tc_objs.lobjs[okey];
        if sig.recv().is_none()
            && lobj.name() == "init"
            && (sig.params_count(self.tc_objs) > 0 || sig.results_count(self.tc_objs) > 0)
        {
            self.error(
                fdecl.pos(self.ast_objs),
                "func init must have no arguments and no return values".to_string(),
            );
            // ok to continue
        }

        if let Some(_) = &fdecl.body {
            let name = lobj.name().clone();
            let body = BodyContainer::FuncDecl(fdecl_key);
            let f = move |checker: &mut Checker, fctx: &mut FilesContext| {
                checker.func_body(dkey, &name, sig_key, body, None, fctx);
            };
            fctx.later(Box::new(f));
        }
    }

    pub fn add_method_decls(&mut self, okey: ObjKey, fctx: &mut FilesContext) {
        // get associated methods
        // (Checker.collect_objects only collects methods with non-blank names;
        // Checker.resolve_base_type_name ensures that obj is not an alias name
        // if it has attached methods.)
        if !fctx.methods.contains_key(&okey) {
            return;
        }
        let methods = fctx.methods.remove(&okey).unwrap();
        // don't use TypeName.is_alias (requires fully set up object)
        debug_assert!(!self.decl_info(self.obj_map[&okey]).as_type().alias);

        let mut mset: HashMap<String, ObjKey> = HashMap::new();
        // LangObj.typ() can only be Some(Type::Named)?
        // see original go code
        let type_key = self.lobj(okey).typ().unwrap();
        let named = self.otype(type_key).try_as_named().unwrap();
        if let Some(struc) = self.otype(named.underlying()).try_as_struct() {
            for f in struc.fields().iter() {
                if self.lobj(*f).name() != "_" {
                    assert!(self.insert_obj_to_set(&mut mset, *f).is_none());
                }
            }
        }
        // if we allow Check.check be called multiple times; additional package files
        // may add methods to already type-checked types. Add pre-existing methods
        // so that we can detect redeclarations.
        for m in named.methods().iter() {
            let lobj = self.lobj(*m);
            assert!(lobj.name() != "_");
            assert!(self.insert_obj_to_set(&mut mset, *m).is_none());
        }

        // get valid methods
        let mut valids: Vec<ObjKey> = methods
            .into_iter()
            .filter(|m| {
                // spec: "For a base type, the non-blank names of methods bound
                // to it must be unique."
                let mobj = self.lobj(*m);
                assert!(mobj.name() != "_");
                if self.insert_obj_to_set(&mut mset, *m).is_some() {
                    match mobj.entity_type() {
                        EntityType::Var(_) => self.error(
                            mobj.pos(),
                            format!("field and method with the same name {}", mobj.name()),
                        ),
                        EntityType::Func(_) => {
                            self.error(
                                mobj.pos(),
                                format!(
                                    "method {} already declared for {}",
                                    mobj.name(),
                                    self.new_dis(m)
                                ),
                            );
                        }
                        _ => unreachable!(),
                    }
                    self.report_alt_decl(*m);
                    false
                } else {
                    true
                }
            })
            .collect();
        // append valid methods
        self.tc_objs.types[type_key]
            .try_as_named_mut()
            .unwrap()
            .methods_mut()
            .append(&mut valids);
    }

    pub fn decl_stmt(&mut self, decl: ast::Decl, fctx: &mut FilesContext) {
        match decl {
            ast::Decl::Bad(_) => { /*ignore*/ }
            ast::Decl::Func(_) => {
                self.invalid_ast(decl.pos(self.ast_objs), "unknown ast.FuncDecl node")
            }
            ast::Decl::Gen(gdecl) => {
                let mut last_full_const_spec: Option<ast::Spec> = None;
                let specs = &(*gdecl).specs;
                for (iota, spec_key) in specs.iter().enumerate() {
                    let spec = &self.ast_objs.specs[*spec_key].clone();
                    let spec_pos = spec.pos(self.ast_objs);
                    let spec_end = spec.end(self.ast_objs);
                    match spec {
                        ast::Spec::Value(vs) => {
                            let vspec = &**vs;
                            let top = fctx.delayed_count();
                            let mut current_vspec = None;
                            let lhs: Vec<ObjKey> = match gdecl.token {
                                Token::CONST => {
                                    if vspec.typ.is_some() || vspec.values.len() > 0 {
                                        last_full_const_spec = Some(spec.clone());
                                        current_vspec = Some(vspec);
                                    } else {
                                        // no ValueSpec with type or init exprs,
                                        // try get the last one
                                        if let Some(spec) = &last_full_const_spec {
                                            match spec {
                                                ast::Spec::Value(v) => {
                                                    current_vspec = Some(&*v);
                                                }
                                                _ => unreachable!(),
                                            }
                                        }
                                    }

                                    // all lhs
                                    vspec
                                        .names
                                        .clone()
                                        .into_iter()
                                        .enumerate()
                                        .map(|(i, name)| {
                                            let ident = &self.ast_objs.idents[name];
                                            let okey = self.tc_objs.new_const(
                                                ident.pos,
                                                Some(self.pkg),
                                                ident.name.clone(),
                                                None,
                                                constant::Value::with_i64(iota as i64),
                                            );
                                            let init = if current_vspec.is_some()
                                                && i < current_vspec.unwrap().values.len()
                                            {
                                                Some(current_vspec.unwrap().values[i].clone())
                                            } else {
                                                None
                                            };
                                            let typ =
                                                current_vspec.map(|x| x.typ.clone()).flatten();
                                            self.const_decl(okey, &typ, &init, fctx);
                                            okey
                                        })
                                        .collect()
                                }
                                Token::VAR => {
                                    let vars: Vec<ObjKey> = vspec
                                        .names
                                        .iter()
                                        .map(|x| {
                                            let ident = &self.ast_objs.idents[*x];
                                            self.tc_objs.new_var(
                                                ident.pos,
                                                Some(self.pkg),
                                                ident.name.clone(),
                                                None,
                                            )
                                        })
                                        .collect();
                                    let n_to_1 = vspec.values.len() == 1 && vspec.names.len() > 1;
                                    if n_to_1 {
                                        self.var_decl(
                                            vars[0],
                                            Some(&vars),
                                            &vspec.typ.clone(),
                                            &Some(vspec.values[0].clone()),
                                            fctx,
                                        );
                                    } else {
                                        for (i, okey) in vars.iter().enumerate() {
                                            self.var_decl(
                                                *okey,
                                                None,
                                                &vspec.typ.clone(),
                                                &vspec.values.get(i).map(|x| x.clone()),
                                                fctx,
                                            );
                                        }
                                    }
                                    vars
                                }
                                _ => {
                                    self.invalid_ast(
                                        spec_pos,
                                        &format!("invalid token {}", gdecl.token),
                                    );
                                    vec![]
                                }
                            };

                            self.arity_match(vspec, gdecl.token == Token::CONST, current_vspec);

                            // process function literals in init expressions before scope changes
                            fctx.process_delayed(top, self);

                            // spec: "The scope of a constant or variable identifier declared
                            // inside a function begins at the end of the ConstSpec or VarSpec
                            // (ShortVarDecl for short variable declarations) and ends at the
                            // end of the innermost containing block."
                            for (i, name) in vspec.names.iter().enumerate() {
                                self.declare(
                                    self.octx.scope.unwrap(),
                                    Some(*name),
                                    lhs[i],
                                    spec_end,
                                );
                            }
                        }
                        ast::Spec::Type(ts) => {
                            let ident = self.ast_ident(ts.name);
                            let (pos, name) = (ident.pos, ident.name.clone());
                            let okey = self.tc_objs.new_type_name(pos, Some(self.pkg), name, None);
                            // spec: "The scope of a type identifier declared inside a function
                            // begins at the identifier in the TypeSpec and ends at the end of
                            // the innermost containing block."
                            self.declare(self.octx.scope.unwrap(), Some(ts.name), okey, pos);
                            // mark and unmark type before calling Checker.type_decl;
                            // its type is still nil (see Checker.obj_decl)
                            self.lobj_mut(okey)
                                .set_color(ObjColor::Gray(fctx.push(okey)));
                            self.type_decl(okey, &ts.typ.clone(), None, ts.assign > 0, fctx);
                            self.lobj_mut(fctx.pop()).set_color(ObjColor::Black);
                        }
                        _ => self.invalid_ast(spec_pos, "const, type, or var declaration expected"),
                    }
                }
            }
        }
    }
}
