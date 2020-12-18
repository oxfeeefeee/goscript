#[macro_use]
extern crate time_test;
extern crate goscript_engine as engine;

fn run(path: &str, trace: bool) -> usize {
    let cfg = engine::Config {
        work_dir: Some("./".to_string()),
        base_path: Some("./std/".to_string()),
        trace_parser: trace,
        trace_checker: trace,
        trace_vm: true,
    };
    let engine = engine::Engine::new(cfg);
    engine.run(path)
}

#[test]
fn test_g2case0() {
    let err_cnt = run("./tests/gs/group2/case0.gos", true);
    assert!(err_cnt == 0);
}

#[test]
fn test_g2case1() {
    let err_cnt = run("./tests/gs/group2/case1.gos", true);
    assert!(err_cnt == 0);
}

#[test]
fn test_g2case2() {
    let err_cnt = run("./tests/gs/group2/case2.gos", true);
    assert!(err_cnt == 0);
}

#[test]
fn test_g2case3() {
    let err_cnt = run("./tests/gs/group2/case3.gos", true);
    assert!(err_cnt == 0);
}

#[test]
fn test_g2nil() {
    let err_cnt = run("./tests/gs/group2/nil.gos", true);
    assert!(err_cnt == 0);
}

#[test]
fn test_case1() {
    let err_cnt = run("./tests/gs/group1/case1.gos", true);
    assert!(err_cnt == 0);
}

#[test]
fn test_closure1() {
    let err_cnt = run("./tests/gs/group1/closure1.gos", true);
    assert!(err_cnt == 0);
}

#[test]
fn test_closure2() {
    let err_cnt = run("./tests/gs/group1/closure2.gos", true);
    assert!(err_cnt == 0);
}

#[test]
fn test_closure3() {
    let err_cnt = run("./tests/gs/group1/closure3.gos", true);
    assert!(err_cnt == 0);
}

#[test]
fn test_closure4() {
    let err_cnt = run("./tests/gs/group1/closure4.gos", true);
    assert!(err_cnt == 0);
}

#[test]
fn test_func1() {
    let err_cnt = run("./tests/gs/group1/func1.gos", true);
    assert!(err_cnt == 0);
}

#[test]
fn test_blankid() {
    let err_cnt = run("./tests/gs/group1/blankid.gos", true);
    assert!(err_cnt == 0);
}

#[test]
fn test_declare() {
    let err_cnt = run("./tests/gs/group1/declare.gos", true);
    assert!(err_cnt == 0);
}

#[test]
fn test_slice1() {
    let err_cnt = run("./tests/gs/group1/slice1.gos", true);
    assert!(err_cnt == 0);
}

#[test]
fn test_map1() {
    let err_cnt = run("./tests/gs/group1/map1.gos", true);
    assert!(err_cnt == 0);
}

#[test]
fn test_pkg() {
    let err_cnt = run("./tests/gs/group1/pkg.gos", true);
    assert!(err_cnt == 0);
}

#[test]
fn test_type() {
    let err_cnt = run("./tests/gs/group1/type.gos", true);
    assert!(err_cnt == 0);
}

#[test]
fn test_pointer() {
    let err_cnt = run("./tests/gs/group1/pointer.gos", true);
    assert!(err_cnt == 0);
}

#[test]
fn test_operations() {
    let err_cnt = run("./tests/gs/group1/operations.gos", true);
    assert!(err_cnt == 0);
}

#[test]
fn test_if() {
    let err_cnt = run("./tests/gs/group1/if.gos", true);
    assert!(err_cnt == 0);
}

#[test]
fn test_for() {
    let err_cnt = run("./tests/gs/group1/for.gos", true);
    assert!(err_cnt == 0);
}

#[test]
fn test_interface() {
    let err_cnt = run("./tests/gs/group1/interface.gos", true);
    assert!(err_cnt == 0);
}

#[test]
fn test_initorder() {
    let err_cnt = run("./tests/gs/group1/initorder.gos", true);
    assert!(err_cnt == 0);
}

#[test]
fn test_switch() {
    let err_cnt = run("./tests/gs/group1/switch.gos", true);
    assert!(err_cnt == 0);
}

#[test]
fn test_typeswitch() {
    let err_cnt = run("./tests/gs/group1/typeswitch.gos", true);
    assert!(err_cnt == 0);
}

#[test]
fn test_basictypes() {
    let err_cnt = run("./tests/gs/group1/basictypes.gos", true);
    assert!(err_cnt == 0);
}

#[test]
fn test_composite() {
    let err_cnt = run("./tests/gs/group1/composite.gos", true);
    assert!(err_cnt == 0);
}

#[test]
fn test_leetcode5() {
    time_test!();

    let err_cnt = run("./tests/gs/group1/leetcode5.gos", true);
    assert!(err_cnt == 0);
}
