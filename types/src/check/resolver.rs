#![allow(dead_code)]
use super::super::display::ExprDisplay;
use super::super::objects::{DeclKey, ObjKey, PackageKey, ScopeKey, TCObjects, TypeKey};
use super::check::{Checker, FilesContext};
use goscript_parser::ast;
use goscript_parser::ast::Node;
use goscript_parser::objects::FuncDeclKey;
use std::collections::HashSet;

/// DeclInfo describes a package-level const, type, var, or func declaration.
pub struct DeclInfo {
    file_scope: ScopeKey,
    lhs: Vec<ObjKey>,
    typ: Option<ast::Expr>,
    init: Option<ast::Expr>,
    fdecl: FuncDeclKey,
    alias: bool,
    deps: HashSet<ObjKey>,
}

impl DeclInfo {
    pub fn has_initializer(&self) -> bool {
        self.init.is_some()
    }

    pub fn add_dep(&mut self, obj: ObjKey) {
        self.deps.insert(obj);
    }
}

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
}
