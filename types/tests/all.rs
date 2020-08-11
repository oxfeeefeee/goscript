extern crate goscript_parser as fe;
extern crate goscript_types as types;
use regex::Regex;
use std::collections::HashMap;
use std::fs::File;
use std::io::{self, BufRead};
use std::path::Path;

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
    let pkgs = &mut HashMap::new();
    let config = types::Config {
        work_dir: Some("./".to_string()),
        base_path: None,
        trace_parser: trace,
        trace_checker: trace,
    };
    let fs = &mut fe::FileSet::new();
    let asto = &mut fe::objects::Objects::new();
    let el = &mut fe::errors::ErrorList::new();
    let tco = &mut types::objects::TCObjects::new();

    let importer = &mut types::Importer::new(&config, fs, pkgs, asto, tco, el, 0);
    let key = types::ImportKey::new(path, "./");
    let _ = importer.import(&key);

    //el.sort();
    //print!("{}", el);

    let mut expected_errs = parse_comment_errors(path).unwrap();

    for e in el.borrow().iter() {
        if let Some(errs) = expected_errs.get_mut(&e.pos.line) {
            if e.msg.starts_with('\t') {
                continue;
            }
            let mut found = false;
            for info in errs.iter_mut() {
                if !info.checked {
                    if info.regex.is_match(&e.msg) {
                        info.checked = true;
                        found = true;
                        break;
                    }
                }
            }
            if !found {
                panic!(format!("unexpected error: {}", e));
            }
        } else {
            panic!(format!("unexpected error: {}", e));
        }
    }

    for (_, errs) in expected_errs.iter() {
        for info in errs.iter() {
            if !info.checked {
                panic!(format!(
                    "expected error at line {} not reported: {}",
                    info.line, info.text
                ));
            }
        }
    }
}

fn parse_comment_errors(path: &str) -> io::Result<HashMap<usize, Vec<ErrInfo>>> {
    let mut result = HashMap::new();
    let lines = read_lines(path)?;
    for (i, x) in lines.enumerate() {
        let t = x?;
        let errors = parse_error(&t, i + 1)?;
        if !errors.is_empty() {
            result.insert(i + 1, errors);
        }
    }
    Ok(result)
}

#[test]
fn test_auto() {
    let trace = false;
    test_file("./tests/data/all/builtins.gos", trace);
    test_file("./tests/data/all/const0.gos", trace);
    test_file("./tests/data/all/const1.gos", trace);
    test_file("./tests/data/all/constdecl.gos", trace);
}
