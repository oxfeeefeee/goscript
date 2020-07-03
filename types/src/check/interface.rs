// This file implements the collection of an interface's methods
// without relying on partially computed types of methods or interfaces
// for interface types declared at the package level.
//
// Because interfaces must not embed themselves, directly or indirectly,
// the method set of a valid interface can always be computed independent
// of any cycles that might exist via method signatures
//
// Except for blank method name and interface cycle errors, no errors
// are reported. Affected methods or embedded interfaces are silently
// dropped. Subsequent type-checking of the interface will check
// signatures and embedded interfaces and report errors at that time.
//
// Only info_from_type_lit should be called directly from code outside this file
// to compute an ifaceInfo.

#![allow(dead_code)]
use super::super::display::{ExprIfaceDisplay, MethodInfoDisplay};
use super::super::obj;
use super::super::objects::{ObjKey, PackageKey, ScopeKey, TCObjects, TypeKey};
use super::super::scope::Scope;
use super::super::typ;
use super::check::{Checker, FilesContext};
use goscript_parser::ast::{self, Expr, Node};
use goscript_parser::objects::{FieldKey, IdentKey, Objects as AstObjects};
use goscript_parser::{Parser, Pos};
use std::borrow::Cow;
use std::collections::{HashMap, HashSet};
use std::fmt;
use std::fmt::Write;

/// MethodInfo represents an interface method.
/// At least one of src or fun must be non-None.
/// (Methods declared in the current package have a non-None scope
/// and src, and eventually a non-None fun field; imported and pre-
/// declared methods have a None scope and src, and only a non-None
/// fun field.)
pub struct MethodInfo {
    // scope of interface method; or None
    pub scope: Option<ScopeKey>,
    // syntax tree representation of interface method; or None
    pub src: Option<FieldKey>,
    // corresponding fully type-checked method type(LangObj::Func); or None
    pub fun: Option<ObjKey>,
}

impl MethodInfo {
    pub fn with_fun(fun: ObjKey) -> MethodInfo {
        MethodInfo {
            scope: None,
            src: None,
            fun: Some(fun),
        }
    }

    pub fn fmt(
        &self,
        f: &mut fmt::Formatter<'_>,
        tc_objs: &TCObjects,
        ast_objs: &AstObjects,
    ) -> fmt::Result {
        let s = if let Some(okey) = self.fun {
            tc_objs.lobjs[okey].name()
        } else {
            &ast_objs.idents[ast_objs.fields[self.src.unwrap()].names[0]].name
        };
        f.write_str(s)
    }

    pub fn pos(&self, tc_objs: &TCObjects, ast_objs: &AstObjects) -> Pos {
        if let Some(okey) = self.fun {
            *tc_objs.lobjs[okey].pos()
        } else {
            self.src.unwrap().pos(ast_objs)
        }
    }

    pub fn id<'a>(
        &self,
        pkey: PackageKey,
        tc_objs: &'a TCObjects,
        ast_objs: &'a AstObjects,
    ) -> Cow<'a, str> {
        if let Some(okey) = self.fun {
            tc_objs.lobjs[okey].id(tc_objs)
        } else {
            let pkg = Some(&tc_objs.pkgs[pkey]);
            let name = &ast_objs.idents[ast_objs.fields[self.src.unwrap()].names[0]].name;
            obj::get_id(pkg, name)
        }
    }
}

/// IfaceInfo describes the method set for an interface.
pub struct IfaceInfo {
    pub explicits: usize,
    pub methods: Vec<MethodInfo>,
}

impl IfaceInfo {
    pub fn new(explicits: usize, methods: Vec<MethodInfo>) -> IfaceInfo {
        IfaceInfo {
            explicits: explicits,
            methods: methods,
        }
    }

    pub fn new_empty() -> IfaceInfo {
        IfaceInfo::new(0, vec![])
    }

    pub fn is_empty(&self) -> bool {
        self.methods.is_empty()
    }

    pub fn fmt(
        &self,
        f: &mut fmt::Formatter<'_>,
        tc_objs: &TCObjects,
        ast_objs: &AstObjects,
    ) -> fmt::Result {
        f.write_str("interface{")?;
        for (i, m) in self.methods.iter().enumerate() {
            if i > 0 {
                f.write_char(' ')?;
            }
            m.fmt(f, tc_objs, ast_objs)?;
        }
        f.write_char('}')
    }
}

impl<'a> Checker<'a> {
    /// info_from_type_lit computes the method set for the given interface iface
    /// declared in scope.
    /// If a corresponding type name exists (tname is_some), it is used for
    /// cycle detection and to cache the method set.
    /// The result is the method set, or None if there is a cycle via embedded
    /// interfaces. A is_some result doesn't mean that there were no errors,
    /// but they were either reported (e.g., blank methods), or will be found
    /// (again) when computing the interface's type.
    /// If tname is not None it must be the last element in path.
    pub fn info_from_type_lit(
        &self,
        skey: ScopeKey,
        iface: &ast::InterfaceType,
        tname: Option<ObjKey>,
        path: &mut Vec<ObjKey>,
        fctx: &mut FilesContext,
    ) -> Option<IfaceInfo> {
        if self.config().trace_checker {
            let ed = ExprIfaceDisplay::new(iface, self.ast_objs);
            let pstr = self.obj_path_str(path);
            let opstr = self.obj_path_str(&fctx.obj_path);
            let msg = format!(
                "-- collect methods for {} (path = {}, objPath = {})",
                ed, pstr, opstr
            );
            self.trace_begin(iface.interface, &msg);
        }

        if self.config().trace_checker {
            let ed = ExprIfaceDisplay::new(iface, self.ast_objs);
            self.trace_end(iface.interface, &format!("=> {}", ed));
        }
        unimplemented!()
    }

    // info_from_type_name computes the method set for the given type name
    // which must denote a type whose underlying type is an interface.
    // The same result qualifications apply as for info_from_type_lit.
    // info_from_type_name should only be called from info_from_type_lit.
    fn info_from_type_name(
        &self,
        skey: ScopeKey,
        name: IdentKey,
        path: &mut Vec<ObjKey>,
        fctx: &mut FilesContext,
    ) -> Option<IfaceInfo> {
        // A single call of info_from_type_name handles a sequence of (possibly
        // recursive) type declarations connected via unqualified type names.
        // The general scenario looks like this:
        //      ...
        //      type Pn T        // previous declarations leading to T, path = [..., Pn]
        //      type T interface { T0; ... }  // T0 leads to call of info_from_type_name
        //
        //      // info_from_type_name(name = T0, path = [..., Pn, T])
        //      type T0 T1       // path = [..., Pn, T, T0]
        //      type T1 T2  <-+  // path = [..., Pn, T, T0, T1]
        //      type T2 ...   |  // path = [..., Pn, T, T0, T1, T2]
        //      type Tn T1  --+  // path = [..., Pn, T, T0, T1, T2, Tn] and T1 is in path => cycle
        // info_from_type_name returns nil when such a cycle is detected. But in
        // contrast to cycles involving interfaces, we must not report the
        // error for "type name only" cycles because they will be found again
        // during type-checking of embedded interfaces. Reporting those cycles
        // here would lead to double reporting. Cycles involving embedding are
        // not reported again later because type-checking of interfaces relies
        // on the IfaceInfos computed here which are cycle-free by design.
        //
        // Remember the path length to detect "type name only" cycles.
        let start = path.len();

        let mut ident = self.ast_ident(name);
        loop {
            let lookup =
                Scope::lookup_parent(&skey, &ident.name, Some(self.octx.pos), self.tc_objs);
            if lookup.is_none() {
                break;
            }
            let tname = lookup.as_ref().unwrap().1;
            let tname_val = self.lobj(tname);
            if &obj::EntityType::TypeName != tname_val.entity_type() {
                break;
            }

            // We have a type name. It may be predeclared (error type),
            // imported (dot import), or declared by a type declaration.
            // It may not be an interface (e.g., predeclared type int).
            // Resolve it by analyzing each possible case.

            // Abort but don't report an error if we have a "type name only"
            // cycle (see big function comment).
            if self.has_cycle(tname, &path[start..], false) {
                break;
            }
            // Abort and report an error if we have a general cycle.
            if self.has_cycle(tname, &path, false) {
                break;
            }

            path.push(tname);

            // If tname is a package-level type declaration, it must be
            // in the obj_map. Follow the RHS of that declaration if so.
            // The RHS may be a literal type (likely case), or another
            // (possibly parenthesized and/or qualified) type name.
            // (The declaration may be an alias declaration, but it
            // doesn't matter for the purpose of determining the under-
            // lying interface.)
            if let Some(decl_key) = self.obj_map.get(&tname) {
                let decl = &self.tc_objs.decls[*decl_key].as_type();
                let ty = Parser::unparen(&decl.typ);
                match ty {
                    Expr::Ident(i) => {
                        // type tname T
                        ident = self.ast_ident(*i);
                    }
                    Expr::Selector(sel) => {
                        // type tname p.T
                        return self.info_from_qualified_type_mame(decl.file_scope, sel);
                    }
                    Expr::Interface(iface) => {
                        // type tname interface{...}
                        return self.info_from_type_lit(
                            decl.file_scope,
                            iface,
                            Some(tname),
                            path,
                            fctx,
                        );
                    }
                    // type tname X // and X is not an interface type
                    _ => break,
                }
            } else {
                // If tname is not a package-level declaration, in a well-typed
                // program it should be a predeclared (error type), imported (dot
                // import), or function local declaration. Either way, it should
                // have been fully declared before use, except if there is a direct
                // cycle, and direct cycles will be caught above. Also, the denoted
                // type should be an interface (e.g., int is not an interface).
                if let Some(ty) = tname_val.typ() {
                    let ty = typ::underlying_type(&ty, self.tc_objs);
                    if let typ::Type::Interface(i) = self.otype(*ty) {
                        return Some(self.info_from_type(i));
                    }
                }
                break;
            }
        }
        None
    }

    /// like Checker::declare_in_set but for method infos.
    fn declare_in_method_set<'b>(
        &self,
        set: &'b mut HashMap<String, &'b MethodInfo>,
        mi: &'b MethodInfo,
        pos: Pos,
    ) -> bool {
        let id = mi.id(self.pkg, self.tc_objs, self.ast_objs);
        if let Some(alt) = set.insert(id.to_string(), mi) {
            let md = MethodInfoDisplay::new(mi, self.ast_objs, self.tc_objs);
            self.error(pos, format!("{} redeclared", md));
            let mpos = mi.pos(self.tc_objs, self.ast_objs);
            if mpos > 0 {
                // We use "other" rather than "previous" here because
                // the first declaration seen may not be textually
                // earlier in the source.
                let md = MethodInfoDisplay::new(alt, self.ast_objs, self.tc_objs);
                self.error(mpos, format!("\tother declaration of {}", md));
            }
            false
        } else {
            true
        }
    }

    /// info_from_qualified_type_mame returns the method set for the given qualified
    /// type name, or None.
    fn info_from_qualified_type_mame(
        &self,
        skey: ScopeKey,
        sel: &ast::SelectorExpr,
    ) -> Option<IfaceInfo> {
        if let Some(name) = sel.expr.try_as_ident() {
            let ident = self.ast_ident(*name);
            if let Some((_, obj1)) =
                Scope::lookup_parent(&skey, &ident.name, Some(self.octx.pos), self.tc_objs)
            {
                let obj_val = self.lobj(obj1);
                if let obj::EntityType::PkgName(imported, _) = obj_val.entity_type() {
                    debug_assert!(obj_val.pkg() == &Some(self.pkg));
                    let imported_val = &self.tc_objs.pkgs[*imported];
                    let scope = &self.tc_objs.scopes[*imported_val.scope()];
                    if let Some(obj2) = scope.lookup(&self.ast_ident(sel.sel).name) {
                        let obj_val2 = self.lobj(*obj2);
                        if !obj_val2.exported() {
                            return None;
                        }
                        if let obj::EntityType::TypeName = obj_val2.entity_type() {
                            let t = typ::underlying_type(
                                obj_val2.typ().as_ref().unwrap(),
                                self.tc_objs,
                            );
                            if let Some(iface) = self.otype(*t).try_as_interface() {
                                return Some(self.info_from_type(iface));
                            }
                        }
                    }
                }
            }
        }
        None
    }

    /// infoFromType computes the method set for the given interface type.
    fn info_from_type(&self, iface: &typ::InterfaceDetail) -> IfaceInfo {
        let all_methods_ref = iface.all_methods();
        let all_methods = all_methods_ref.as_ref().unwrap();
        let all_methods_len = all_methods.len();

        let mut mis = iface
            .methods()
            .iter()
            .map(|x| MethodInfo::with_fun(*x))
            .collect();
        if all_methods_len == iface.methods().len() {
            return IfaceInfo::new(all_methods_len, mis);
        }

        // there are embedded method, put them after explicite methods
        let set: HashSet<ObjKey> = iface.methods().clone().into_iter().collect();
        let mut embedded: Vec<MethodInfo> = all_methods
            .iter()
            .filter_map(|x| {
                if set.contains(x) {
                    None
                } else {
                    Some(MethodInfo::with_fun(*x))
                }
            })
            .collect();
        mis.append(&mut embedded);
        IfaceInfo::new(iface.methods().len(), mis)
    }
}
