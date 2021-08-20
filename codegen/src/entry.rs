#![allow(dead_code)]
use super::codegen::CodeGen;
use super::emit::Emitter;
use super::interface::IfaceMapping;
use super::package::PkgVarPairs;
use super::types::TypeCache;
use goscript_parser::ast::Ident;
use goscript_parser::errors::ErrorList;
use goscript_parser::objects::Objects as AstObjects;
use goscript_parser::objects::*;
use goscript_parser::FileSet;
use goscript_types::{Config, PackageKey as TCPackageKey, TCObjects, TypeInfo};
use goscript_vm::gc::GcoVec;
use goscript_vm::instruction::*;
use goscript_vm::null_key;
use goscript_vm::value::*;
use goscript_vm::vm::ByteCode;
use std::collections::HashMap;
use std::pin::Pin;

pub struct EntryGen<'a> {
    objects: Pin<Box<VMObjects>>,
    ast_objs: &'a AstObjects,
    tc_objs: &'a TCObjects,
    dummy_gcv: GcoVec,
    packages: Vec<PackageKey>,
    iface_mapping: IfaceMapping,
    // pkg_indices maps TCPackageKey to the index (in the generated code) of the package
    pkg_indices: HashMap<TCPackageKey, OpIndex>,
    blank_ident: IdentKey,
}

impl<'a> EntryGen<'a> {
    pub fn new(asto: &'a AstObjects, tco: &'a TCObjects, bk: IdentKey) -> EntryGen<'a> {
        EntryGen {
            objects: Box::pin(VMObjects::new()),
            ast_objs: asto,
            tc_objs: tco,
            dummy_gcv: GcoVec::new(),
            packages: Vec::new(),
            iface_mapping: IfaceMapping::new(),
            pkg_indices: HashMap::new(),
            blank_ident: bk,
        }
    }

    // generate the entry function for ByteCode
    fn gen_entry_func(
        &mut self,
        pkg: PackageKey,
        index: OpIndex,
        main_ident: IdentKey,
        pairs: &mut PkgVarPairs,
    ) -> FunctionKey {
        // import the 0th pkg and call the main function of the pkg
        let fmeta = self.objects.metadata.default_sig;
        let f = GosValue::new_function(
            null_key!(),
            fmeta.clone(),
            &mut self.objects,
            &self.dummy_gcv,
            false,
        );
        let fkey = *f.as_function();
        let func = &mut self.objects.functions[fkey];
        let mut emitter = Emitter::new(func);
        emitter.emit_import(index, pkg, None);
        emitter.emit_load(
            EntIndex::PackageMember(pkg, main_ident),
            Some((pairs, fkey)),
            ValueType::Function,
            None,
        );
        emitter.emit_pre_call(None);
        emitter.emit_call(false, false, None);
        emitter.emit_return(None);
        *f.as_function()
    }

    pub fn gen(
        mut self,
        checker_result: &HashMap<TCPackageKey, TypeInfo>,
        main_pkg: TCPackageKey,
        main_ident: IdentKey,
    ) -> ByteCode {
        let mut main_pkg_idx = None;
        for (&tcpkg, _) in checker_result.iter() {
            // create vm packages and store the indices
            let name = self.tc_objs.pkgs[tcpkg].name().clone().unwrap();
            let pkey = self.objects.packages.insert(PackageVal::new(name));
            self.packages.push(pkey);
            let index = (self.packages.len() - 1) as OpIndex;
            self.pkg_indices.insert(tcpkg, index);
            if tcpkg == main_pkg {
                main_pkg_idx = Some(index);
            }
        }
        let mut type_cache: TypeCache = HashMap::new();
        let mut pairs = PkgVarPairs::new();
        for (i, (tcpkg, ti)) in checker_result.iter().enumerate() {
            let mut cgen = CodeGen::new(
                &mut self.objects,
                self.ast_objs,
                self.tc_objs,
                &mut self.dummy_gcv,
                &ti,
                &mut type_cache,
                &mut self.iface_mapping,
                &self.pkg_indices,
                &self.packages,
                self.packages[i],
                self.blank_ident,
            );
            cgen.gen_with_files(&ti.ast_files, *tcpkg, i as OpIndex);
            pairs.append_from_util(cgen.pkg_util());
        }
        let index = main_pkg_idx.unwrap();
        let entry =
            self.gen_entry_func(self.packages[index as usize], index, main_ident, &mut pairs);
        pairs.patch_index(self.ast_objs, &mut self.objects);
        ByteCode {
            objects: self.objects,
            packages: self.packages,
            ifaces: self.iface_mapping.into_result(),
            entry: entry,
        }
    }
}

pub fn parse_check_gen(
    path: &str,
    config: &Config,
    fset: &mut FileSet,
    el: &ErrorList,
) -> Result<ByteCode, usize> {
    let asto = &mut AstObjects::new();
    let tco = &mut goscript_types::TCObjects::new();
    let results = &mut HashMap::new();
    let pkgs = &mut HashMap::new();

    let importer =
        &mut goscript_types::Importer::new(&config, fset, pkgs, results, asto, tco, el, 0);
    let key = goscript_types::ImportKey::new(path, "./");
    let main_pkg = importer.import(&key);

    if el.len() > 0 {
        Err(el.len())
    } else {
        let blank_ident = asto.idents.insert(Ident::blank(0));
        let main_ident = asto.idents.insert(Ident::with_str(0, "main"));
        let gen = EntryGen::new(asto, tco, blank_ident);
        Ok(gen.gen(results, main_pkg.unwrap(), main_ident))
    }
}
