// Copyright 2022 The Goscript Authors. All rights reserved.
// Use of this source code is governed by a BSD-style
// license that can be found in the LICENSE file.

use crate::context::*;
use goscript_parser::ast::*;
use goscript_parser::objects::Objects as AstObjects;
use goscript_parser::objects::*;
use goscript_parser::{Map, Token};
use goscript_types::{ObjKey as TCObjKey, PackageKey as TCPackageKey, TCObjects, TypeInfo};
use goscript_vm::ffi::FfiCtx;
use goscript_vm::value::*;
use std::rc::Rc;

pub(crate) struct PkgHelper<'a> {
    tc_objs: &'a TCObjects,
    ast_objs: &'a AstObjects,
    pkg_map: &'a Map<TCPackageKey, PackageKey>,
}

impl<'a> PkgHelper<'a> {
    pub fn new(
        ast_objs: &'a AstObjects,
        tc_objs: &'a TCObjects,
        pkg_map: &'a Map<TCPackageKey, PackageKey>,
    ) -> PkgHelper<'a> {
        PkgHelper {
            tc_objs,
            ast_objs,
            pkg_map,
        }
    }

    pub fn gen_imports(&self, tcpkg: TCPackageKey, fctx: &mut FuncCtx) {
        let pkg = &self.tc_objs.pkgs[tcpkg];
        let unsafe_ = self.tc_objs.universe().unsafe_pkg();
        for tckey in pkg.imports().iter() {
            if tckey != unsafe_ {
                let key = self.pkg_map[tckey];
                fctx.emit_import(key, None);
            }
        }
    }

    pub fn get_runtime_key(&self, tcpkg: TCPackageKey) -> PackageKey {
        self.pkg_map[&tcpkg]
    }

    pub fn get_member_index(
        &self,
        fctx: &mut FuncCtx,
        okey: TCObjKey,
        ident: IdentKey,
    ) -> VirtualAddr {
        let tc_pkg = self.tc_objs.lobjs[okey].pkg().unwrap();
        let pkg = self.get_runtime_key(tc_pkg);
        let pkg_addr = fctx.add_const(FfiCtx::new_package(pkg));
        let index_addr = Addr::PkgMemberIndex(pkg, ident);
        VirtualAddr::PackageMember(pkg_addr, index_addr)
    }

    // sort_var_decls returns all var names and sorted var decl statments
    pub fn sort_var_decls(
        &self,
        files: &Vec<File>,
        ti: &TypeInfo,
    ) -> (Vec<IdentKey>, Vec<Rc<ValueSpec>>) {
        let mut orders = Map::new();
        for (i, init) in ti.init_order.iter().enumerate() {
            for okey in init.lhs.iter() {
                let name = self.tc_objs.lobjs[*okey].name();
                orders.insert(name, i);
            }
        }

        let mut names = vec![];
        let mut decls = vec![];
        for f in files.iter() {
            for d in f.decls.iter() {
                match d {
                    Decl::Gen(gdecl) => {
                        for spec_key in gdecl.specs.iter() {
                            if gdecl.token == Token::VAR {
                                let spec = &self.ast_objs.specs[*spec_key];
                                match spec {
                                    Spec::Value(v) => {
                                        names.extend(v.names.iter());
                                        let name = &self.ast_objs.idents[v.names[0]].name;
                                        if let Some(order) = orders.get(name) {
                                            decls.push((v.clone(), order));
                                        }
                                    }
                                    _ => unimplemented!(),
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
        decls.sort_by(|a, b| a.1.cmp(&b.1));
        (names, decls.into_iter().map(|x| x.0).collect())
    }
}
