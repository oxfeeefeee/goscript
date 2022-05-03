// Copyright 2022 The Goscript Authors. All rights reserved.
// Use of this source code is governed by a BSD-style
// license that can be found in the LICENSE file.

use criterion::{criterion_group, criterion_main, Criterion};

extern crate goscript_engine as engine;

fn run(path: &str, trace: bool) -> Result<(), engine::ErrorList> {
    let cfg = engine::run_fs::Config {
        working_dir: Some("./"),
        base_dir: Some("./std/"),
        trace_parser: trace,
        trace_checker: trace,
    };
    engine::run_fs::run(cfg, path)
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
