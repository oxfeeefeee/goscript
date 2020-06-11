#![allow(dead_code)]

use super::super::obj::LangObj;
use super::super::objects::{DeclInfoKey, ObjKey, PackageKey, ScopeKey, TCObjects, TypeKey};
use super::super::scope::Scope;
use super::check::{Checker, TypeInfo};
use goscript_parser::objects::IdentKey;
use goscript_parser::position::Pos;

impl<'a> Checker<'a> {
    fn report_alt_decl(&self, okey: &ObjKey) {
        let lobj = self.lobj(*okey);
        let pos = *lobj.pos();
        if pos > 0 {
            self.error(pos, format!("\tother declaration of {}", lobj.name()));
        }
    }

    pub fn declare(&mut self, skey: ScopeKey, ikey: Option<IdentKey>, okey: ObjKey, pos: Pos) {
        // spec: "The blank identifier, represented by the underscore
        // character _, may be used in a declaration like any other
        // identifier but the declaration does not introduce a new
        // binding."
        if self.lobj(okey).name() != "_" {
            let alt = Scope::insert(skey, okey, self.tc_objs).map(|x| x.clone());
            if let Some(o) = alt {
                let lobj = self.lobj(okey);
                self.error(
                    *lobj.pos(),
                    format!("{} redeclared in this block", lobj.name()),
                );
                self.report_alt_decl(&o);
                return;
            }
            self.lobj_mut(okey).set_scope_pos(pos);
        }
        if ikey.is_some() {
            self.result.record_def(ikey.unwrap(), okey);
        }
    }
}
