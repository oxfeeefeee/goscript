#![no_main]
use arbitrary::Arbitrary;
use goscript_parser::errors::ErrorList;
use goscript_parser::objects::Objects;
use goscript_parser::{parse_file, FileSet};
use libfuzzer_sys::fuzz_target;

#[derive(Arbitrary, Debug)]
struct Input {
    src: String,
    trace: bool,
}

fuzz_target!(|input: Input| {
    let mut fs = FileSet::new();
    let o = &mut Objects::new();
    let el = &mut ErrorList::new();
    let _ = parse_file(o, &mut fs, el, "/a", &input.src, input.trace);
});
