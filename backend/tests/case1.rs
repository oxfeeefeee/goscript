//use std::cell::RefCell;
//use std::rc::Rc;
extern crate goscript_backend as be;

fn load_parse_gen(path: &str, trace: bool) {
    let bc = be::codegen::CodeGen::load_parse_gen(path, trace);
    let mut vm = be::vm::GosVM::new(bc);
    vm.run();
}
#[test]
fn test_case1() {
    let _err_cnt = load_parse_gen("./tests/data/case1.gos", true);
}

#[test]
fn test_closure1() {
    let _err_cnt = load_parse_gen("./tests/data/closure1.gos", true);
}

#[test]
fn test_closure2() {
    let _err_cnt = load_parse_gen("./tests/data/closure2.gos", true);
}

#[test]
fn test_closure3() {
    let _err_cnt = load_parse_gen("./tests/data/closure3.gos", true);
}

#[test]
fn test_closure4() {
    let _err_cnt = load_parse_gen("./tests/data/closure4.gos", true);
}

#[test]
fn test_func1() {
    let _err_cnt = load_parse_gen("./tests/data/func1.gos", true);
}

#[test]
fn test_blankid() {
    let _err_cnt = load_parse_gen("./tests/data/blankid.gos", true);
}

#[test]
fn test_declare() {
    let _err_cnt = load_parse_gen("./tests/data/declare.gos", true);
}

#[test]
fn test_slice1() {
    let _err_cnt = load_parse_gen("./tests/data/slice1.gos", true);
}

#[test]
fn test_map1() {
    let _err_cnt = load_parse_gen("./tests/data/map1.gos", true);
}

#[test]
fn test_pkg() {
    let _err_cnt = load_parse_gen("./tests/data/pkg.gos", true);
}
