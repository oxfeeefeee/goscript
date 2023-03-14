// Copyright 2022 The Goscript Authors. All rights reserved.
// Use of this source code is governed by a BSD-style
// license that can be found in the LICENSE file.

use criterion::{criterion_group, criterion_main, Criterion};

extern crate goscript_engine as engine;
use std::path::{Path, PathBuf};

#[cfg(feature = "go_std")]
fn run(path: &str, trace: bool) -> Result<(), engine::ErrorList> {
    let mut cfg = engine::Config::default();
    cfg.trace_parser = trace;
    cfg.trace_checker = trace;
    let sr = engine::SourceReader::local_fs(PathBuf::from("../std/"), PathBuf::from("./"));
    engine::run(cfg, &sr, Path::new(path))
}

#[cfg(not(feature = "go_std"))]
fn run(_path: &str, _trace: bool) -> Result<(), engine::ErrorList> {
    unimplemented!()
}

fn leetcode5() {
    let errs = run("./tests/demo/leetcode5.gos", false);
    assert!(errs.is_ok());
}

pub fn criterion_benchmark(c: &mut Criterion) {
    c.bench_function("leet5", |b| b.iter(|| leetcode5()));
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
