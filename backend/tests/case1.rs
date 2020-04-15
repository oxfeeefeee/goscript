use std::cell::RefCell;
use std::rc::Rc;
extern crate goscript_backend as be;

fn load_parse_gen(path: &str, trace: bool) {
    let (bc, entry) = be::codegen::CodeGen::load_parse_gen(path, trace);
    let mut vm = be::vm::GosVM::new(bc);
    vm.run(entry);
}
#[test]
fn test_case1() {
    let err_cnt = load_parse_gen("./tests/data/case1.gos", true);
}

#[test]
fn test_closure1() {
    let err_cnt = load_parse_gen("./tests/data/closure1.go", true);
}

#[test]
fn test_closure2() {
    let err_cnt = load_parse_gen("./tests/data/closure2.go", true);
}

#[test]
fn test_closure3() {
    let err_cnt = load_parse_gen("./tests/data/closure3.go", true);
}

#[test]
fn test_closure4() {
    let err_cnt = load_parse_gen("./tests/data/closure4.go", true);
}

#[test]
fn test_func1() {
    let err_cnt = load_parse_gen("./tests/data/func1.go", true);
}

#[test]
fn test_blankid() {
    let err_cnt = load_parse_gen("./tests/data/blankid.go", true);
}
