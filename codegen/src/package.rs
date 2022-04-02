use super::emit::Emitter;
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
    data: Vec<(PackageKey, IdentKey, FunctionKey, usize, bool)>,
}

impl PkgVarPairs {
    pub fn new() -> PkgVarPairs {
        PkgVarPairs { data: vec![] }
    }

    /// recode information so that we can patch_index, when codegen is done
    pub fn add_pair(
        &mut self,
        pkg: PackageKey,
        var: IdentKey,
        func: FunctionKey,
        i: usize,
        is824: bool,
    ) {
        self.data.push((pkg, var, func, i, is824));
    }

    pub fn append(&mut self, other: &mut PkgVarPairs) {
        self.data.append(&mut other.data);
    }

    /// patch_index sets correct var index of packages to the placeholder of
    /// previously generated code
    pub fn patch_index(&self, ast_objs: &AstObjects, vmo: &mut VMObjects) {
        for (pkg, var, func, i, is_8_24) in self.data.iter() {
            let pkg_val = &vmo.packages[*pkg];
            let id = &ast_objs.idents[*var];
            let index = pkg_val.get_member_index(&id.name).unwrap();
            if *is_8_24 {
                let (imm0, _) = vmo.functions[*func].code()[*i].imm824();
                vmo.functions[*func]
                    .instruction_mut(*i)
                    .set_imm824(imm0, *index);
            } else {
                vmo.functions[*func].instruction_mut(*i).set_imm(*index);
            }
        }
    }
}

pub struct PkgHelper<'a> {
    tc_objs: &'a TCObjects,
    ast_objs: &'a AstObjects,
    pkg_indices: &'a HashMap<TCPackageKey, OpIndex>,
    pkgs: &'a Vec<PackageKey>,
    pairs: &'a mut PkgVarPairs,
}

impl<'a> PkgHelper<'a> {
    pub fn new(
        ast_objs: &'a AstObjects,
        tc_objs: &'a TCObjects,
        pkg_indices: &'a HashMap<TCPackageKey, OpIndex>,
        pkgs: &'a Vec<PackageKey>,
        pkg_pairs: &'a mut PkgVarPairs,
    ) -> PkgHelper<'a> {
        PkgHelper {
            tc_objs: tc_objs,
            ast_objs: ast_objs,
            pkg_indices: pkg_indices,
            pkgs: pkgs,
            pairs: pkg_pairs,
        }
    }

    pub fn pairs_mut(&mut self) -> &mut PkgVarPairs {
        &mut self.pairs
    }

    pub fn gen_imports(&mut self, tcpkg: TCPackageKey, func: &mut FunctionVal) {
        let pkg = &self.tc_objs.pkgs[tcpkg];
        let unsafe_ = self.tc_objs.universe().unsafe_pkg();
        for key in pkg.imports().iter() {
            if key != unsafe_ {
                let index = self.pkg_indices[key];
                Emitter::new(func).emit_import(index, self.pkgs[index as usize], None);
            }
        }
    }

    pub fn get_vm_pkg_key(&self, tcpkg: TCPackageKey) -> PackageKey {
        let index = self.pkg_indices[&tcpkg];
        self.pkgs[index as usize]
    }

    /// recode information so that we can patch_index, when codegen is done
    pub fn add_pair(
        &mut self,
        pkg: PackageKey,
        var: IdentKey,
        func: FunctionKey,
        i: usize,
        is824: bool,
    ) {
        self.pairs.add_pair(pkg, var, func, i, is824);
    }

    // sort_var_decls returns all var names and sorted var decl statments
    pub fn sort_var_decls(
        &self,
        files: &Vec<File>,
        ti: &TypeInfo,
    ) -> (Vec<IdentKey>, Vec<Rc<ValueSpec>>) {
        let mut orders = HashMap::new();
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
