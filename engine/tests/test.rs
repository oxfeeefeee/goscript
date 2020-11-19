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
fn test_case1() {
    let err_cnt = run("./tests/gs/case1.gos", true);
    assert!(err_cnt == 0);
}

#[test]
fn test_closure1() {
    let err_cnt = run("./tests/gs/closure1.gos", true);
    assert!(err_cnt == 0);
}

#[test]
fn test_closure2() {
    let err_cnt = run("./tests/gs/closure2.gos", true);
    assert!(err_cnt == 0);
}

#[test]
fn test_closure3() {
    let err_cnt = run("./tests/gs/closure3.gos", true);
    assert!(err_cnt == 0);
}

#[test]
fn test_closure4() {
    let err_cnt = run("./tests/gs/closure4.gos", true);
    assert!(err_cnt == 0);
}

#[test]
fn test_func1() {
    let err_cnt = run("./tests/gs/func1.gos", true);
    assert!(err_cnt == 0);
}

#[test]
fn test_blankid() {
    let err_cnt = run("./tests/gs/blankid.gos", true);
    assert!(err_cnt == 0);
}

#[test]
fn test_declare() {
    let err_cnt = run("./tests/gs/declare.gos", true);
    assert!(err_cnt == 0);
}

#[test]
fn test_slice1() {
    let err_cnt = run("./tests/gs/slice1.gos", true);
    assert!(err_cnt == 0);
}

#[test]
fn test_map1() {
    let err_cnt = run("./tests/gs/map1.gos", true);
    assert!(err_cnt == 0);
}

#[test]
fn test_pkg() {
    let err_cnt = run("./tests/gs/pkg.gos", true);
    assert!(err_cnt == 0);
}

#[test]
fn test_type() {
    let err_cnt = run("./tests/gs/type.gos", true);
    assert!(err_cnt == 0);
}

#[test]
fn test_pointer() {
    let err_cnt = run("./tests/gs/pointer.gos", true);
    assert!(err_cnt == 0);
}

#[test]
fn test_operations() {
    let err_cnt = run("./tests/gs/operations.gos", true);
    assert!(err_cnt == 0);
}

#[test]
fn test_if() {
    let err_cnt = run("./tests/gs/if.gos", true);
    assert!(err_cnt == 0);
}

#[test]
fn test_for() {
    let err_cnt = run("./tests/gs/for.gos", true);
    assert!(err_cnt == 0);
}

#[test]
fn test_interface() {
    let err_cnt = run("./tests/gs/interface.gos", true);
    assert!(err_cnt == 0);
}

#[test]
fn test_initorder() {
    let err_cnt = run("./tests/gs/initorder.gos", true);
    assert!(err_cnt == 0);
}

#[test]
fn test_switch() {
    let err_cnt = run("./tests/gs/switch.gos", true);
    assert!(err_cnt == 0);
}

#[test]
fn test_typeswitch() {
    let err_cnt = run("./tests/gs/typeswitch.gos", true);
    assert!(err_cnt == 0);
}

#[test]
fn test_leetcode5() {
    time_test!();

    let err_cnt = run("./tests/gs/leetcode5.gos", true);
    assert!(err_cnt == 0);
}
