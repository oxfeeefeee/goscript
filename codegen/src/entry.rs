// Copyright 2022 The Goscript Authors. All rights reserved.
// Use of this source code is governed by a BSD-style
// license that can be found in the LICENSE file.

#![allow(dead_code)]
use super::branch::BranchHelper;
use super::codegen::CodeGen;
use super::consts::*;
use super::context::*;
use super::package::PkgHelper;
use super::selector::*;
use super::types::TypeCache;
use goscript_parser::ast::Ident;
use goscript_parser::errors::ErrorList;
use goscript_parser::objects::Objects as AstObjects;
use goscript_parser::objects::*;
use goscript_parser::FileSet;
use goscript_types::{PackageKey as TCPackageKey, SourceRead, TCObjects, TraceConfig, TypeInfo};
use goscript_vm::gc::GcoVec;
use goscript_vm::null_key;
use goscript_vm::value::*;
use goscript_vm::vm::ByteCode;
use std::collections::HashMap;
use std::pin::Pin;
use std::vec;

pub struct EntryGen<'a> {
    objects: Pin<Box<VMObjects>>,
    consts: Consts,
    ast_objs: &'a AstObjects,
    tc_objs: &'a TCObjects,
    dummy_gcv: GcoVec,
    iface_selector: IfaceSelector,
    struct_selector: StructSelector,
    // pkg_map maps TCPackageKey to runtime PackageKey
    pkg_map: HashMap<TCPackageKey, PackageKey>,
    blank_ident: IdentKey,
}

impl<'a> EntryGen<'a> {
    pub fn new(asto: &'a AstObjects, tco: &'a TCObjects, bk: IdentKey) -> EntryGen<'a> {
        EntryGen {
            objects: Box::pin(VMObjects::new()),
            consts: Consts::new(),
            ast_objs: asto,
            tc_objs: tco,
            dummy_gcv: GcoVec::new(),
            iface_selector: IfaceSelector::new(),
            struct_selector: StructSelector::new(),
            pkg_map: HashMap::new(),
            blank_ident: bk,
        }
    }

    // generate the entry function for ByteCode
    fn gen_entry_func(&mut self, pkg: PackageKey, main_ident: IdentKey) -> FunctionKey {
        // import the 0th pkg and call the main function of the pkg
        let fmeta = self.objects.s_meta.default_sig;
        let f = GosValue::function_with_meta(
            null_key!(),
            fmeta.clone(),
            &mut self.objects,
            &self.dummy_gcv,
            FuncFlag::Default,
        );
        let fkey = *f.as_function();
        let mut fctx = FuncCtx::new(fkey, None, &self.consts);
        fctx.emit_import(pkg, None);
        let pkg_addr = Addr::Const(self.consts.add_package(pkg));
        let index = Addr::PkgMemberIndex(pkg, main_ident);
        fctx.emit_load_pkg(Addr::Regsiter(0), pkg_addr, index, None);
        fctx.emit_pre_call(Addr::Regsiter(0), 0, 0, None);
        fctx.emit_call(CallStyle::Default, None);
        fctx.emit_return(None, None, &self.objects.functions);
        *f.as_function()
    }

    pub fn gen(
        mut self,
        checker_result: &HashMap<TCPackageKey, TypeInfo>,
        main_pkg: TCPackageKey,
        main_ident: IdentKey,
    ) -> ByteCode {
        for (&tcpkg, _) in checker_result.iter() {
            let pkey = self.objects.packages.insert(PackageVal::new());
            self.pkg_map.insert(tcpkg, pkey);
        }
        let mut type_cache: TypeCache = HashMap::new();
        let mut branch_helper = BranchHelper::new();
        let mut result_funcs = vec![];
        for (tcpkg, ti) in checker_result.iter() {
            let mut pkg_helper = PkgHelper::new(self.ast_objs, self.tc_objs, &self.pkg_map);
            let cgen = CodeGen::new(
                &mut self.objects,
                &self.consts,
                self.ast_objs,
                self.tc_objs,
                &mut self.dummy_gcv,
                &ti,
                &mut type_cache,
                &mut self.iface_selector,
                &mut self.struct_selector,
                &mut branch_helper,
                &mut pkg_helper,
                self.pkg_map[tcpkg],
                self.blank_ident,
            );
            result_funcs.append(&mut cgen.gen_with_files(&ti.ast_files, *tcpkg));
        }
        for f in result_funcs.into_iter() {
            f.into_runtime_func(self.ast_objs, &mut self.objects, branch_helper.labels());
        }
        let entry = self.gen_entry_func(self.pkg_map[&main_pkg], main_ident);
        let consts = self.consts.into_runtime_consts(&mut self.objects);
        ByteCode::new(
            self.objects,
            consts,
            self.iface_selector.result(),
            self.struct_selector.result(),
            entry,
        )
    }
}

pub fn parse_check_gen<S: SourceRead>(
    path: &str,
    tconfig: &TraceConfig,
    reader: &S,
    fset: &mut FileSet,
) -> Result<ByteCode, ErrorList> {
    let asto = &mut AstObjects::new();
    let tco = &mut goscript_types::TCObjects::new();
    let results = &mut HashMap::new();
    let pkgs = &mut HashMap::new();
    let el = ErrorList::new();

    let importer = &mut goscript_types::Importer::new(
        &tconfig, reader, fset, pkgs, results, asto, tco, &el, 0,
    );
    let key = goscript_types::ImportKey::new(path, "./");
    let main_pkg = importer.import(&key);
    if el.len() > 0 {
        Err(el)
    } else {
        let blank_ident = asto.idents.insert(Ident::blank(0));
        let main_ident = asto.idents.insert(Ident::with_str(0, "main"));
        let gen = EntryGen::new(asto, tco, blank_ident);
        Ok(gen.gen(results, main_pkg.unwrap(), main_ident))
    }
}
