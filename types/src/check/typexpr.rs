#![allow(dead_code)]
use super::super::obj::EntityType;
use super::super::objects::{DeclInfoKey, ObjKey, PackageKey, ScopeKey, TCObjects, TypeKey};
use super::super::operand::{Operand, OperandMode};
use super::super::scope::Scope;
use super::check::{Checker, FilesContext};
use goscript_parser::ast::{Expr, FieldList};
use goscript_parser::objects::{FuncTypeKey, IdentKey};

impl<'a> Checker<'a> {
    /// ident type-checks identifier ikey and initializes x with the value or type of ikey.
    /// If an error occurred, x.mode is set to invalid.
    /// For the meaning of def, see Checker.defined_type, below.
    /// If want_type is set, the identifier e is expected to denote a type.
    pub fn ident(
        &mut self,
        x: &mut Operand,
        ikey: IdentKey,
        def: TypeKey,
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
                self.obj_decl(okey, Some(def), fctx);
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

    pub fn type_expr(&mut self, e: &Expr) -> TypeKey {
        unimplemented!()
    }

    pub fn defined_type(&mut self, e: &Expr, def: TypeKey) -> TypeKey {
        unimplemented!()
    }

    pub fn func_type(&mut self, sig: TypeKey, recv: Option<FieldList>, ftype: FuncTypeKey) {
        unimplemented!()
    }
}
