// Copyright 2022 The Goscript Authors. All rights reserved.
// Use of this source code is governed by a BSD-style
// license that can be found in the LICENSE file.

/// run_fs runs an engine with a file system.
use crate::engine::Engine;
use crate::ErrorList;
use goscript_codegen::FsReader;

pub struct Config<'a> {
    /// working directory
    pub working_dir: Option<&'a str>,
    /// base directory for non-local imports
    pub base_dir: Option<&'a str>,
    /// print debug info in parser
    pub trace_parser: bool,
    /// print debug info in checker
    pub trace_checker: bool,
}

pub fn run(config: Config, path: &str) -> Result<(), ErrorList> {
    run_fs_impl(config, None, path)
}

pub fn run_string(config: Config, source: &str) -> Result<(), ErrorList> {
    run_fs_impl(config, Some(source), FsReader::temp_file_path())
}

fn run_fs_impl(config: Config, temp_source: Option<&str>, path: &str) -> Result<(), ErrorList> {
    let engine = Engine::new();
    let reader = FsReader::new(config.working_dir, config.base_dir, temp_source);
    engine.run(config.trace_parser, config.trace_checker, &reader, path)
}
