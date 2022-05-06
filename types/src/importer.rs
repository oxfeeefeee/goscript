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

use super::check::check::{Checker, TypeInfo};
use super::objects::{PackageKey, TCObjects};
use goscript_parser::ast;
use goscript_parser::errors::ErrorList;
use goscript_parser::objects::Objects as AstObjects;
use goscript_parser::position;
use goscript_parser::{FileSet, Parser};
use std::collections::HashMap;
use std::env;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

pub struct TraceConfig {
    //print debug info in parser
    pub trace_parser: bool,
    // print debug info in checker
    pub trace_checker: bool,
}

pub trait SourceRead {
    fn working_dir(&self) -> io::Result<PathBuf>;

    fn base_dir(&self) -> Option<&str>;

    fn read_file(&self, path: &Path) -> io::Result<String>;

    fn read_dir(&self, path: &Path) -> io::Result<Vec<PathBuf>>;

    fn is_file(&self, path: &Path) -> bool;

    fn is_dir(&self, path: &Path) -> bool;

    fn canonicalize_path(&self, path: &PathBuf) -> io::Result<PathBuf>;
}

pub struct FsReader<'a> {
    working_dir: Option<&'a str>,
    base_dir: Option<&'a str>,
    temp_file: Option<&'a str>,
}

impl<'a> FsReader<'a> {
    pub fn new(
        working_dir: Option<&'a str>,
        base_dir: Option<&'a str>,
        temp_file: Option<&'a str>,
    ) -> FsReader<'a> {
        FsReader {
            working_dir,
            base_dir,
            temp_file,
        }
    }

    pub fn temp_file_path() -> &'static str {
        &"./temp_file_in_memory_for_testing_and_you_can_only_have_one.gos"
    }
}

impl<'a> SourceRead for FsReader<'a> {
    fn working_dir(&self) -> io::Result<PathBuf> {
        if let Some(wd) = &self.working_dir {
            let mut buf = PathBuf::new();
            buf.push(wd);
            Ok(buf)
        } else {
            env::current_dir()
        }
    }

    fn base_dir(&self) -> Option<&str> {
        self.base_dir
    }

    fn read_file(&self, path: &Path) -> io::Result<String> {
        if path.ends_with(Self::temp_file_path()) {
            self.temp_file
                .map(|x| x.to_owned())
                .ok_or(io::Error::from(io::ErrorKind::NotFound))
        } else {
            fs::read_to_string(path)
        }
    }

    fn read_dir(&self, path: &Path) -> io::Result<Vec<PathBuf>> {
        Ok(fs::read_dir(path)?
            .filter_map(|x| {
                x.map_or(None, |e| {
                    let path = e.path();
                    (!path.is_dir()).then(|| path)
                })
            })
            .collect())
    }

    fn is_file(&self, path: &Path) -> bool {
        if path.ends_with(Self::temp_file_path()) {
            true
        } else {
            path.is_file()
        }
    }

    fn is_dir(&self, path: &Path) -> bool {
        path.is_dir()
    }

    fn canonicalize_path(&self, path: &PathBuf) -> io::Result<PathBuf> {
        if path.ends_with(Self::temp_file_path()) {
            Ok(path.clone())
        } else if !path.exists() {
            Err(io::Error::from(io::ErrorKind::NotFound))
        } else {
            path.canonicalize()
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

pub struct Importer<'a, S: SourceRead> {
    trace_config: &'a TraceConfig,
    reader: &'a S,
    fset: &'a mut FileSet,
    pkgs: &'a mut HashMap<String, PackageKey>,
    all_results: &'a mut HashMap<PackageKey, TypeInfo>,
    ast_objs: &'a mut AstObjects,
    tc_objs: &'a mut TCObjects,
    errors: &'a ErrorList,
    pos: position::Pos,
}

impl<'a, S: SourceRead> Importer<'a, S> {
    pub fn new(
        config: &'a TraceConfig,
        reader: &'a S,
        fset: &'a mut FileSet,
        pkgs: &'a mut HashMap<String, PackageKey>,
        all_results: &'a mut HashMap<PackageKey, TypeInfo>,
        ast_objs: &'a mut AstObjects,
        tc_objs: &'a mut TCObjects,
        errors: &'a ErrorList,
        pos: position::Pos,
    ) -> Importer<'a, S> {
        Importer {
            trace_config: config,
            reader: reader,
            fset: fset,
            pkgs: pkgs,
            all_results: all_results,
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
        let pb = self.canonicalize_import(key)?;
        let path = pb.0.as_path();
        let import_path = pb.1;
        match self.pkgs.get(&import_path) {
            Some(key) => Ok(*key),
            None => {
                let pkg = self.tc_objs.new_package(import_path.clone());
                self.pkgs.insert(import_path, pkg);
                let files = self.parse_path(path)?;
                Checker::new(
                    self.tc_objs,
                    self.ast_objs,
                    self.fset,
                    self.errors,
                    self.pkgs,
                    self.all_results,
                    pkg,
                    self.trace_config,
                    self.reader,
                )
                .check(files)
            }
        }
    }

    fn canonicalize_import(&mut self, key: &'a ImportKey) -> Result<(PathBuf, String), ()> {
        let mut import_path = key.path.clone();
        let path = if is_local(&key.path) {
            let working_dir = self.reader.working_dir();
            if working_dir.is_err() {
                return self.error(format!("failed to get working dir for: {}", key.path));
            }
            let mut wd = working_dir.unwrap();
            wd.push(&key.dir);
            wd.push(&key.path);
            if let Some(base) = &self.reader.base_dir() {
                if let Ok(rel) = wd.as_path().strip_prefix(base) {
                    import_path = rel.to_string_lossy().to_string()
                }
            }
            wd
        } else {
            if let Some(base) = &self.reader.base_dir() {
                let mut p = PathBuf::new();
                p.push(base);
                p.push(&key.path);
                p
            } else {
                return self.error(format!("base dir required for path: {}", key.path));
            }
        };
        match self.reader.canonicalize_path(&path) {
            Ok(p) => Ok((p, import_path)),
            Err(e) => self.error(format!("{} {}", e, key.path)),
        }
    }

    fn parse_path(&mut self, path: &Path) -> Result<Vec<ast::File>, ()> {
        match read_content(path, self.reader) {
            Ok(contents) => {
                if contents.len() == 0 {
                    self.error(format!("no source file found in dir: {}", path.display()))
                } else {
                    let mut afiles = vec![];
                    for (full_name, content) in contents.into_iter() {
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
                            self.trace_config.trace_parser,
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
            Err(e) => self.error(format!(
                "failed to read from path: {}, {}",
                path.display(),
                e
            )),
        }
    }

    fn error<T>(&self, err: String) -> Result<T, ()> {
        self.errors
            .add(self.fset.position(self.pos), err, false, false);
        Err(())
    }
}

fn read_content(p: &Path, reader: &dyn SourceRead) -> io::Result<Vec<(String, String)>> {
    let working_dir = reader
        .working_dir()
        .ok()
        .map(|x| x.canonicalize().ok())
        .flatten();
    let mut result = vec![];
    let mut read = |path: PathBuf| -> io::Result<()> {
        if let Some(ext) = path.extension() {
            if ext == "gos" || ext == "go" || ext == "src" {
                if let Some(fs) = path.file_stem() {
                    let s = fs.to_str();
                    if s.is_some() && !s.unwrap().ends_with("_test") {
                        let p = path.as_path();
                        let content = reader.read_file(p)?;
                        // try get short display name for the file
                        let full_name = match &working_dir {
                            Some(wd) => p.strip_prefix(wd).unwrap_or(p),
                            None => p,
                        }
                        .to_string_lossy()
                        .to_string();
                        result.push((full_name, content))
                    }
                }
            }
        }
        Ok(())
    };

    if reader.is_dir(p) {
        let mut paths = reader.read_dir(p)?;
        paths.sort_by(|a, b| a.as_os_str().cmp(b.as_os_str()));
        for p in paths.into_iter() {
            read(p)?;
        }
    } else if reader.is_file(p) {
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
