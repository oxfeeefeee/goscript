#![allow(dead_code)]
use super::check::check::Checker;
use super::objects::{PackageKey, TCObjects};
use goscript_parser::ast;
use goscript_parser::errors::{ErrorList, FilePosErrors};
use goscript_parser::objects::Objects as AstObjects;
use goscript_parser::position;
use goscript_parser::{FileSet, Parser};
use std::collections::HashMap;
use std::env;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

pub struct Config {
    // working directory
    pub work_dir: Option<String>,
    // base path for non-local imports
    pub base_path: Option<String>,
    // print debug info in parser
    pub trace_parser: bool,
    // print debug info in parser
    pub trace_checker: bool,
}

impl Config {
    fn get_working_dir(&self) -> io::Result<PathBuf> {
        if let Some(wd) = &self.work_dir {
            let mut buf = PathBuf::new();
            buf.push(wd);
            Ok(buf)
        } else {
            env::current_dir()
        }
    }
}

/// ImportKey identifies an imported package by import path and source directory
/// (directory containing the file containing the import). In practice, the directory
/// may always be the same, or may not matter. Given an (import path, directory), an
/// importer must always return the same package (but given two different import paths,
/// an importer may still return the same package by mapping them to the same package
/// paths).
#[derive(PartialEq, Eq, Hash, Debug)]
pub struct ImportKey {
    pub path: String,
    pub dir: String,
}

impl ImportKey {
    pub fn new(path: &str, dir: &str) -> ImportKey {
        ImportKey {
            path: path.to_string(),
            dir: dir.to_string(),
        }
    }
}

pub struct Importer<'a> {
    config: &'a Config,
    fset: &'a mut FileSet,
    pkgs: &'a mut HashMap<String, PackageKey>,
    ast_objs: &'a mut AstObjects,
    tc_objs: &'a mut TCObjects,
    errors: &'a ErrorList,
    pos: position::Pos,
}

impl<'a> Importer<'a> {
    pub fn new(
        config: &'a Config,
        fset: &'a mut FileSet,
        pkgs: &'a mut HashMap<String, PackageKey>,
        ast_objs: &'a mut AstObjects,
        tc_objs: &'a mut TCObjects,
        errors: &'a ErrorList,
        pos: position::Pos,
    ) -> Importer<'a> {
        Importer {
            config: config,
            fset: fset,
            pkgs: pkgs,
            ast_objs: ast_objs,
            tc_objs: tc_objs,
            errors: errors,
            pos: pos,
        }
    }

    pub fn import(&mut self, key: &'a ImportKey) -> Result<PackageKey, ()> {
        if key.path == "unsafe" {
            return Ok(*self.tc_objs.universe().unsafe_pkg());
        }
        let pb = self.validate_path(key)?;
        let path = pb.0.as_path();
        let import_path = pb.1;
        let pkg = self.tc_objs.new_package(import_path.clone());
        self.pkgs.insert(import_path, pkg);
        let files = self.parse_dir(path)?;
        Checker::new(
            self.tc_objs,
            self.ast_objs,
            self.fset,
            self.errors,
            self.pkgs,
            pkg,
            self.config,
        )
        .check(files)
    }

    fn validate_path(&mut self, key: &'a ImportKey) -> Result<(PathBuf, String), ()> {
        let mut import_path = key.path.clone();
        let path = if is_local(&key.path) {
            let working_dir = self.config.get_working_dir();
            if working_dir.is_err() {
                self.error(format!("failed to get working dir for: {}", key.path));
                return Err(());
            }
            let mut wd = working_dir.unwrap();
            wd.push(&key.dir);
            wd.push(&key.path);
            if let Some(base) = &self.config.base_path {
                if let Ok(rel) = wd.as_path().strip_prefix(base) {
                    import_path = rel.to_string_lossy().to_string()
                }
            }
            wd
        } else {
            if let Some(base) = &self.config.base_path {
                let mut p = PathBuf::new();
                p.push(base);
                p.push(&key.path);
                p
            } else {
                self.error(format!("base dir required for path: {}", key.path));
                return Err(());
            }
        };
        if !path.exists() {
            self.error(format!("failed to locate path: {}", key.path));
            return Err(());
        }
        Ok((path, import_path))
    }

    fn parse_dir(&mut self, path: &Path) -> Result<Vec<ast::File>, ()> {
        match read_content(path) {
            Ok(contents) => {
                if contents.len() == 0 {
                    self.error(format!("no source file found in dir: {}", path.display()));
                    Err(())
                } else {
                    let mut afiles = vec![];
                    for (name, content) in contents.iter() {
                        let mut pfile = self.fset.add_file(
                            name,
                            Some(self.fset.base()),
                            content.chars().count(),
                        );
                        let afile = Parser::new(
                            self.ast_objs,
                            &mut pfile,
                            self.errors,
                            content,
                            self.config.trace_parser,
                        )
                        .parse_file();
                        if afile.is_none() {
                            // parse error, the details should be in the errorlist already.
                            // give up
                            return Err(());
                        } else {
                            afiles.push(afile.unwrap());
                        }
                    }
                    Ok(afiles)
                }
            }
            Err(_) => {
                self.error(format!("failed to read dir: {}", path.display()));
                Err(())
            }
        }
    }

    fn error(&self, err: String) {
        let pos_file = self.fset.file(self.pos).unwrap();
        FilePosErrors::new(pos_file, self.errors).add(self.pos, err, false);
    }
}

fn read_content(path: &Path) -> io::Result<Vec<(String, String)>> {
    let mut result = vec![];
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        let path = entry.path();
        if !path.is_dir() {
            if let Some(ext) = path.extension() {
                if ext == "gos" || ext == "go" {
                    result.push((
                        path.as_path().to_string_lossy().to_string(),
                        fs::read_to_string(path)?,
                    ))
                }
            }
        }
    }
    Ok(result)
}

fn is_local(path: &str) -> bool {
    path == "." || path == ".." || path.starts_with("./") || path.starts_with("../")
}
