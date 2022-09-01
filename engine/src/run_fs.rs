// Copyright 2022 The Goscript Authors. All rights reserved.
// Use of this source code is governed by a BSD-style
// license that can be found in the LICENSE file.

/// run_fs runs an engine with a file system.
use crate::engine::{Engine, SourceRead};
use crate::ErrorList;
use std::env;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

#[derive(Default)]
pub struct Config<'a> {
    /// working directory
    pub working_dir: Option<&'a str>,
    /// base directory for non-local imports
    pub base_dir: Option<&'a str>,
    /// print debug info in parser
    pub trace_parser: bool,
    /// print debug info in checker
    pub trace_checker: bool,
    /// custom std in
    pub std_in: Option<Box<dyn std::io::Read + Sync + Send>>,
    /// custom std out
    pub std_out: Option<Box<dyn std::io::Write + Sync + Send>>,
    /// custom std err
    pub std_err: Option<Box<dyn std::io::Write + Sync + Send>>,
}

pub fn run(config: Config, path: &str) -> Result<(), ErrorList> {
    run_fs_impl(config, None, path)
}

pub fn run_string(config: Config, source: &str) -> Result<(), ErrorList> {
    run_fs_impl(config, Some(source), FsReader::temp_file_path())
}

fn run_fs_impl(config: Config, temp_source: Option<&str>, path: &str) -> Result<(), ErrorList> {
    let engine = Engine::new();
    engine.set_std_io(config.std_in, config.std_out, config.std_err);
    let reader = FsReader::new(config.working_dir, config.base_dir, temp_source);
    engine.run(config.trace_parser, config.trace_checker, &reader, path)
}

pub struct FsReader<'a> {
    working_dir: Option<&'a str>,
    base_dir: Option<&'a str>,
    temp_file: Option<&'a str>,
}

impl<'a> FsReader<'a> {
    pub fn new(
        working_dir: Option<&'a str>,
        base_dir: Option<&'a str>,
        temp_file: Option<&'a str>,
    ) -> FsReader<'a> {
        FsReader {
            working_dir,
            base_dir,
            temp_file,
        }
    }

    pub fn temp_file_path() -> &'static str {
        &"./temp_file_in_memory_for_testing_and_you_can_only_have_one.gos"
    }
}

impl<'a> SourceRead for FsReader<'a> {
    fn working_dir(&self) -> io::Result<PathBuf> {
        if let Some(wd) = &self.working_dir {
            let mut buf = PathBuf::new();
            buf.push(wd);
            Ok(buf)
        } else {
            env::current_dir()
        }
    }

    fn base_dir(&self) -> Option<&str> {
        self.base_dir
    }

    fn read_file(&self, path: &Path) -> io::Result<String> {
        if path.ends_with(Self::temp_file_path()) {
            self.temp_file
                .map(|x| x.to_owned())
                .ok_or(io::Error::from(io::ErrorKind::NotFound))
        } else {
            fs::read_to_string(path)
        }
    }

    fn read_dir(&self, path: &Path) -> io::Result<Vec<PathBuf>> {
        Ok(fs::read_dir(path)?
            .filter_map(|x| {
                x.map_or(None, |e| {
                    let path = e.path();
                    (!path.is_dir()).then(|| path)
                })
            })
            .collect())
    }

    fn is_file(&self, path: &Path) -> bool {
        if path.ends_with(Self::temp_file_path()) {
            true
        } else {
            path.is_file()
        }
    }

    fn is_dir(&self, path: &Path) -> bool {
        path.is_dir()
    }

    fn canonicalize_path(&self, path: &PathBuf) -> io::Result<PathBuf> {
        if path.ends_with(Self::temp_file_path()) {
            Ok(path.clone())
        } else if !path.exists() {
            Err(io::Error::from(io::ErrorKind::NotFound))
        } else {
            path.canonicalize()
        }
    }
}
