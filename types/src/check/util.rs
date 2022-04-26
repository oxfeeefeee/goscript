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

use super::super::display::{Display, Displayer};
use super::super::obj;
use super::super::objects::{DeclInfoKey, ObjKey, PackageKey, ScopeKey, TCObjects, TypeKey};
use super::super::operand::{Operand, OperandMode};
use super::super::package::Package;
use super::super::scope::Scope;
use super::super::typ::{self, BasicType, Type};
use super::super::universe::{Builtin, BuiltinInfo};
use super::check::{Checker, FilesContext};
use super::resolver::DeclInfo;
use std::cmp::Ordering;

use goscript_parser::objects::IdentKey;
use goscript_parser::{ast, ast::Expr, position::Pos, position::Position};
use std::collections::HashMap;

macro_rules! error_operand {
    ($x:ident, $fmt:expr, $checker:ident) => {
        let xd = $checker.new_dis(&$x);
        $checker.error(xd.pos(), format!($fmt, xd));
    };
}

#[derive(Debug)]
pub enum UnpackResult<'a> {
    Tuple(Option<Expr>, Vec<Option<TypeKey>>, Ordering), // rhs is a tuple
    CommaOk(Option<Expr>, [TypeKey; 2]),                 // rhs returns comma_ok
    Mutliple(&'a Vec<Expr>, Ordering),                   // N to N
    Single(Operand, Ordering),                           // 1 to 1
    Nothing(Ordering),                                   // nothing to unpack
    Error,                                               // errors when trying to unpack
}

impl<'a> UnpackResult<'a> {
    pub fn get(&self, checker: &mut Checker, x: &mut Operand, i: usize, fctx: &mut FilesContext) {
        match self {
            UnpackResult::Tuple(expr, types, _) => {
                x.mode = OperandMode::Value;
                x.expr = expr.clone();
                x.typ = types[i];
            }
            UnpackResult::CommaOk(expr, types) => {
                x.mode = OperandMode::Value;
                x.expr = expr.clone();
                x.typ = Some(types[i]);
            }
            UnpackResult::Mutliple(exprs, _) => {
                checker.multi_expr(x, &exprs[i], fctx);
            }
            UnpackResult::Single(sx, _) => {
                x.mode = sx.mode.clone();
                x.expr = sx.expr.clone();
                x.typ = sx.typ;
            }
            UnpackResult::Nothing(_) => unreachable!(),
            UnpackResult::Error => unreachable!(),
        }
    }

    // rhs_count return the var count of rhs, and if it less than or equal to or matchs
    // the required count
    pub fn rhs_count(&self) -> (usize, Ordering) {
        match self {
            UnpackResult::Tuple(_, types, ord) => (types.len(), *ord),
            UnpackResult::CommaOk(_, types) => (types.len(), Ordering::Equal),
            UnpackResult::Mutliple(exprs, ord) => (exprs.len(), *ord),
            UnpackResult::Single(_, ord) => (1, *ord),
            UnpackResult::Nothing(ord) => (0, *ord),
            UnpackResult::Error => unreachable!(),
        }
    }

    pub fn use_(&self, checker: &mut Checker, from: usize, fctx: &mut FilesContext) {
        let exprs = match self {
            UnpackResult::Mutliple(exprs, _) => exprs,
            _ => {
                return;
            }
        };

        let mut x = Operand::new();
        for i in from..exprs.len() {
            checker.multi_expr(&mut x, &exprs[i], fctx);
        }
    }

    pub fn is_err(&self) -> bool {
        match self {
            UnpackResult::Error => true,
            _ => false,
        }
    }
}

#[derive(Debug)]
pub struct UnpackedResultLeftovers<'a> {
    pub leftovers: &'a UnpackResult<'a>,
    pub consumed: Option<&'a Vec<Operand>>,
}

impl<'a> UnpackedResultLeftovers<'a> {
    pub fn new(
        re: &'a UnpackResult<'a>,
        consumed: Option<&'a Vec<Operand>>,
    ) -> UnpackedResultLeftovers<'a> {
        UnpackedResultLeftovers {
            leftovers: re,
            consumed: consumed,
        }
    }

    pub fn use_all(&self, checker: &mut Checker, fctx: &mut FilesContext) {
        let from = if self.consumed.is_none() {
            0
        } else {
            self.consumed.unwrap().len()
        };
        self.leftovers.use_(checker, from, fctx);
    }

    pub fn get(&self, checker: &mut Checker, x: &mut Operand, i: usize, fctx: &mut FilesContext) {
        if self.consumed.is_none() {
            self.leftovers.get(checker, x, i, fctx);
            return;
        }
        let consumed = self.consumed.unwrap();
        if i < consumed.len() {
            let c = &consumed[i];
            x.mode = c.mode.clone();
            x.expr = c.expr.clone();
            x.typ = c.typ;
        } else {
            self.leftovers.get(checker, x, i, fctx);
        }
    }
}

impl<'a> Checker<'a> {
    pub fn unparen(x: &Expr) -> &Expr {
        if let Expr::Paren(p) = x {
            Checker::unparen(&p.expr)
        } else {
            x
        }
    }

    /// invalid_ast helps to report ast error
    pub fn invalid_ast(&self, pos: Pos, err: &str) {
        self.error(pos, format!("invalid AST: {}", err));
    }

    pub fn invalid_arg(&self, pos: Pos, err: &str) {
        self.error(pos, format!("invalid argument: {}", err));
    }

    pub fn invalid_op(&self, pos: Pos, err: &str) {
        self.error(pos, format!("invalid operation: {}", err));
    }

    pub fn obj_path_str(&self, path: &Vec<ObjKey>) -> String {
        let names: Vec<&str> = path.iter().map(|p| self.lobj(*p).name().as_str()).collect();
        names[..].join("->")
    }

    pub fn dump(&self, pos: Option<Pos>, msg: &str) {
        if let Some(p) = pos {
            let p = self.fset.position(p);
            print!("checker dump({}):{}\n", p, msg);
        } else {
            print!("checker dump:{}\n", msg);
        }
    }

    pub fn print_trace(&self, pos: Pos, msg: &str) {
        let p = self.fset.position(pos);
        print!("{}:\t{}{}\n", p, ".  ".repeat(*self.indent.borrow()), msg);
    }

    pub fn trace_begin(&self, pos: Pos, msg: &str) {
        self.print_trace(pos, msg);
        *self.indent.borrow_mut() += 1;
    }

    pub fn trace_end(&self, pos: Pos, msg: &str) {
        *self.indent.borrow_mut() -= 1;
        self.print_trace(pos, msg);
    }

    // has_cycle reports whether obj appears in path or not.
    // If it does, and report is set, it also reports a cycle error.
    pub fn has_cycle(&self, okey: ObjKey, path: &[ObjKey], report: bool) -> bool {
        if let Some((i, _)) = path.iter().enumerate().find(|(_, &x)| x == okey) {
            if report {
                let obj_val = self.lobj(okey);
                self.error(
                    obj_val.pos(),
                    format!("illegal cycle in declaration of {}", obj_val.name()),
                );
                // print cycle
                for o in path[i..].iter() {
                    let oval = self.lobj(*o);
                    self.error(oval.pos(), format!("\t{} refers to", oval.name()));
                }
                self.error(obj_val.pos(), format!("\t{}", obj_val.name()));
            }
            return true;
        }
        false
    }

    pub fn comma_ok_type(
        tc_objs: &mut TCObjects,
        pos: usize,
        pkg: PackageKey,
        t: &[TypeKey; 2],
    ) -> TypeKey {
        let vars = vec![
            tc_objs.lobjs.insert(obj::LangObj::new_var(
                pos,
                Some(pkg),
                String::new(),
                Some(t[0]),
            )),
            tc_objs.lobjs.insert(obj::LangObj::new_var(
                pos,
                Some(pkg),
                String::new(),
                Some(t[1]),
            )),
        ];
        tc_objs.new_t_tuple(vars)
    }

    pub fn unpack<'b>(
        &mut self,
        rhs: &'b Vec<Expr>,
        lhs_len: usize,
        allow_comma_ok: bool,
        variadic: bool,
        fctx: &mut FilesContext,
    ) -> UnpackResult<'b> {
        let do_match = |rhs_len: usize| {
            let order = rhs_len.cmp(&lhs_len);
            if variadic && order == Ordering::Greater {
                Ordering::Equal
            } else {
                order
            }
        };
        if rhs.len() != 1 {
            let matching = do_match(rhs.len());
            return if rhs.len() == 0 {
                UnpackResult::Nothing(matching)
            } else {
                UnpackResult::Mutliple(rhs, matching)
            };
        }

        let mut x = Operand::new();
        self.multi_expr(&mut x, &rhs[0], fctx);
        if x.invalid() {
            return UnpackResult::Error;
        }

        if let Some(t) = self.otype(x.typ.unwrap()).try_as_tuple() {
            let types: Vec<Option<TypeKey>> =
                t.vars().iter().map(|x| self.lobj(*x).typ()).collect();
            let matching = do_match(types.len());
            return UnpackResult::Tuple(x.expr.clone(), types, matching);
        } else if x.mode == OperandMode::MapIndex || x.mode == OperandMode::CommaOk {
            if allow_comma_ok {
                let types = [x.typ.unwrap(), self.basic_type(BasicType::UntypedBool)];
                return UnpackResult::CommaOk(x.expr.clone(), types);
            }
            x.mode = OperandMode::Value;
        }

        UnpackResult::Single(x, do_match(1))
    }

    pub fn use_exprs(&mut self, exprs: &Vec<Expr>, fctx: &mut FilesContext) {
        let x = &mut Operand::new();
        for e in exprs.iter() {
            self.raw_expr(x, &e, None, fctx);
        }
    }

    // use_lhs is like use_exprs, but doesn't "use" top-level identifiers.
    // It should be called instead of use_exprs if the arguments are
    // expressions on the lhs of an assignment.
    // The arguments must not be nil.
    pub fn use_lhs(&mut self, lhs: &Vec<Expr>, fctx: &mut FilesContext) {
        let x = &mut Operand::new();
        for e in lhs.iter() {
            let v = match Checker::unparen(e) {
                Expr::Ident(ikey) => match &self.ast_ident(*ikey).name {
                    s if s == "_" => continue,
                    s => Scope::lookup_parent(
                        self.octx.scope.as_ref().unwrap(),
                        s,
                        None,
                        self.tc_objs,
                    )
                    .map(|(_, okey)| okey)
                    .map(|okey| {
                        let lobj = self.lobj(okey);
                        match lobj.entity_type() {
                            obj::EntityType::Var(vp) => match lobj.pkg() == Some(self.pkg) {
                                true => Some((okey, vp.used)),
                                false => None,
                            },
                            _ => None,
                        }
                    }),
                },
                _ => None,
            }
            .flatten();

            self.raw_expr(x, &e, None, fctx);

            if let Some((okey, used)) = v {
                match self.lobj_mut(okey).entity_type_mut() {
                    obj::EntityType::Var(vp) => vp.used = used,
                    _ => unreachable!(),
                }
            }
        }
    }

    pub fn lookup(&self, name: &str) -> Option<ObjKey> {
        Scope::lookup_parent(
            self.octx.scope.as_ref().unwrap(),
            name,
            self.octx.pos,
            self.tc_objs,
        )
        .map(|(_, okey)| okey)
    }

    pub fn add_decl_dep(&mut self, to: ObjKey) {
        if self.octx.decl.is_none() {
            // not in a package-level init expression
            return;
        }
        if !self.obj_map.contains_key(&to) {
            return;
        }
        self.tc_objs.decls[self.octx.decl.unwrap()].add_dep(to);
    }

    pub fn insert_obj_to_set(
        &self,
        set: &mut HashMap<String, ObjKey>,
        okey: ObjKey,
    ) -> Option<ObjKey> {
        let obj_val = self.lobj(okey);
        let id = obj_val.id(self.tc_objs).to_string();
        set.insert(id, okey)
    }

    pub fn ast_ident(&self, key: IdentKey) -> &ast::Ident {
        &self.ast_objs.idents[key]
    }

    pub fn lobj(&self, key: ObjKey) -> &obj::LangObj {
        &self.tc_objs.lobjs[key]
    }

    pub fn lobj_mut(&mut self, key: ObjKey) -> &mut obj::LangObj {
        &mut self.tc_objs.lobjs[key]
    }

    pub fn otype(&self, key: TypeKey) -> &Type {
        &self.tc_objs.types[key]
    }

    pub fn otype_mut(&mut self, key: TypeKey) -> &mut Type {
        &mut self.tc_objs.types[key]
    }

    pub fn otype_interface(&self, key: TypeKey) -> &typ::InterfaceDetail {
        self.otype(key).try_as_interface().unwrap()
    }

    pub fn otype_signature(&self, key: TypeKey) -> &typ::SignatureDetail {
        self.otype(key).try_as_signature().unwrap()
    }

    pub fn otype_interface_mut(&mut self, key: TypeKey) -> &mut typ::InterfaceDetail {
        self.otype_mut(key).try_as_interface_mut().unwrap()
    }

    pub fn otype_signature_mut(&mut self, key: TypeKey) -> &mut typ::SignatureDetail {
        self.otype_mut(key).try_as_signature_mut().unwrap()
    }

    pub fn package(&self, key: PackageKey) -> &Package {
        &self.tc_objs.pkgs[key]
    }

    pub fn package_mut(&mut self, key: PackageKey) -> &mut Package {
        &mut self.tc_objs.pkgs[key]
    }

    pub fn scope(&self, key: ScopeKey) -> &Scope {
        &self.tc_objs.scopes[key]
    }

    pub fn decl_info(&self, key: DeclInfoKey) -> &DeclInfo {
        &self.tc_objs.decls[key]
    }

    pub fn position(&self, pos: Pos) -> Position {
        self.fset.file(pos).unwrap().position(pos)
    }

    pub fn builtin_info(&self, id: Builtin) -> &BuiltinInfo {
        &self.tc_objs.universe().builtins()[&id]
    }

    pub fn basic_type(&self, t: typ::BasicType) -> TypeKey {
        self.tc_objs.universe().types()[&t]
    }

    pub fn invalid_type(&self) -> TypeKey {
        self.basic_type(typ::BasicType::Invalid)
    }

    pub fn new_dis<'b>(&'b self, x: &'b impl Display) -> Displayer<'b> {
        Displayer::new(x, Some(self.ast_objs), Some(self.tc_objs))
    }

    pub fn new_td_o<'t>(&'t self, t: &'t Option<TypeKey>) -> Displayer<'t> {
        Displayer::new(t.as_ref().unwrap(), Some(self.ast_objs), Some(self.tc_objs))
    }
}
