#![allow(dead_code)]

use std::borrow::Cow;
#[cfg(feature = "read_zip")]
use std::fs;
use std::io;
use std::io::Write;
#[cfg(any(feature = "read_zip", feature = "go_std"))]
use std::path::{Path, PathBuf};
use std::rc::Rc;

#[macro_use]
extern crate time_test;
extern crate go_engine as engine;

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
    run_path(path, trace, true)
}

#[cfg(feature = "go_std")]
fn run_path(path: &str, trace: bool, fail_on_panic: bool) -> Result<(), engine::ErrorList> {
    let mut cfg = engine::Config::default();
    cfg.trace_parser = trace;
    cfg.trace_checker = trace;
    let sr = engine::SourceReader::local_fs(PathBuf::from("../std/"), PathBuf::from("./"));
    let ph: Option<Rc<dyn Fn(String, String)>> =
        Some(Rc::new(move |msg: String, stack: String| {
            eprintln!("{}\n", msg);
            eprintln!("{}\n", stack);
            if fail_on_panic {
                panic!("test panicked");
            }
        }));
    let result = engine::run(cfg, &sr, Path::new(path), ph);
    if let Err(el) = &result {
        el.sort();
        eprint!("{}", el);
    }
    result
}

#[cfg(not(feature = "go_std"))]
fn run_path(_path: &str, _trace: bool, fail_on_panic: bool) -> Result<(), engine::ErrorList> {
    unimplemented!()
}

#[cfg(all(feature = "read_zip", feature = "go_std"))]
fn run_zip(zip: &str, path: &str, trace: bool) -> Result<(), engine::ErrorList> {
    let zip = fs::read(Path::new(zip)).unwrap();

    let mut cfg = engine::Config::default();
    cfg.trace_parser = trace;
    cfg.trace_checker = trace;
    let sr = engine::SourceReader::zip_lib_and_local_fs(
        std::borrow::Cow::Owned(zip),
        PathBuf::from("std/"),
        PathBuf::from("./"),
    );
    let ph: Option<Rc<dyn Fn(String, String)>> =
        Some(Rc::new(move |msg: String, stack: String| {
            eprintln!("{}\n", msg);
            eprintln!("{}\n", stack);
            panic!("test panicked");
        }));
    let result = engine::run(cfg, &sr, Path::new(path), ph);
    if let Err(el) = &result {
        el.sort();
        eprint!("{}", el);
    }
    result
}

#[cfg(not(feature = "go_std"))]
fn run_zip(_zip: &str, _path: &str, _trace: bool) -> Result<(), engine::ErrorList> {
    unimplemented!()
}

#[cfg(feature = "go_std")]
fn run_string(source: Cow<'static, str>, trace: bool) -> Result<(), engine::ErrorList> {
    let mut cfg = engine::Config::default();
    cfg.trace_parser = trace;
    cfg.trace_checker = trace;
    let (sr, path) = engine::SourceReader::fs_lib_and_string(PathBuf::from("../std/"), source);
    let ph: Option<Rc<dyn Fn(String, String)>> =
        Some(Rc::new(move |msg: String, stack: String| {
            eprintln!("{}\n", msg);
            eprintln!("{}\n", stack);
            panic!("test panicked");
        }));
    let result = engine::run(cfg, &sr, &path, ph);
    if let Err(el) = &result {
        el.sort();
        eprint!("{}", el);
    }
    result
}

#[cfg(not(feature = "go_std"))]
fn run_string(_source: &str, _trace: bool) -> Result<(), engine::ErrorList> {
    unimplemented!()
}

#[cfg(feature = "read_zip")]
fn run_zip_and_string(
    file: &str,
    source: Cow<'static, str>,
    trace: bool,
) -> Result<(), engine::ErrorList> {
    let zip = fs::read(Path::new(file)).unwrap();

    let mut cfg = engine::Config::default();
    cfg.trace_parser = trace;
    cfg.trace_checker = trace;
    let (sr, path) = engine::SourceReader::zip_lib_and_string(
        std::borrow::Cow::Owned(zip),
        PathBuf::from("std/"),
        source,
    );
    let ph: Option<Rc<dyn Fn(String, String)>> =
        Some(Rc::new(move |msg: String, stack: String| {
            eprintln!("{}\n", msg);
            eprintln!("{}\n", stack);
            panic!("test panicked");
        }));
    let result = engine::run(cfg, &sr, &path, ph);
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
    let result = run_string(Cow::Borrowed(source), false);
    dbg!(&result);
    assert!(result.is_ok());
}

#[test]
#[cfg(feature = "read_zip")]
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
    let result = run_zip_and_string("./tests/std.zip", Cow::Borrowed(source), false);
    assert!(result.is_ok());
}

#[test]
#[cfg(all(feature = "read_zip", feature = "go_std"))]
fn test_zip_g2case0() {
    let result = run_zip("./tests/std.zip", "./tests/group2/case0.gos", false);
    assert!(result.is_ok());
}

#[test]
fn test_g2case0() {
    let result = run("./tests/group2/case0.gos", false);
    assert!(result.is_ok());
}

#[test]
fn test_g2case1() {
    let result = run("./tests/group2/case1.gos", true);
    assert!(result.is_ok());
}

#[test]
fn test_g2case2() {
    let result = run_path("./tests/group2/case2.gos", true, false);
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
fn test_local() {
    let result = run("./tests/group2/local.gos", false);
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
fn test_issue12() {
    time_test!();

    let result = run("./tests/issues/issue12.gos", true);
    assert!(result.is_ok());
}

#[test]
fn test_issue16() {
    time_test!();

    let result = run("./tests/issues/issue16.gos", true);
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
fn test_map_perf() {
    time_test!();
    let mut m: go_parser::Map<usize, usize> = go_parser::Map::new();
    for i in 0..200000 {
        m.insert(i, i);
    }
    let mut total: usize = 0;
    for i in 0..200000 {
        total += m[&i];
    }
    dbg!(total);
}

#[test]
fn test_map_perf2() {
    time_test!();
    use go_vm::types::GosValue;
    let mut m: go_parser::Map<GosValue, usize> = go_parser::Map::new();
    for i in 0..200000 {
        m.insert(i.into(), i);
    }
    let mut total: usize = 0;
    for i in 0usize..200000 {
        total += m[&i.into()];
    }
    dbg!(total);
}

#[test]
fn test_std_temp() {
    time_test!();

    let result = run("./tests/std/temp.gos", false);
    assert!(result.is_ok());
}
