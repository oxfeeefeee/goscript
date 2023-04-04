// Copyright 2022 The Goscript Authors. All rights reserved.
// Use of this source code is governed by a BSD-style
// license that can be found in the LICENSE file.

#![no_main]
use arbitrary::Arbitrary;
use go_parser::errors::ErrorList;
use go_parser::objects::Objects;
use go_parser::{parse_file, FileSet};
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
