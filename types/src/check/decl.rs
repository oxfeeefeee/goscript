#![allow(dead_code)]

use super::super::constant;
use super::super::display::{LangObjDisplay, TypeDisplay};
use super::super::obj::*;
use super::super::objects::{DeclInfoKey, ObjKey, PackageKey, ScopeKey, TCObjects, TypeKey};
use super::super::operand::Operand;
use super::super::scope::Scope;
use super::super::typ::BasicType;
use super::assignment;
use super::check::{Checker, FilesContext, TypeInfo};
use super::expr;
use super::typexpr;
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

    pub fn obj_decl(&mut self, okey: ObjKey, def: Option<ObjKey>, fctx: &mut FilesContext) {
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

        // not really a loop, just to make sure trace_end gets called
        loop {
            let lobj = &mut self.tc_objs.lobjs[okey];
            if *lobj.color() == ObjColor::White && lobj.typ().is_some() {
                lobj.set_color(ObjColor::Black);
                break;
            }
            match lobj.color() {
                ObjColor::White => {}
                ObjColor::Gray(_) => {}
                ObjColor::Black => {}
            }
            break;
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

        loop {
            // provide valid constant value under all circumstances
            self.lobj_mut(okey).set_const_val(constant::Value::Unknown);
            // determine type, if any
            if let Some(e) = typ {
                let t = self.type_expr(e);
                let tval = &self.tc_objs.types[t];
                if !tval.is_const_type(self.tc_objs) {
                    let invalid_type = self.tc_objs.universe().types()[&BasicType::Invalid];
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
                    break;
                }
            }

            let mut x = Operand::new();
            if let Some(expr) = init {
                self.expr(&mut x, expr);
            }
            self.init_const(okey, &mut x);

            break;
        }

        self.octx.iota = None;
    }

    pub fn add_method_decls(&mut self, _okey: ObjKey) {
        unimplemented!()
    }
}
