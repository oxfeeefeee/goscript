#![allow(dead_code)]
use super::super::display::ExprDisplay;
use super::super::importer::{ImportKey, Importer};
use super::super::objects::{ObjKey, PackageKey, ScopeKey, TCObjects, TypeKey};
use super::super::package::DeclInfo;
use super::check::{Checker, FilesContext};
use super::decl;
use goscript_parser::ast;
use goscript_parser::ast::Node;
use goscript_parser::objects::FuncDeclKey;
use goscript_parser::objects::IdentKey;
use goscript_parser::position::Pos;

impl<'a> Checker<'a> {
    pub fn collect_objects(&mut self, fctx: &mut FilesContext) {}

    /// arity_match checks that the lhs and rhs of a const or var decl
    /// have the appropriate number of names and init exprs. For const
    /// decls, init is the value spec providing the init exprs; for
    /// var decls, init is nil (the init exprs are in s in this case).
    pub fn arity_match(&self, s: &ast::ValueSpec, init: Option<&ast::ValueSpec>) -> Result<(), ()> {
        let l = s.names.len();
        let mut r = s.values.len();
        if let Some(i) = init {
            r = i.values.len();
        }
        if init.is_none() || r == 0 {
            // var decl w/o init expr
            if s.typ.is_none() {
                self.error(
                    self.ident(s.names[0]).pos,
                    "missing type or init expr".to_string(),
                );
                return Err(());
            }
        } else if l < r {
            if init.is_none() {
                let expr = &s.values[l];
                self.error(
                    expr.pos(self.ast_objs()),
                    format!(
                        "extra init expr {}",
                        ExprDisplay::new(expr, self.ast_objs())
                    ),
                );
                return Err(());
            } else {
                let pos = self.ident(init.unwrap().names[0]).pos;
                self.error(
                    self.ident(s.names[0]).pos,
                    format!("extra init expr at {}", self.position(pos)),
                );
                return Err(());
            }
        } else if l > r && (init.is_some() || r != 1) {
            let ident = self.ident(s.names[r]);
            self.error(ident.pos, format!("missing init expr for {}", ident.name));
            return Err(());
        }
        Ok(())
    }

    fn valid_import_path(&self, path: &'a str, pos: Pos) -> Result<&'a str, ()> {
        if path.len() < 3 || (!path.starts_with('"') || !path.ends_with('"')) {
            self.error(pos, format!("invalid import path: {}", path));
            return Err(());
        }
        let result = &path[1..path.len() - 1];
        let mut illegal_chars: Vec<char> = r##"!"#$%&'()*,:;<=>?[\]^{|}`"##.chars().collect();
        illegal_chars.push('\u{FFFD}');
        if let Some(c) = illegal_chars
            .iter()
            .find(|&x| x.is_ascii_graphic() || x.is_whitespace() || result.contains(*x))
        {
            self.error(pos, format!("invalid character: {}", c));
            return Err(());
        }
        Ok(result)
    }

    /// declare_pkg_obj declares obj in the package scope, records its ident -> obj mapping,
    /// and updates check.objMap. The object must not be a function or method.
    fn declare_pkg_obj(&mut self, ikey: IdentKey, okey: ObjKey, d: DeclInfo) -> Result<(), ()> {
        let ident = self.ident(ikey);
        let lobj = self.lobj(okey);
        assert_eq!(&ident.name, lobj.name());
        // spec: "A package-scope or file-scope identifier with name init
        // may only be declared to be a function with this (func()) signature."
        if &ident.name == "init" {
            self.error(ident.pos, "cannot declare init - must be func".to_owned());
            return Err(());
        }
        // spec: "The main package must have package name main and declare
        // a function main that takes no arguments and returns no value."
        let pkg_name = self.pkg_val().name();
        if &ident.name == "main" && pkg_name.is_some() && pkg_name.as_ref().unwrap() == "main" {
            self.error(ident.pos, "cannot declare main - must be func".to_owned());
            return Err(());
        }
        let scope = *self.pkg_val().scope();
        self.declare(scope, Some(ikey), okey, 0);
        let dkey = self.tc_objs_mut().decls.insert(d);
        self.obj_map_mut().insert(okey, dkey);
        let order = self.obj_map().len() as u32;
        self.lobj_mut(okey).set_order(order);
        Ok(())
    }

    fn import_package(&mut self, pos: Pos, path: String, dir: String) -> PackageKey {
        // If we already have a package for the given (path, dir)
        // pair, use it instead of doing a full import.
        // Checker.imp_map only caches packages that are marked Complete
        // or fake (dummy packages for failed imports). Incomplete but
        // non-fake packages do require an import to complete them.
        let key = ImportKey::new(path.clone(), dir);
        if let Some(imp) = self.imp_map().get(&key) {
            return *imp;
        }

        let mut imported = self.new_importer(pos).import(&key);
        if imported.is_err() {
            self.error(pos, format!("could not import {}", &path));
            // create a new fake package
        }
        self.imp_map_mut().insert(key, imported.unwrap());
        imported.unwrap()
    }
}
