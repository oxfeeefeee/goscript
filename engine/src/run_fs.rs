// Copyright 2022 The Goscript Authors. All rights reserved.
// Use of this source code is governed by a BSD-style
// license that can be found in the LICENSE file.

/// run_fs runs an engine with a file system.
extern crate goscript_codegen as cg;
use crate::engine::Engine;
use cg::FsReader;

pub struct Config {
    /// working directory
    pub working_dir: Option<String>,
    /// base directory for non-local imports
    pub base_dir: Option<String>,
    /// print debug info in parser
    pub trace_parser: bool,
    /// print debug info in checker
    pub trace_checker: bool,
    /// proint debug info for vm
    pub trace_vm: bool,
}

pub fn run(config: Config, path: &str) -> usize {
    run_fs_impl(config, None, path)
}

pub fn run_string(config: Config, source: String) -> usize {
    run_fs_impl(config, Some(source), FsReader::temp_file_path())
}

fn run_fs_impl(config: Config, temp_source: Option<String>, path: &str) -> usize {
    let engine = Engine::new();
    let reader = FsReader::new(config.working_dir, config.base_dir, temp_source);
    engine.run(
        config.trace_parser,
        config.trace_checker,
        config.trace_vm,
        &reader,
        path,
    )
}
