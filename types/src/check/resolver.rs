// Copyright 2022 The Goscript Authors. All rights reserved.
// Use of this source code is governed by a BSD-style
// license that can be found in the LICENSE file.
//
//
// This code is adapted from the offical Go code written in Go
// with license as follows:
// Copyright 2013 The Go Authors. All rights reserved.
// Use of this source code is governed by a BSD-style
// license that can be found in the LICENSE file.

#![allow(dead_code)]
use crate::SourceRead;

use super::super::constant;
use super::super::importer::ImportKey;
use super::super::obj::EntityType;
use super::super::objects::{DeclInfoKey, ObjKey, PackageKey, ScopeKey};
use super::check::{Checker, FilesContext};
use goscript_parser::ast::{self, Expr, Node};
use goscript_parser::objects::{FuncDeclKey, IdentKey};
use goscript_parser::{Pos, Token};
use std::collections::HashSet;

#[derive(Debug)]
pub struct DeclInfoConst {
    pub file_scope: ScopeKey,  // scope of file containing this declaration
    pub typ: Option<Expr>,     // type, or None
    pub init: Option<Expr>,    // init/orig expression, or None
    pub deps: HashSet<ObjKey>, // deps tracks initialization expression dependencies.
}

#[derive(Debug)]
pub struct DeclInfoVar {
    pub file_scope: ScopeKey,     // scope of file containing this declaration
    pub lhs: Option<Vec<ObjKey>>, // lhs of n:1 variable declarations, or None
    pub typ: Option<Expr>,        // type, or None
    pub init: Option<Expr>,       // init/orig expression, or None
    pub deps: HashSet<ObjKey>,    // deps tracks initialization expression dependencies.
}

#[derive(Debug)]
pub struct DeclInfoType {
    pub file_scope: ScopeKey, // scope of file containing this declaration
    pub typ: Expr,            // type
    pub alias: bool,          // type alias declaration
}

#[derive(Debug)]
pub struct DeclInfoFunc {
    pub file_scope: ScopeKey,  // scope of file containing this declaration
    pub fdecl: FuncDeclKey,    // func declaration, or None
    pub deps: HashSet<ObjKey>, // deps tracks initialization expression dependencies.
}

/// DeclInfo describes a package-level const, type, var, or func declaration.
#[derive(Debug)]
pub enum DeclInfo {
    Const(DeclInfoConst),
    Var(DeclInfoVar),
    Type(DeclInfoType),
    Func(DeclInfoFunc),
}

impl DeclInfo {
    pub fn new_const(file_scope: ScopeKey, typ: Option<Expr>, init: Option<Expr>) -> DeclInfo {
        DeclInfo::Const(DeclInfoConst {
            file_scope: file_scope,
            typ: typ,
            init: init,
            deps: HashSet::new(),
        })
    }

    pub fn new_var(
        file_scope: ScopeKey,
        lhs: Option<Vec<ObjKey>>,
        typ: Option<Expr>,
        init: Option<Expr>,
    ) -> DeclInfo {
        DeclInfo::Var(DeclInfoVar {
            file_scope: file_scope,
            lhs: lhs,
            typ: typ,
            init: init,
            deps: HashSet::new(),
        })
    }

    pub fn new_type(file_scope: ScopeKey, typ: Expr, alias: bool) -> DeclInfo {
        DeclInfo::Type(DeclInfoType {
            file_scope: file_scope,
            typ: typ,
            alias: alias,
        })
    }

    pub fn new_func(file_scope: ScopeKey, fdecl: FuncDeclKey) -> DeclInfo {
        DeclInfo::Func(DeclInfoFunc {
            file_scope: file_scope,
            fdecl: fdecl,
            deps: HashSet::new(),
        })
    }

    pub fn as_const(&self) -> &DeclInfoConst {
        match self {
            DeclInfo::Const(c) => c,
            _ => unreachable!(),
        }
    }

    pub fn as_var(&self) -> &DeclInfoVar {
        match self {
            DeclInfo::Var(v) => v,
            _ => unreachable!(),
        }
    }

    pub fn as_type(&self) -> &DeclInfoType {
        match self {
            DeclInfo::Type(t) => t,
            _ => unreachable!(),
        }
    }

    pub fn as_func(&self) -> &DeclInfoFunc {
        match self {
            DeclInfo::Func(f) => f,
            _ => unreachable!(),
        }
    }

    pub fn file_scope(&self) -> &ScopeKey {
        match self {
            DeclInfo::Const(c) => &c.file_scope,
            DeclInfo::Var(v) => &v.file_scope,
            DeclInfo::Type(t) => &t.file_scope,
            DeclInfo::Func(f) => &f.file_scope,
        }
    }

    pub fn deps(&self) -> &HashSet<ObjKey> {
        match self {
            DeclInfo::Const(c) => &c.deps,
            DeclInfo::Var(v) => &v.deps,
            DeclInfo::Func(f) => &f.deps,
            _ => unreachable!(),
        }
    }

    pub fn deps_mut(&mut self) -> &mut HashSet<ObjKey> {
        match self {
            DeclInfo::Const(c) => &mut c.deps,
            DeclInfo::Var(v) => &mut v.deps,
            DeclInfo::Func(f) => &mut f.deps,
            _ => unreachable!(),
        }
    }

    pub fn add_dep(&mut self, okey: ObjKey) {
        self.deps_mut().insert(okey);
    }
}

impl<'a, S: SourceRead> Checker<'a, S> {
    pub fn collect_objects(&mut self, fctx: &mut FilesContext<S>) {
        let mut all_imported: HashSet<PackageKey> = self
            .package(self.pkg)
            .imports()
            .iter()
            .map(|x| *x)
            .collect();
        // list of methods with non-blank names
        let mut methods: Vec<ObjKey> = Vec::new();
        for (file_num, file) in fctx.files.iter().enumerate() {
            // the original go version record a none here, what for?
            //self.result_mut().result.(file.name,  None)

            // Use the actual source file extent rather than ast::File extent since the
            // latter doesn't include comments which appear at the start or end of the file.
            // Be conservative and use the ast::File extent if we don't have a position::File.
            let mut pos = file.pos(self.ast_objs);
            let mut end = file.end(self.ast_objs);
            if let Some(f) = self.fset.file(pos) {
                pos = f.base();
                end = pos + f.size();
            }
            let parent_scope = Some(*self.package(self.pkg).scope());
            let scope_comment = fctx.file_name(file_num, self);
            let file_scope = self
                .tc_objs
                .new_scope(parent_scope, pos, end, scope_comment, false);
            self.result.record_scope(file, file_scope);

            for decl in file.decls.iter() {
                match decl {
                    ast::Decl::Bad(_) => {}
                    ast::Decl::Gen(gdecl) => {
                        let mut last_full_const_spec: Option<ast::Spec> = None;
                        let specs = &(*gdecl).specs;
                        for (iota, spec_key) in specs.iter().enumerate() {
                            let spec = &self.ast_objs.specs[*spec_key].clone();
                            let spec_pos = spec.pos(self.ast_objs);
                            match spec {
                                ast::Spec::Import(is) => {
                                    let ispec = &**is;
                                    let path = match self.valid_import_path(&ispec.path) {
                                        Ok(p) => p,
                                        Err(e) => {
                                            self.error(
                                                ispec.path.pos,
                                                format!("invalid import path ({})", e),
                                            );
                                            continue;
                                        }
                                    };
                                    let dir = self.file_dir(file);
                                    let imp =
                                        self.import_package(ispec.path.pos, path.to_string(), dir);

                                    // add package to list of explicit imports
                                    // (this functionality is provided as a convenience
                                    // for clients; it is not needed for type-checking)
                                    if !all_imported.contains(&imp) {
                                        all_imported.insert(imp);
                                        self.package_mut(self.pkg).add_import(imp);
                                    }

                                    let name = ispec.name.map_or(
                                        self.package(imp).name().clone().unwrap(),
                                        |x| {
                                            // see if local name overrides imported package name
                                            let ident = &self.ast_ident(x);
                                            if ident.name == "init" {
                                                self.error_str(
                                                    ident.pos,
                                                    "cannot declare init - must be func",
                                                );
                                            }
                                            ident.name.clone()
                                        },
                                    );

                                    let pkg_name_obj = self.tc_objs.new_pkg_name(
                                        spec_pos,
                                        Some(self.pkg),
                                        name.to_owned(),
                                        imp,
                                    );
                                    if let Some(n) = ispec.name {
                                        // in a dot-import, the dot represents the package
                                        self.result.record_def(n, Some(pkg_name_obj));
                                    } else {
                                        self.result.record_implicit(spec, pkg_name_obj);
                                    }

                                    // add import to file scope
                                    if name == "." {
                                        // merge imported scope with file scope
                                        let pkg_val = self.package(imp);
                                        let scope_val = self.scope(*pkg_val.scope());
                                        let elems: Vec<ObjKey> = scope_val
                                            .elems()
                                            .iter()
                                            .filter_map(|(_, v)| {
                                                if self.lobj(*v).exported() {
                                                    Some(*v)
                                                } else {
                                                    None
                                                }
                                            })
                                            .collect();
                                        for elem in elems.into_iter() {
                                            self.declare(file_scope, None, elem, 0);
                                        }
                                        // add position to set of dot-import positions for this file
                                        // (this is only needed for "imported but not used" errors)
                                        fctx.add_unused_dot_import(&file_scope, &imp, spec_pos);
                                    } else {
                                        // declare imported package object in file scope
                                        self.declare(file_scope, None, pkg_name_obj, 0);
                                    }
                                }
                                ast::Spec::Value(vs) => {
                                    let vspec = &**vs;
                                    match gdecl.token {
                                        Token::CONST => {
                                            let mut current_vspec = None;
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
                                            // declare all constants
                                            for (i, name) in
                                                vspec.names.clone().into_iter().enumerate()
                                            {
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
                                                let d = self.tc_objs.decls.insert(
                                                    DeclInfo::new_const(file_scope, typ, init),
                                                );
                                                let _ = self.declare_pkg_obj(name, okey, d);
                                            }
                                            self.arity_match(vspec, true, current_vspec);
                                        }
                                        Token::VAR => {
                                            let lhs: Vec<ObjKey> = vspec
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
                                            let n_to_1 =
                                                vspec.values.len() == 1 && vspec.names.len() > 1;
                                            let n_to_1_di = if n_to_1 {
                                                Some(self.tc_objs.decls.insert(DeclInfo::new_var(
                                                    file_scope,
                                                    Some(lhs.clone()),
                                                    vspec.typ.clone(),
                                                    Some(vspec.values[0].clone()),
                                                )))
                                            } else {
                                                None
                                            };
                                            for (i, name) in vspec.names.iter().enumerate() {
                                                let di = if n_to_1 {
                                                    n_to_1_di.unwrap()
                                                } else {
                                                    self.tc_objs.decls.insert(DeclInfo::new_var(
                                                        file_scope,
                                                        None,
                                                        vspec.typ.clone(),
                                                        vspec.values.get(i).map(|x| x.clone()),
                                                    ))
                                                };
                                                let _ = self.declare_pkg_obj(*name, lhs[i], di);
                                            }

                                            self.arity_match(vspec, false, None);
                                        }
                                        _ => self.error(
                                            spec_pos,
                                            format!("invalid token {}", gdecl.token),
                                        ),
                                    }
                                }
                                ast::Spec::Type(ts) => {
                                    let tspec = &**ts;
                                    let ident = &self.ast_objs.idents[tspec.name];
                                    let okey = self.tc_objs.new_type_name(
                                        ident.pos,
                                        Some(self.pkg),
                                        ident.name.clone(),
                                        None,
                                    );
                                    let di = self.tc_objs.decls.insert(DeclInfo::new_type(
                                        file_scope,
                                        tspec.typ.clone(),
                                        tspec.assign > 0,
                                    ));
                                    let _ = self.declare_pkg_obj(tspec.name, okey, di);
                                }
                            }
                        }
                    }
                    ast::Decl::Func(fdkey) => {
                        let fdecl = &self.ast_objs.fdecls[*fdkey];
                        let ident_key = fdecl.name;
                        let ident = &self.ast_objs.idents[ident_key];
                        let lobj = self.tc_objs.new_func(
                            ident.pos,
                            Some(self.pkg),
                            ident.name.clone(),
                            None,
                        );
                        if fdecl.recv.is_none() {
                            // regular function
                            let scope = *self.package(self.pkg).scope();
                            if ident.name == "init" {
                                self.tc_objs.lobjs[lobj].set_parent(Some(scope));
                                self.result.record_def(ident_key, Some(lobj));
                                if fdecl.body.is_none() {
                                    self.error(ident.pos, "missing function body".to_owned());
                                }
                            } else {
                                self.declare(scope, Some(ident_key), lobj, 0);
                            }
                        } else {
                            // method
                            // (Methods with blank _ names are never found; no need to collect
                            // them for later type association. They will still be type-checked
                            // with all the other functions.)
                            if ident.name != "_" {
                                methods.push(lobj);
                            }
                            self.result.record_def(ident_key, Some(lobj));
                        }
                        let di = self
                            .tc_objs
                            .decls
                            .insert(DeclInfo::new_func(file_scope, *fdkey));
                        self.obj_map.insert(lobj, di);
                        let order = self.obj_map.len() as u32;
                        self.lobj_mut(lobj).set_order(order);
                    }
                }
            }
        }
        // verify that objects in package and file scopes have different names
        let pkg_scope = self.scope(*self.package(self.pkg).scope());
        for s in pkg_scope.children().iter() {
            for (_, okey) in self.scope(*s).elems() {
                let obj_val = self.lobj(*okey);
                if let Some(alt) = pkg_scope.lookup(obj_val.name()) {
                    let alt_val = self.lobj(*alt);
                    match obj_val.entity_type() {
                        EntityType::PkgName(pkey, _) => {
                            let pkg_val = self.package(*pkey);
                            self.error(
                                alt_val.pos(),
                                format!(
                                    "{} already declared through import of {}",
                                    alt_val.name(),
                                    pkg_val
                                ),
                            );
                        }
                        _ => {
                            let pkg_val = self.package(obj_val.pkg().unwrap());
                            self.error(
                                alt_val.pos(),
                                format!(
                                    "{} already declared through dot-import of {}",
                                    alt_val.name(),
                                    pkg_val
                                ),
                            );
                        }
                    }
                    self.report_alt_decl(*okey);
                }
            }
        }
        // Now that we have all package scope objects and all methods,
        // associate methods with receiver base type name where possible.
        // Ignore methods that have an invalid receiver. They will be
        // type-checked later, with regular functions.
        for f in methods.into_iter() {
            let fdkey = self.tc_objs.decls[self.obj_map[&f]].as_func().fdecl;
            let fdecl = &self.ast_objs.fdecls[fdkey];
            if let Some(fl) = &fdecl.recv {
                // f is a method.
                // determine the receiver base type and associate f with it.
                let typ = &self.ast_objs.fields[fl.list[0]].typ;
                if let Some((ptr, base)) = self.resolve_base_type_name(typ) {
                    self.lobj_mut(f)
                        .entity_type_mut()
                        .func_set_has_ptr_recv(ptr);
                    fctx.methods.entry(base).or_default().push(f);
                }
            }
        }
    }

    /// package_objects typechecks all package objects, but not function bodies.
    pub fn package_objects(&mut self, fctx: &mut FilesContext<S>) {
        // process package objects in source order for reproducible results
        let mut obj_list: Vec<ObjKey> = self.obj_map.iter().map(|(o, _)| *o).collect();
        obj_list.sort_by(|a, b| self.lobj(*a).order().cmp(&self.lobj(*b).order()));

        for o in obj_list.iter() {
            let lobj = self.lobj(*o);
            if lobj.entity_type().is_type_name() && lobj.typ().is_some() {
                self.add_method_decls(*o, fctx);
            }
        }

        // We process non-alias declarations first, in order to avoid situations where
        // the type of an alias declaration is needed before it is available. In general
        // this is still not enough, as it is possible to create sufficiently convoluted
        // recursive type definitions that will cause a type alias to be needed before it
        // is available (see Golang issue #25838 for examples).
        // As an aside, the cmd/compiler suffers from the same problem (Golang #25838).
        let alias_list: Vec<ObjKey> = obj_list
            .into_iter()
            .filter(|&o| {
                if self.lobj(o).entity_type().is_type_name()
                    && self.decl_info(self.obj_map[&o]).as_type().alias
                {
                    true
                } else {
                    // phase 1
                    self.obj_decl(o, None, fctx);
                    false
                }
            })
            .collect();
        for o in alias_list.into_iter() {
            // phase 2
            self.obj_decl(o, None, fctx);
        }

        // At this point we may have a non-empty FilesContext.methods map; this means that
        // not all entries were deleted at the end of type_decl because the respective
        // receiver base types were not found. In that case, an error was reported when
        // declaring those methods. We can now safely discard this map.
        fctx.methods.clear();
    }

    /// unused_imports checks for unused imports.
    pub fn unused_imports(&mut self, fctx: &mut FilesContext<S>) {
        // check use of regular imported packages
        let pkg_scope = self.scope(*self.package(self.pkg).scope());
        for s in pkg_scope.children().iter() {
            for (_, okey) in self.scope(*s).elems() {
                let obj_val = self.lobj(*okey);
                match obj_val.entity_type() {
                    EntityType::PkgName(pkey, used) => {
                        if !*used {
                            let (path, base) = self.pkg_path_and_name(*pkey);
                            if obj_val.name() == base {
                                self.soft_error(
                                    obj_val.pos(),
                                    format!("{} imported but not used", path),
                                );
                            } else {
                                self.soft_error(
                                    obj_val.pos(),
                                    format!("{} imported but not used as {}", path, base),
                                );
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
        // check use of dot-imported packages
        for (_, imports) in fctx.unused_dot_imports.iter() {
            for (pkey, pos) in imports.iter() {
                self.soft_error(
                    *pos,
                    format!("{} imported but not used", self.package(*pkey).path()),
                );
            }
        }
    }

    /// arity_match checks that the lhs and rhs of a const or var decl
    /// have the appropriate number of names and init exprs.
    /// set 'cst' as true for const decls, 'init' is not used for var decls.
    pub fn arity_match(&self, s: &ast::ValueSpec, cst: bool, init: Option<&ast::ValueSpec>) {
        let l = s.names.len();
        let r = if cst {
            if let Some(i) = init {
                i.values.len()
            } else {
                0
            }
        } else {
            s.values.len()
        };
        if !cst && r == 0 {
            // var decl w/o init expr
            if s.typ.is_none() {
                self.error(
                    self.ast_ident(s.names[0]).pos,
                    "missing type or init expr".to_owned(),
                );
            }
        } else if l < r {
            if l < s.values.len() {
                let expr = &s.values[l];
                let ed = self.new_dis(expr);
                self.error(ed.pos(), format!("extra init expr {}", ed));
            } else {
                let pos = self.ast_ident(init.unwrap().names[0]).pos;
                self.error(
                    self.ast_ident(s.names[0]).pos,
                    format!("extra init expr at {}", self.position(pos)),
                );
            }
        } else if l > r && (cst || r != 1) {
            let ident = self.ast_ident(s.names[r]);
            self.error(ident.pos, format!("missing init expr for {}", ident.name));
        }
    }

    // resolve_base_type_name returns the non-alias base type name for typ, and whether
    // there was a pointer indirection to get to it. The base type name must be declared
    // in package scope, and there can be at most one pointer indirection. If no such type
    // name exists, the returned base is nil.
    // Algorithm: Starting from a type expression, which may be a name,
    // we follow that type through alias declarations until we reach a
    // non-alias type name. If we encounter anything but pointer types or
    // parentheses we're done. If we encounter more than one pointer type
    // we're done.
    fn resolve_base_type_name(&self, expr: &Expr) -> Option<(bool, ObjKey)> {
        let scope = self.scope(*self.package(self.pkg).scope());
        let mut typ = expr;
        let mut path = Vec::new();
        let mut ptr = false;
        loop {
            typ = Checker::<S>::unparen(typ);
            if let Expr::Star(t) = typ {
                // if we've already seen a pointer, we're done
                if ptr {
                    break;
                }
                ptr = true;
                typ = Checker::<S>::unparen(&t.expr);
            }

            // typ must be the name
            if let Expr::Ident(i) = typ {
                // name must denote an object found in the current package scope
                // (note that dot-imported objects are not in the package scope!)
                let ident = &self.ast_objs.idents[*i];
                if let Some(&okey) = scope.lookup(&ident.name) {
                    let lobj = self.lobj(okey);
                    // the object must be a type name...
                    if !lobj.entity_type().is_type_name() {
                        break;
                    }
                    // ... which we have not seen before
                    if self.has_cycle(okey, &path, false) {
                        break;
                    }
                    if let DeclInfo::Type(t) = &self.tc_objs.decls[self.obj_map[&okey]] {
                        if !t.alias {
                            // we're done if tdecl defined tname as a new type
                            // (rather than an alias)
                            return Some((ptr, okey));
                        } else {
                            // otherwise, continue resolving
                            typ = &t.typ;
                            path.push(okey);
                            continue;
                        }
                    }
                }
            }
            break;
        }
        None
    }

    fn valid_import_path(&self, blit: &'a ast::BasicLit) -> Result<&'a str, String> {
        let path = blit.token.get_literal();
        if path.len() < 3 || (!path.starts_with('"') || !path.ends_with('"')) {
            return Err("empty string".to_owned());
        }
        let result = &path[1..path.len() - 1];
        let mut illegal_chars: Vec<char> = r##"!"#$%&'()*,:;<=>?[\]^{|}`"##.chars().collect();
        illegal_chars.push('\u{FFFD}');
        if let Some(c) = result
            .chars()
            .find(|&x| !x.is_ascii_graphic() || x.is_whitespace() || illegal_chars.contains(&x))
        {
            return Err(format!("invalid character: {}", c));
        }
        Ok(result)
    }

    /// declare_pkg_obj declares obj in the package scope, records its ident -> obj mapping,
    /// and updates check.objMap. The object must not be a function or method.
    fn declare_pkg_obj(
        &mut self,
        ikey: IdentKey,
        okey: ObjKey,
        dkey: DeclInfoKey,
    ) -> Result<(), ()> {
        let ident = self.ast_ident(ikey);
        let lobj = self.lobj(okey);
        assert_eq!(&ident.name, lobj.name());
        // spec: "A package-scope or file-scope identifier with name init
        // may only be declared to be a function with this (func()) signature."
        if &ident.name == "init" {
            self.error_str(ident.pos, "cannot declare init - must be func");
            return Err(());
        }
        // spec: "The main package must have package name main and declare
        // a function main that takes no arguments and returns no value."
        let pkg_name = self.package(self.pkg).name();
        if &ident.name == "main" && pkg_name.is_some() && pkg_name.as_ref().unwrap() == "main" {
            self.error_str(ident.pos, "cannot declare main - must be func");
            return Err(());
        }
        let scope = *self.package(self.pkg).scope();
        self.declare(scope, Some(ikey), okey, 0);
        self.obj_map.insert(okey, dkey);
        let order = self.obj_map.len() as u32;
        self.lobj_mut(okey).set_order(order);
        Ok(())
    }

    fn import_package(&mut self, pos: Pos, path: String, dir: String) -> PackageKey {
        // If we already have a package for the given (path, dir)
        // pair, use it instead of doing a full import.
        // Checker.imp_map only caches packages that are marked Complete
        // or fake (dummy packages for failed imports). Incomplete but
        // non-fake packages do require an import to complete them.
        let key = ImportKey::new(&path, &dir);
        if let Some(imp) = self.imp_map.get(&key) {
            return *imp;
        }

        let mut imported = self.new_importer(pos).import(&key);
        if imported.is_err() {
            self.error(pos, format!("could not import {}", &path));
            // create a new fake package
            let mut name = &path[0..path.len()];
            if name.len() > 0 && name.ends_with('/') {
                name = &name[0..name.len() - 1];
            }
            if let Some(i) = name.rfind('/') {
                name = &name[i..name.len()]
            }
            let pkg = self.tc_objs.new_package(path.clone());
            self.package_mut(pkg).mark_fake_with_name(name.to_owned());
            imported = Ok(pkg);
        }
        self.imp_map.insert(key, imported.unwrap());
        imported.unwrap()
    }

    // pkg_name returns the package's path and name (last element) of a PkgName obj.
    fn pkg_path_and_name(&self, pkey: PackageKey) -> (&str, &str) {
        let pkg_val = self.package(pkey);
        let path = pkg_val.path();
        if let Some((i, _)) = path.match_indices('/').next() {
            if i > 0 {
                return (&path, &path[i + 1..]);
            }
        }
        (&path, &path)
    }

    /// dir makes a good-faith attempt to return the directory
    /// portion of path. If path is empty, the result is ".".
    fn file_dir(&self, file: &ast::File) -> String {
        let path = self
            .fset
            .file(self.ast_ident(file.name).pos)
            .unwrap()
            .name();
        if let Some((i, _)) = path.rmatch_indices(&['/', '\\'][..]).next() {
            if i > 0 {
                return path[0..i].to_owned();
            }
        }
        ".".to_owned()
    }
}
