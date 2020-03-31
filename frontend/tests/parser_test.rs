extern crate goscript_frontend as fe;
use std::fs;

fn load_parse(path: &str, trace: bool) -> usize {
    let mut fs = fe::FileSet::new();
    let src = fs::read_to_string(path).expect("read file err: ");
    let o = &mut fe::ast_objects::Objects::new();
    let (p, _) = fe::parse_file(o, &mut fs, path, &src, trace);

    print!("{}", p.get_errors());

    let l = p.get_errors().len();
    l
}

#[test]
fn test_parser_case0() {
    load_parse(
        "./../../../../go/src/github.com/golang/go/src/archive/tar/strconv_test.go",
        true,
    );
}

#[test]
fn test_parser_case1() {
    let err_cnt = load_parse("./tests/data/case1.gos", true);
    dbg!(err_cnt);
}

fn parse_dir(s: &str, trace: bool) -> usize {
    let mut total = 0;
    let paths = fs::read_dir(s).unwrap();
    for path in paths {
        let p0 = path.unwrap().path();
        if p0.is_dir() {
            total += parse_dir(p0.to_str().unwrap(), trace);
        }
        let p = p0.to_str().unwrap();
        if p.ends_with(".go") {
            println!("Name: {}", p);
            total += load_parse(p, trace);
        }
    }
    total
}

#[test]
fn test_parser_dir() {
    let t = parse_dir("./../../../../go/src/github.com/golang/go/src", false);
    //let t = parse_dir("./../../../../go/src/github.com/ethereum", false);
    println!("hohohoh{}", t);
}
