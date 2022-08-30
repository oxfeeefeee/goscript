#![allow(dead_code)]

#[cfg(feature = "run_zip")]
use std::fs;
use std::io;
use std::io::Write;
#[cfg(feature = "run_zip")]
use std::path::Path;

#[macro_use]
extern crate time_test;
extern crate goscript_engine as engine;

#[derive(Clone)]
struct WriteBuf {
    buffer: io::Cursor<Vec<u8>>,
}

impl WriteBuf {
    fn new() -> WriteBuf {
        WriteBuf {
            buffer: io::Cursor::new(vec![]),
        }
    }

    fn into_string(self) -> String {
        String::from_utf8_lossy(&self.buffer.into_inner()).into_owned()
    }
}

impl Write for WriteBuf {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.buffer.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

fn run(path: &str, trace: bool) -> Result<(), engine::ErrorList> {
    let mut cfg = engine::run_fs::Config::default();
    cfg.working_dir = Some("./");
    cfg.base_dir = Some("../std/");
    cfg.trace_parser = trace;
    cfg.trace_checker = trace;
    let result = engine::run_fs::run(cfg, path);
    if let Err(el) = &result {
        el.sort();
        eprint!("{}", el);
    }
    result
}

fn run_string(source: &str, trace: bool) -> Result<(), engine::ErrorList> {
    let mut cfg = engine::run_fs::Config::default();
    cfg.working_dir = Some("./");
    cfg.base_dir = Some("../std/");
    cfg.trace_parser = trace;
    cfg.trace_checker = trace;
    let result = engine::run_fs::run_string(cfg, source);
    if let Err(el) = &result {
        el.sort();
        eprint!("{}", el);
    }
    result
}

#[cfg(feature = "run_zip")]
fn run_zip_and_string(file: &str, source: &str, trace: bool) -> Result<(), engine::ErrorList> {
    let mut cfg = engine::run_zip::Config::default();
    cfg.base_dir = Some("std/");
    cfg.trace_parser = trace;
    cfg.trace_checker = trace;
    let zip = fs::read(Path::new(file)).unwrap();
    let result = engine::run_zip::run_string(&zip, cfg, source);
    if let Err(el) = &result {
        el.sort();
        eprint!("{}", el);
    }
    result
}

#[test]
fn test_source() {
    let source = r#"
    package main
    import (
        "fmt"
    )
    func main() {
        fmt.Println("hello from raw source")
    }
    "#;
    let result = run_string(source, false);
    assert!(result.is_ok());
}

#[test]
#[cfg(feature = "run_zip")]
fn test_zip() {
    let source = r#"
    package main
    import (
        "fmt"
    )
    func main() {
        fmt.Println("hello from raw zip lalala")
    }
    "#;
    let result = run_zip_and_string("./tests/std.zip", source, false);
    assert!(result.is_ok());
}

#[test]
fn test_g2case0() {
    let result = run("./tests/group2/case0.gos", true);
    assert!(result.is_ok());
}

#[test]
fn test_g2case1() {
    let result = run("./tests/group2/case1.gos", true);
    assert!(result.is_ok());
}

#[test]
fn test_g2case2() {
    let result = run("./tests/group2/case2.gos", true);
    assert!(result.is_ok());
}

#[test]
fn test_g2case3() {
    let result = run("./tests/group2/case3.gos", true);
    assert!(result.is_ok());
}

#[test]
fn test_g2nil() {
    let result = run("./tests/group2/nil.gos", true);
    assert!(result.is_ok());
}

#[test]
fn test_g2display() {
    let result = run("./tests/group2/display.gos", true);
    assert!(result.is_ok());
}

#[test]
fn test_unicode0() {
    time_test!();

    let result = run("./tests/group2/unicode0.gos", true);
    assert!(result.is_ok());
}

#[test]
fn test_sync_mutex() {
    let result = run("./tests/group2/sync_mutex.gos", true);
    assert!(result.is_ok());
}

#[test]
fn test_sync_rwmutex() {
    let result = run("./tests/group2/sync_rwmutex.gos", true);
    assert!(result.is_ok());
}

#[test]
fn test_g1case1() {
    let result = run("./tests/group1/case1.gos", true);
    assert!(result.is_ok());
}

#[test]
fn test_closure1() {
    let result = run("./tests/group1/closure1.gos", true);
    assert!(result.is_ok());
}

#[test]
fn test_closure2() {
    let result = run("./tests/group1/closure2.gos", true);
    assert!(result.is_ok());
}

#[test]
fn test_closure3() {
    let result = run("./tests/group1/closure3.gos", true);
    assert!(result.is_ok());
}

#[test]
fn test_closure4() {
    let result = run("./tests/group1/closure4.gos", true);
    assert!(result.is_ok());
}

#[test]
fn test_func1() {
    let result = run("./tests/group1/func1.gos", true);
    assert!(result.is_ok());
}

#[test]
fn test_blankid() {
    let result = run("./tests/group1/blankid.gos", true);
    assert!(result.is_ok());
}

#[test]
fn test_declare() {
    let result = run("./tests/group1/declare.gos", true);
    assert!(result.is_ok());
}

#[test]
fn test_slice1() {
    let result = run("./tests/group1/slice1.gos", true);
    assert!(result.is_ok());
}

#[test]
fn test_map1() {
    let result = run("./tests/group1/map1.gos", true);
    assert!(result.is_ok());
}

#[test]
fn test_pkg() {
    let result = run("./tests/group1/pkg.gos", true);
    assert!(result.is_ok());
}

#[test]
fn test_type1() {
    let result = run("./tests/group1/type1.gos", true);
    assert!(result.is_ok());
}

#[test]
fn test_pointer() {
    let result = run("./tests/group1/pointer.gos", true);
    assert!(result.is_ok());
}

#[test]
fn test_operations() {
    let result = run("./tests/group1/operations.gos", true);
    assert!(result.is_ok());
}

#[test]
fn test_if() {
    let result = run("./tests/group1/if.gos", true);
    assert!(result.is_ok());
}

#[test]
fn test_for() {
    let result = run("./tests/group1/for.gos", true);
    assert!(result.is_ok());
}

#[test]
fn test_interface1() {
    let result = run("./tests/group1/interface1.gos", true);
    assert!(result.is_ok());
}

#[test]
fn test_interface2() {
    let result = run("./tests/group1/interface2.gos", true);
    assert!(result.is_ok());
}

#[test]
fn test_initorder() {
    let result = run("./tests/group1/initorder.gos", true);
    assert!(result.is_ok());
}

#[test]
fn test_switch() {
    let result = run("./tests/group1/switch.gos", true);
    assert!(result.is_ok());
}

#[test]
fn test_typeswitch() {
    let result = run("./tests/group1/typeswitch.gos", false);
    assert!(result.is_ok());
}

#[test]
fn test_basictypes() {
    let result = run("./tests/group1/basictypes.gos", true);
    assert!(result.is_ok());
}

#[test]
fn test_composite() {
    let result = run("./tests/group1/composite.gos", true);
    assert!(result.is_ok());
}

#[test]
fn test_array() {
    let result = run("./tests/group1/array.gos", true);
    assert!(result.is_ok());
}

#[test]
fn test_conversion() {
    let result = run("./tests/group1/conversion.gos", true);
    assert!(result.is_ok());
}

#[test]
fn test_async() {
    let result = run("./tests/group1/async.gos", true);
    assert!(result.is_ok());
}

#[test]
fn test_channel() {
    let result = run("./tests/group1/channel.gos", true);
    assert!(result.is_ok());
}

#[test]
fn test_defer() {
    let result = run("./tests/group1/defer.gos", true);
    assert!(result.is_ok());
}

#[test]
fn test_goto() {
    let result = run("./tests/group1/goto.gos", true);
    assert!(result.is_ok());
}

#[test]
fn test_recover() {
    let result = run("./tests/group1/recover.gos", true);
    assert!(result.is_ok());
}

#[test]
fn test_select() {
    let result = run("./tests/group1/select.gos", true);
    assert!(result.is_ok());
}

#[test]
fn test_init_func() {
    let result = run("./tests/group1/init_func.gos", true);
    assert!(result.is_ok());
}

#[test]
fn test_complex() {
    let result = run("./tests/group1/complex.gos", true);
    assert!(result.is_ok());
}

#[test]
fn test_linked() {
    let result = run("./tests/demo/linked.gos", true);
    assert!(result.is_ok());
}

#[test]
fn test_leetcode5() {
    time_test!();

    let result = run("./tests/demo/leetcode5.gos", true);
    assert!(result.is_ok());
}

#[test]
fn test_fibonacci() {
    time_test!();

    let result = run("./tests/demo/fibonacci.gos", true);
    assert!(result.is_ok());
}

#[test]
fn test_issue8() {
    time_test!();

    let result = run("./tests/issues/issue8.gos", true);
    assert!(result.is_ok());
}

#[test]
fn test_std_math() {
    time_test!();

    let result = run("./tests/std/math.gos", false);
    assert!(result.is_ok());
}

#[test]
fn test_std_strconv() {
    time_test!();

    let result = run("./tests/std/strconv.gos", false);
    assert!(result.is_ok());
}

#[test]
fn test_std_reflect() {
    time_test!();

    let result = run("./tests/std/reflect.gos", false);
    assert!(result.is_ok());
}

#[test]
fn test_std_time() {
    time_test!();

    let result = run("./tests/std/time.gos", false);
    assert!(result.is_ok());
}

#[test]
fn test_std_sort() {
    time_test!();

    let result = run("./tests/std/sort.gos", false);
    assert!(result.is_ok());
}

#[test]
fn test_std_path() {
    time_test!();

    let result = run("./tests/std/path.gos", false);
    assert!(result.is_ok());
}

#[test]
fn test_std_bytes() {
    time_test!();

    let result = run("./tests/std/bytes.gos", false);
    assert!(result.is_ok());
}

#[test]
fn test_std_strings() {
    time_test!();

    let result = run("./tests/std/strings.gos", false);
    assert!(result.is_ok());
}

#[test]
fn test_std_fmt() {
    time_test!();

    let result = run("./tests/std/fmt.gos", false);
    assert!(result.is_ok());
}

#[test]
fn test_std_temp() {
    time_test!();

    let result = run("./tests/std/temp.gos", false);
    assert!(result.is_ok());
}
