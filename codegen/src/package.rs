#![allow(dead_code)]

use super::func::FuncGen;
use goscript_parser::ast::*;
use goscript_parser::objects::Objects as AstObjects;
use goscript_parser::objects::*;
use goscript_parser::token::Token;
use goscript_types::{PackageKey as TCPackageKey, TCObjects, TypeInfo};
use goscript_vm::instruction::*;
use goscript_vm::value::*;
use std::collections::HashMap;
use std::rc::Rc;

pub struct PkgVarPairs {
    data: Vec<(PackageKey, IdentKey, FunctionKey, usize)>,
}

impl PkgVarPairs {
    pub fn new() -> PkgVarPairs {
        PkgVarPairs { data: vec![] }
    }

    pub fn append_from_util(&mut self, util: &mut PkgUtil) {
        self.data.append(&mut util.pairs.data);
    }

    /// patch_index sets correct var index of packages to the placeholder of
    /// previously generated code
    pub fn patch_index(&self, ast_objs: &AstObjects, vmo: &mut VMObjects) {
        for (pkg, var, func, i) in self.data.iter() {
            let pkg_val = &vmo.packages[*pkg];
            let id = &ast_objs.idents[*var];
            let index = pkg_val.get_member_index(&id.name).unwrap();
            vmo.functions[*func].code[*i].set_imm(*index);
        }
    }
}

pub struct PkgUtil<'a> {
    tc_objs: &'a TCObjects,
    ast_objs: &'a AstObjects,
    pkg_indices: &'a HashMap<TCPackageKey, OpIndex>,
    pkgs: &'a Vec<PackageKey>,
    pkg: PackageKey,
    pairs: PkgVarPairs,
}

impl<'a> PkgUtil<'a> {
    pub fn new(
        ast_objs: &'a AstObjects,
        tc_objs: &'a TCObjects,
        pkg_indices: &'a HashMap<TCPackageKey, OpIndex>,
        pkgs: &'a Vec<PackageKey>,
        pkg: PackageKey,
    ) -> PkgUtil<'a> {
        PkgUtil {
            tc_objs: tc_objs,
            ast_objs: ast_objs,
            pkg_indices: pkg_indices,
            pkgs: pkgs,
            pkg: pkg,
            pairs: PkgVarPairs::new(),
        }
    }

    pub fn gen_imports(&mut self, tcpkg: TCPackageKey, func: &mut FunctionVal) {
        let pkg = &self.tc_objs.pkgs[tcpkg];
        for key in pkg.imports().iter() {
            let index = self.pkg_indices[key];
            func.emit_import(index);
        }
    }

    pub fn get_vm_pkg(&self, tcpkg: TCPackageKey) -> PackageKey {
        let index = self.pkg_indices[&tcpkg];
        self.pkgs[index as usize]
    }

    /// recode information so that we can patch_index, when codegen is done
    pub fn add_pair(&mut self, pkg: PackageKey, var: IdentKey, func: FunctionKey, i: usize) {
        self.pairs.data.push((pkg, var, func, i));
    }

    // sort_var_decls returns a vec of sorted var decl statments
    pub fn sort_var_decls(&self, files: &Vec<File>, ti: &TypeInfo) -> Vec<Rc<ValueSpec>> {
        let mut orders = HashMap::new();
        for (i, init) in ti.init_order.iter().enumerate() {
            for okey in init.lhs.iter() {
                let name = self.tc_objs.lobjs[*okey].name();
                orders.insert(name, i);
            }
        }

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
                                        let name = &self.ast_objs.idents[v.names[0]].name;
                                        let order = orders[name];
                                        decls.push((v.clone(), order));
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
        decls.into_iter().map(|x| x.0).collect()
    }
}
