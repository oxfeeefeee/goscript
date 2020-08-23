//use std::cell::RefCell;
//use std::rc::Rc;
#[macro_use]
extern crate time_test;
extern crate goscript_codegen as cg;
extern crate goscript_parser as fe;
extern crate goscript_types as types;
extern crate goscript_vm as vm;

fn load_parse_gen(path: &str, trace: bool) -> usize {
    let config = types::Config {
        work_dir: Some("./".to_string()),
        base_path: None,
        trace_parser: trace,
        trace_checker: trace,
    };
    let fs = &mut fe::FileSet::new();
    let el = &mut fe::errors::ErrorList::new();
    let code = cg::entry::parse_check_gen(path, &config, fs, el);
    if let Ok(bc) = code {
        let mut vm = vm::vm::GosVM::new(bc);
        vm.run();
        0
    } else {
        if trace {
            el.sort();
            print!("{}", el);
        }
        code.unwrap_err()
    }
}

#[test]
fn test_bcase1() {
    let err_cnt = load_parse_gen("./tests/data/case1.gos", true);
    assert!(err_cnt == 0);
}

#[test]
fn test_closure1() {
    let err_cnt = load_parse_gen("./tests/data/closure1.gos", true);
    assert!(err_cnt == 0);
}

#[test]
fn test_closure2() {
    let err_cnt = load_parse_gen("./tests/data/closure2.gos", true);
    assert!(err_cnt == 0);
}

#[test]
fn test_closure3() {
    let err_cnt = load_parse_gen("./tests/data/closure3.gos", true);
    assert!(err_cnt == 0);
}

#[test]
fn test_closure4() {
    let err_cnt = load_parse_gen("./tests/data/closure4.gos", true);
    assert!(err_cnt == 0);
}

#[test]
fn test_func1() {
    let err_cnt = load_parse_gen("./tests/data/func1.gos", true);
    assert!(err_cnt == 0);
}

#[test]
fn test_blankid() {
    let err_cnt = load_parse_gen("./tests/data/blankid.gos", true);
    assert!(err_cnt == 0);
}

#[test]
fn test_declare() {
    let err_cnt = load_parse_gen("./tests/data/declare.gos", true);
    assert!(err_cnt == 0);
}

#[test]
fn test_slice1() {
    let err_cnt = load_parse_gen("./tests/data/slice1.gos", true);
    assert!(err_cnt == 0);
}

#[test]
fn test_map1() {
    let err_cnt = load_parse_gen("./tests/data/map1.gos", true);
    assert!(err_cnt == 0);
}

#[test]
fn test_pkg() {
    let err_cnt = load_parse_gen("./tests/data/pkg.gos", true);
    assert!(err_cnt == 0);
}

#[test]
fn test_type() {
    let err_cnt = load_parse_gen("./tests/data/type.gos", true);
    assert!(err_cnt == 0);
}

#[test]
fn test_pointer() {
    let err_cnt = load_parse_gen("./tests/data/pointer.gos", true);
    assert!(err_cnt == 0);
}

#[test]
fn test_operations() {
    let err_cnt = load_parse_gen("./tests/data/operations.gos", true);
    assert!(err_cnt == 0);
}

#[test]
fn test_if() {
    let err_cnt = load_parse_gen("./tests/data/if.gos", true);
    assert!(err_cnt == 0);
}

#[test]
fn test_for() {
    let err_cnt = load_parse_gen("./tests/data/for.gos", true);
    assert!(err_cnt == 0);
}

#[test]
fn test_leetcode5() {
    time_test!();

    let err_cnt = load_parse_gen("./tests/data/leetcode5.gos", true);
    assert!(err_cnt == 0);
}
