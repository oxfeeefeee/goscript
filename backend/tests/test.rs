//use std::cell::RefCell;
//use std::rc::Rc;
extern crate goscript_backend as be;

fn load_parse_gen(path: &str, trace: bool) -> usize {
    let bc = be::code_gen::CodeGen::load_parse_gen(path, trace);
    let mut vm = be::vm::GosVM::new(bc.0);
    vm.run();
    bc.1
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
