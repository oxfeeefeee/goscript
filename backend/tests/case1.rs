use std::cell::RefCell;
use std::rc::Rc;
extern crate goscript_backend as be;

fn load_parse_gen(path: &str, trace: bool) {
    let mut cg = be::codegen::CodeGen::new();
    cg.new_load_parse_gen(path, trace);
    let (bc, entry) = cg.into_byte_code();
    let mut vm = be::vm::GosVM::new(&bc);
    vm.run(entry);
}
#[test]
fn test_parser_case1() {
    let err_cnt = load_parse_gen("./tests/data/case1.gos", true);
}
