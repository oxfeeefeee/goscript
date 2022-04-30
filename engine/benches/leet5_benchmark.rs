// Copyright 2022 The Goscript Authors. All rights reserved.
// Use of this source code is governed by a BSD-style
// license that can be found in the LICENSE file.

use criterion::{criterion_group, criterion_main, Criterion};

extern crate goscript_engine as engine;

fn run(path: &str, trace: bool) -> usize {
    let cfg = engine::Config {
        work_dir: Some("./".to_owned()),
        base_path: Some("./std/".to_owned()),
        trace_parser: trace,
        trace_checker: trace,
        trace_vm: true,
    };
    let mut engine = engine::Engine::new();
    engine.run(cfg, path)
}

fn leetcode5() {
    let err_cnt = run("./tests/demo/leetcode5.gos", false);
    assert!(err_cnt == 0);
}

pub fn criterion_benchmark(c: &mut Criterion) {
    c.bench_function("leet5", |b| b.iter(|| leetcode5()));
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
