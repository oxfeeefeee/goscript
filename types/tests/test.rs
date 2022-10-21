// Copyright 2022 The Goscript Authors. All rights reserved.
// Use of this source code is governed by a BSD-style
// license that can be found in the LICENSE file.

extern crate goscript_parser as fe;
extern crate goscript_types as types;
use goscript_parser::Map;
use regex::Regex;
use std::env;
use std::fs;
use std::fs::File;
use std::io::{self, BufRead};
use std::path::{Path, PathBuf};
use types::SourceRead;

// Copied from engine/run_fs.rs
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

#[derive(Debug)]
struct ErrInfo {
    text: String,
    regex: Regex,
    checked: bool,
    line: usize,
}

impl ErrInfo {
    fn new(txt: String, line: usize) -> ErrInfo {
        let txt = txt.trim().trim_matches('"').to_string();
        let regex = Regex::new(&txt).unwrap();
        ErrInfo {
            text: txt,
            regex: regex,
            checked: false,
            line: line,
        }
    }
}

fn read_lines<P>(filename: P) -> io::Result<io::Lines<io::BufReader<File>>>
where
    P: AsRef<Path>,
{
    let file = File::open(filename)?;
    Ok(io::BufReader::new(file).lines())
}

fn parse_error(s: &str, line: usize) -> io::Result<Vec<ErrInfo>> {
    let mut texts = vec![];
    let mut cursor = 0;
    loop {
        if let Some(index) = s[cursor..].find("/* ERROR ") {
            if let Some(end) = s[cursor + index..].find(" */") {
                let txt = s[cursor + index + "/* ERROR ".len()..cursor + index + end].to_string();
                texts.push(ErrInfo::new(txt, line));
                cursor = cursor + index + end
            } else {
                return Err(io::Error::new(io::ErrorKind::Other, "invalid comment"));
            }
        } else if let Some(index) = s[cursor..].find("// ERROR ") {
            let txt = s[cursor + index + "// ERROR ".len()..].to_string();
            texts.push(ErrInfo::new(txt, line));
            break;
        } else {
            break;
        }
    }
    Ok(texts)
}

fn test_file(path: &str, trace: bool) {
    dbg!(path);
    let pkgs = &mut Map::new();
    let config = types::TraceConfig {
        trace_parser: trace,
        trace_checker: trace,
    };
    let reader = FsReader::new(Some("./"), None, None);
    let fs = &mut fe::FileSet::new();
    let asto = &mut fe::AstObjects::new();
    let el = &mut fe::ErrorList::new();
    let tco = &mut types::TCObjects::new();
    let results = &mut Map::new();

    let importer = &mut types::Importer::new(&config, &reader, fs, pkgs, results, asto, tco, el, 0);
    let key = types::ImportKey::new(path, "./");
    let _ = importer.import(&key);

    if trace {
        el.sort();
        print!("{}", el);
        //dbg!(results);
    }

    let mut expected_errs = parse_comment_errors(path).unwrap();

    for e in el.borrow().iter() {
        if e.msg.starts_with('\t') || e.by_parser {
            continue;
        }
        if let Some(errs) = expected_errs.get_mut(&e.pos.line) {
            let mut found = false;
            for info in errs.iter_mut() {
                if !info.checked {
                    if info.text == e.msg || info.regex.is_match(&e.msg) {
                        info.checked = true;
                        found = true;
                        break;
                    }
                }
            }
            if !found {
                panic!("unexpected error(1): {}", e);
            }
        } else {
            panic!("unexpected error(2): {}", e);
        }
    }

    for (_, errs) in expected_errs.iter() {
        for info in errs.iter() {
            if !info.checked {
                panic!(
                    "expected error at line {} not reported: {}",
                    info.line, info.text
                );
            }
        }
    }
}

fn parse_comment_errors<P>(path: P) -> io::Result<Map<usize, Vec<ErrInfo>>>
where
    P: AsRef<Path>,
{
    let mut result = Map::new();
    let mut parse_file = |lines: io::Lines<io::BufReader<File>>| -> io::Result<()> {
        for (i, x) in lines.enumerate() {
            let t = x?;
            let mut errors = parse_error(&t, i + 1)?;
            if !errors.is_empty() {
                let entry = result.entry(i + 1).or_insert(vec![]);
                entry.append(&mut errors);
            }
        }
        Ok(())
    };

    if path.as_ref().is_file() {
        let lines = read_lines(path)?;
        parse_file(lines)?;
    } else if path.as_ref().is_dir() {
        for entry in fs::read_dir(path)? {
            let entry = entry?;
            let path = entry.path();
            if !path.is_dir() {
                let lines = read_lines(path)?;
                parse_file(lines)?;
            }
        }
    }

    Ok(result)
}

#[test]
fn test_auto() {
    let trace = false;
    test_file("./tests/data/builtins.gos", trace);
    test_file("./tests/data/const0.gos", trace);
    test_file("./tests/data/const1.gos", trace);
    test_file("./tests/data/constdecl.gos", trace);
    test_file("./tests/data/conversions.gos", trace);
    test_file("./tests/data/conversions2.gos", trace);
    test_file("./tests/data/cycles.gos", trace);
    test_file("./tests/data/cycles1.gos", trace);
    test_file("./tests/data/cycles2.gos", trace);
    test_file("./tests/data/cycles3.gos", trace);
    test_file("./tests/data/cycles4.gos", trace);
    test_file("./tests/data/cycles5.gos", trace);
    test_file("./tests/data/decls0.src", trace);
    test_file("./tests/data/decls1.src", trace);
    test_file("./tests/data/decls2", trace);
    test_file("./tests/data/decls3.src", trace);
    test_file("./tests/data/decls4.src", trace);
    test_file("./tests/data/decls5.src", trace);
    test_file("./tests/data/errors.src", trace);
    test_file("./tests/data/expr0.src", trace);
    test_file("./tests/data/expr2.src", trace);
    test_file("./tests/data/expr3.src", trace);
    test_file("./tests/data/gotos.src", trace);
    test_file("./tests/data/importdecl0", trace);
    test_file("./tests/data/importdecl1", trace);

    test_file("./tests/data/init0.src", trace);
    test_file("./tests/data/init1.src", trace);
    test_file("./tests/data/init2.src", trace);

    test_file("./tests/data/issues.src", trace);
    test_file("./tests/data/labels.src", trace);
    test_file("./tests/data/methodsets.src", trace);
    test_file("./tests/data/shifts.src", trace);
    test_file("./tests/data/stmt0.src", trace);
    test_file("./tests/data/stmt1.src", trace);
    test_file("./tests/data/vardecl.src", trace);
}

#[test]
fn test_temp() {
    test_file("./tests/data/temp.gos", true);
}
