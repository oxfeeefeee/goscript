use std::cell::RefCell;
use std::rc::Rc;
extern crate goscript_backend as be;

fn load_parse_gen(path: &str, trace: bool) {
    let (bc, entry) = be::codegen::CodeGen::load_parse_gen(path, trace);
    let mut vm = be::vm::GosVM::new(&bc);
    vm.run(entry);
}
#[test]
fn test_parser_case1() {
    let err_cnt = load_parse_gen("./tests/data/case1.gos", true);
}
