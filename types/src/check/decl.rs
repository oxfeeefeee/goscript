#![allow(dead_code)]

use super::super::constant;
use super::super::display::{LangObjDisplay, TypeDisplay};
use super::super::obj::{EntityType, LangObj, ObjColor};
use super::super::objects::{DeclInfoKey, ObjKey, PackageKey, ScopeKey, TCObjects, TypeKey};
use super::super::operand::Operand;
use super::super::scope::Scope;
use super::super::typ::{self, NamedDetail, Type};

use super::check::{Checker, FilesContext, ObjContext, TypeInfo};
use goscript_parser::ast::Expr;
use goscript_parser::ast::Node;
use goscript_parser::objects::IdentKey;
use goscript_parser::position::Pos;

impl<'a> Checker<'a> {
    pub fn report_alt_decl(&self, okey: &ObjKey) {
        let lobj = self.lobj(*okey);
        let pos = *lobj.pos();
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
                    *lobj.pos(),
                    format!("{} redeclared in this block", lobj.name()),
                );
                self.report_alt_decl(&o);
                return;
            }
            self.lobj_mut(okey).set_scope_pos(pos);
        }
        if ikey.is_some() {
            self.result.record_def(ikey.unwrap(), okey);
        }
    }

    pub fn obj_decl(&mut self, okey: ObjKey, def: Option<TypeKey>, fctx: &mut FilesContext) {
        let trace_end_data = if self.config().trace_checker {
            let lobj = self.lobj(okey);
            let pos = *lobj.pos();
            let obj_display = LangObjDisplay::new(&okey, self.tc_objs);
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
        if *lobj.color() == ObjColor::White && lobj.typ().is_some() {
            lobj.set_color(ObjColor::Black);

            if let Some((p, m)) = &trace_end_data {
                self.trace_end(*p, m);
            }
            return;
        }

        match lobj.color() {
            ObjColor::White => {
                assert!(lobj.typ().is_none());
                let index = fctx.push(okey);
                lobj.set_color(ObjColor::Gray(index));

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
                        self.const_decl(okey, &typ, &init);
                    }
                    EntityType::Var(_, _, _) => {
                        self.octx.decl = Some(dkey);
                        let cd = d.as_var();
                        let (lhs, typ, init) = (cd.lhs.clone(), cd.typ.clone(), cd.init.clone());
                        self.var_decl(okey, &lhs, &typ, &init);
                    }
                    EntityType::TypeName => {
                        let cd = d.as_type();
                        let (typ, alias) = (cd.typ.clone(), cd.alias);
                        self.type_decl(okey, &typ, def, alias);
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
                    EntityType::Const(_) | EntityType::Var(_, _, _) => {
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
            ObjColor::Gray(v) => *v,
            _ => unreachable!(),
        };
        let cycle = &fctx.obj_path[start..];
        let mut ncycle = cycle.len(); // including indirections
        for o in cycle {
            let oval = self.lobj(*o);
            match oval.entity_type() {
                EntityType::Const(_) | EntityType::Var(_, _, _) => {
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
                            oval.type_name_is_alias()
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
        let pos = *lobj.pos();
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

    pub fn const_decl(&mut self, okey: ObjKey, typ: &Option<Expr>, init: &Option<Expr>) {
        let lobj = self.lobj(okey);
        assert!(lobj.typ().is_none());
        self.octx.iota = Some(lobj.const_val().clone());

        // provide valid constant value under all circumstances
        self.lobj_mut(okey).set_const_val(constant::Value::Unknown);
        // determine type, if any
        if let Some(e) = typ {
            let t = self.type_expr(e);
            let tval = &self.tc_objs.types[t];
            if !tval.is_const_type(self.tc_objs) {
                let invalid_type = self.invalid_type();
                if tval.underlying().unwrap_or(&t) == &invalid_type {
                    self.error(
                        e.pos(self.ast_objs),
                        format!(
                            "invalid constant type {}",
                            TypeDisplay::new(&t, self.tc_objs)
                        ),
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
            self.expr(&mut x, expr);
        }
        self.init_const(okey, &mut x);

        // clear iota
        self.octx.iota = None;
    }

    pub fn var_decl(
        &mut self,
        okey: ObjKey,
        lhs: &Option<Vec<ObjKey>>,
        typ: &Option<Expr>,
        init: &Option<Expr>,
    ) {
        debug_assert!(self.lobj(okey).typ().is_none());

        // determine type, if any
        if let Some(texpr) = typ {
            let t = self.type_expr(texpr);
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
            self.expr(&mut x, init.as_ref().unwrap());
            self.init_var(okey, &mut x, "variable declaration");
            return;
        }

        debug_assert!(lhs.as_ref().unwrap().iter().find(|&&x| x == okey).is_some());

        // We have multiple variables on the lhs and one init expr.
        // Make sure all variables have been given the same type if
        // one was specified, otherwise they assume the type of the
        // init expression values
        if typ.is_some() {
            let t = *self.lobj(okey).typ();
            for o in lhs.as_ref().unwrap().iter() {
                self.lobj_mut(*o).set_type(t);
            }
        }

        self.init_vars(lhs.as_ref().unwrap(), &vec![init.clone().unwrap()], 0);
    }

    pub fn type_decl(&mut self, okey: ObjKey, typ: &Expr, def: Option<TypeKey>, alias: bool) {
        debug_assert!(self.lobj(okey).typ().is_none());

        if alias {
            let invalid = self.invalid_type();
            self.lobj_mut(okey).set_type(Some(invalid));
            let t = self.type_expr(typ);
            self.lobj_mut(okey).set_type(Some(t));
        } else {
            let named = Type::Named(NamedDetail::new(Some(okey), None, vec![], self.tc_objs));
            let named_key = self.tc_objs.types.insert(named);
            if let Some(d) = def {
                self.tc_objs.types[d]
                    .try_as_named_mut()
                    .unwrap()
                    .set_underlying(named_key);
            }
            // make sure recursive type declarations terminate
            self.lobj_mut(okey).set_type(Some(named_key));

            // determine underlying type of named
            self.defined_type(typ, named_key);

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
            let underlying = *typ::deep_underlying_type(&named_key, self.tc_objs);
            self.tc_objs.types[named_key]
                .try_as_named_mut()
                .unwrap()
                .set_underlying(underlying);
        }
        self.add_method_decls(okey);
    }

    pub fn func_decl(&mut self, okey: ObjKey, dkey: DeclInfoKey, fctx: &mut FilesContext) {
        /*
        let f = move |checker: &mut Checker| {
            checker.add_method_decls(okey);
        };
        fctx.later(Box::new(f));
        let lobj = self.lobj(okey);
        */
        unimplemented!()
    }

    pub fn add_method_decls(&mut self, _okey: ObjKey) {
        unimplemented!()
    }
}
