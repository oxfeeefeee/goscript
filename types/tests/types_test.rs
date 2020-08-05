extern crate goscript_parser as fe;
extern crate goscript_types as types;
use std::collections::HashMap;

fn load_parse_check(dir: &str, trace: bool) -> usize {
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
    let key = types::ImportKey::new("./", dir);
    importer.import(&key).unwrap();

    el.sort();
    print!("{}", el);

    el.len()
}

#[test]
fn test_types_case1() {
    load_parse_check("./tests/data/case1/", true);
}

#[test]
fn test_types_interface() {
    load_parse_check("./tests/data/case2/", true);
}

#[test]
fn test_types_cycles() {
    load_parse_check("./tests/data/case3/", true);
}

#[test]
fn test_types_decl0() {
    load_parse_check("./tests/data/case4/", true);
}

#[test]
fn test_types_decl1() {
    load_parse_check("./tests/data/case5/", true);
}

#[test]
fn test_types_init() {
    load_parse_check("./tests/data/case6/", true);
}

#[test]
fn test_types_expr() {
    load_parse_check("./tests/data/case7/", true);
}

#[test]
fn test_types_stmt() {
    load_parse_check("./tests/data/case8/", true);
}

#[test]
fn test_types_stmt1() {
    load_parse_check("./tests/data/case9/", true);
}

#[test]
fn test_types_goto() {
    load_parse_check("./tests/data/goto/", true);
}

#[test]
fn test_types_simple() {
    load_parse_check("./tests/data/simple/", true);
}
