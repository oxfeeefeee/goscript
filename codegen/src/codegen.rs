// Copyright 2022 The Goscript Authors. All rights reserved.
// Use of this source code is governed by a BSD-style
// license that can be found in the LICENSE file.

use slotmap::{Key, KeyData};
use std::convert::TryFrom;
use std::iter::FromIterator;

use super::branch::*;
use super::consts::*;
use super::package::PkgHelper;
use super::selector::*;
use super::types::{SelectionType, TypeCache, TypeLookup};
use crate::context::*;

use goscript_vm::gc::GcoVec;
use goscript_vm::instruction::*;
use goscript_vm::metadata::*;
use goscript_vm::value::*;

use goscript_parser::ast::*;
use goscript_parser::objects::Objects as AstObjects;
use goscript_parser::objects::*;
use goscript_parser::position::Pos;
use goscript_parser::token::Token;
use goscript_parser::visitor::{walk_decl, walk_expr, walk_stmt, ExprVisitor, StmtVisitor};
use goscript_types::{
    identical_ignore_tags, Builtin, ObjKey as TCObjKey, OperandMode, PackageKey as TCPackageKey,
    TCObjects, Type, TypeInfo, TypeKey as TCTypeKey,
};

macro_rules! current_fctx {
    ($gen:ident) => {
        $gen.func_ctx_stack.last_mut().unwrap()
    };
}

/// CodeGen implements the code generation logic.
pub struct CodeGen<'a, 'c> {
    objects: &'a mut VMObjects,
    consts: &'c Consts,
    ast_objs: &'a AstObjects,
    tc_objs: &'a TCObjects,
    dummy_gcv: &'a mut GcoVec,
    t: TypeLookup<'a>,
    iface_selector: &'a mut IfaceSelector,
    struct_selector: &'a mut StructSelector,
    branch_helper: &'a mut BranchHelper,
    pkg_helper: &'a mut PkgHelper<'a>,

    pkg_key: PackageKey,
    blank_ident: IdentKey,
    func_ctx_stack: Vec<FuncCtx<'c>>,
    expr_ctx_stack: Vec<ExprCtx>,
    results: Vec<FuncCtx<'c>>,
}

impl<'a, 'c> CodeGen<'a, 'c> {
    pub fn new(
        objects: &'a mut VMObjects,
        consts: &'c Consts,
        ast_objs: &'a AstObjects,
        tc_objs: &'a TCObjects,
        dummy_gcv: &'a mut GcoVec,
        ti: &'a TypeInfo,
        type_cache: &'a mut TypeCache,
        iface_selector: &'a mut IfaceSelector,
        struct_selector: &'a mut StructSelector,
        branch_helper: &'a mut BranchHelper,
        pkg_helper: &'a mut PkgHelper<'a>,
        pkg_key: PackageKey,
        blank_ident: IdentKey,
    ) -> CodeGen<'a, 'c> {
        CodeGen {
            objects,
            consts,
            ast_objs,
            tc_objs,
            dummy_gcv,
            t: TypeLookup::new(tc_objs, ti, type_cache),
            iface_selector,
            struct_selector,
            branch_helper,
            pkg_helper,
            pkg_key,
            blank_ident,
            func_ctx_stack: vec![],
            expr_ctx_stack: vec![],
            results: vec![],
        }
    }

    pub fn pkg_helper(&mut self) -> &mut PkgHelper<'a> {
        &mut self.pkg_helper
    }

    pub fn gen_with_files(mut self, files: &Vec<File>, tcpkg: TCPackageKey) -> Vec<FuncCtx<'c>> {
        let pkey = self.pkg_key;
        let fmeta = self.objects.s_meta.default_sig;
        let f = GosValue::function_with_meta(
            pkey,
            fmeta,
            self.objects,
            self.dummy_gcv,
            FuncFlag::PkgCtor,
        );
        let fkey = *f.as_function();
        // the 0th member is the constructor
        self.objects.packages[pkey].add_member(
            String::new(),
            GosValue::new_closure_static(fkey, &self.objects.functions),
            ValueType::Closure,
        );
        self.pkg_key = pkey;
        self.func_ctx_stack
            .push(FuncCtx::new(fkey, None, self.consts));

        // let (names, vars) = self.pkg_helper.sort_var_decls(files, self.t.type_info());
        // self.add_pkg_var_member(pkey, &names);

        // self.pkg_helper
        //     .gen_imports(tcpkg, &mut current_func_emitter!(self));

        // for f in files.iter() {
        //     for d in f.decls.iter() {
        //         self.visit_decl(d)
        //     }
        // }
        // for v in vars.iter() {
        //     self.gen_def_var(v);
        // }

        current_fctx!(self).emit_return(Some(self.pkg_key), None, &self.objects.functions);
        self.results.push(self.func_ctx_stack.pop().unwrap());
        self.results
    }
}
