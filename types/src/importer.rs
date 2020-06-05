#![allow(dead_code)]
use super::objects::{DeclKey, ObjKey, PackageKey, ScopeKey, TCObjects, TypeKey};
use goscript_parser::errors::{ErrorList, FilePosErrors};
use goscript_parser::FileSet;
use std::collections::HashMap;

/// ImportKey identifies an imported package by import path and source directory
/// (directory containing the file containing the import). In practice, the directory
/// may always be the same, or may not matter. Given an (import path, directory), an
/// importer must always return the same package (but given two different import paths,
/// an importer may still return the same package by mapping them to the same package
/// paths).
#[derive(PartialEq, Eq, Hash)]
pub struct ImportKey {
    pub path: String,
    pub dir: String,
}

pub struct Importer<'a> {
    base_path: Option<&'a String>,
    fset: &'a mut FileSet,
    pkgs: &'a mut HashMap<String, PackageKey>,
    objs: &'a mut TCObjects,
    errors: &'a ErrorList,
}

impl<'a> Importer<'a> {
    pub fn new(
        base: Option<&'a String>,
        fset: &'a mut FileSet,
        pkgs: &'a mut HashMap<String, PackageKey>,
        objs: &'a mut TCObjects,
        errors: &'a ErrorList,
    ) -> Importer<'a> {
        Importer {
            base_path: base,
            fset: fset,
            pkgs: pkgs,
            objs: objs,
            errors: errors,
        }
    }

    pub fn import(&mut self, key: &ImportKey) -> Result<ObjKey, ()> {
        unimplemented!()
    }
}
