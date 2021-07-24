#![allow(dead_code)]
use super::check::check::{Checker, TypeInfo};
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
    // print debug info in checker
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
    pub dir: String, // makes a difference only when importing local files
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
    all_results: &'a mut HashMap<PackageKey, TypeInfo>,
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
        all_results: &'a mut HashMap<PackageKey, TypeInfo>,
        ast_objs: &'a mut AstObjects,
        tc_objs: &'a mut TCObjects,
        errors: &'a ErrorList,
        pos: position::Pos,
    ) -> Importer<'a> {
        Importer {
            config,
            fset,
            pkgs,
            all_results,
            ast_objs,
            tc_objs,
            errors,
            pos,
        }
    }

    pub fn import(&mut self, key: &'a ImportKey) -> Result<PackageKey, ()> {
        if key.path == "unsafe" {
            return Ok(*self.tc_objs.universe().unsafe_pkg());
        }
        let (path, import_path) = self.validate_path(key)?;
        let path = path.as_path();
        let pkg = self.tc_objs.new_package(import_path.clone());
        self.pkgs.insert(import_path, pkg);
        let files = self.parse_dir(path)?;
        Checker::new(
            self.tc_objs,
            self.ast_objs,
            self.fset,
            self.errors,
            self.pkgs,
            self.all_results,
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
                    import_path = rel.to_string_lossy().into_owned();
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
        match path.canonicalize() {
            Ok(p) => Ok((p, import_path)),
            Err(_) => {
                self.error(format!("failed to canonicalize path: {}", key.path));
                return Err(());
            }
        }
    }

    fn parse_dir(&mut self, path: &Path) -> Result<Vec<ast::File>, ()> {
        let working_dir = self
            .config
            .get_working_dir()
            .ok()
            .map(|x| x.canonicalize().ok())
            .flatten();
        match read_content(path) {
            Ok(contents) => {
                if contents.len() == 0 {
                    self.error(format!("no source file found in dir: {}", path.display()));
                    Err(())
                } else {
                    let mut afiles = vec![];
                    for (path_buf, content) in contents.into_iter() {
                        // try get short display name for the file
                        let p = path_buf.as_path();
                        let full_name = match &working_dir {
                            Some(wd) => p.strip_prefix(wd).unwrap_or(p),
                            None => p,
                        }
                        .to_string_lossy()
                        .to_string();
                        let mut pfile = self.fset.add_file(
                            full_name,
                            Some(self.fset.base()),
                            content.chars().count(),
                        );
                        let afile = Parser::new(
                            self.ast_objs,
                            &mut pfile,
                            self.errors,
                            &content,
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

fn read_content(p: &Path) -> io::Result<Vec<(PathBuf, String)>> {
    let mut result = vec![];
    let mut read = |path: PathBuf| -> io::Result<()> {
        if let Some(ext) = path.extension() {
            if ext == "gos" || ext == "go" || ext == "src" {
                let content = fs::read_to_string(path.as_path())?;
                result.push((path, content))
            }
        }
        Ok(())
    };

    if p.is_dir() {
        let mut paths = vec![];
        for entry in fs::read_dir(p)? {
            let path = entry?.path();
            if !path.is_dir() {
                paths.push(path);
            }
        }
        paths.sort_by(|a, b| a.as_os_str().cmp(b.as_os_str()));
        for p in paths.into_iter() {
            read(p)?;
        }
    } else if p.is_file() {
        read(p.to_path_buf())?;
    }
    if result.len() == 0 {
        return Err(io::Error::new(io::ErrorKind::Other, "no file/dir found"));
    }
    Ok(result)
}

fn is_local(path: &str) -> bool {
    path == "." || path == ".." || path.starts_with("./") || path.starts_with("../")
}
