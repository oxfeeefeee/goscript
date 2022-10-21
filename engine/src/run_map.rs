// Copyright 2022 The Goscript Authors. All rights reserved.
// Use of this source code is governed by a BSD-style
// license that can be found in the LICENSE file.

/// run_map runs an engine with a Map containning code.
use crate::engine::{Engine, SourceRead};
use crate::ErrorList;
use goscript_parser::Map;
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
    /// lib extensions
    pub extensions: Option<Vec<Box<dyn FnOnce(&mut Engine)>>>,
}

pub fn run(map: &Map<&Path, String>, config: Config, path: &str) -> Result<(), ErrorList> {
    run_map_impl(map, config, None, path)
}

pub fn run_string(map: &Map<&Path, String>, config: Config, source: &str) -> Result<(), ErrorList> {
    run_map_impl(map, config, Some(source), MapReader::temp_file_path())
}

fn run_map_impl(
    map: &Map<&Path, String>,
    config: Config,
    temp_source: Option<&str>,
    path: &str,
) -> Result<(), ErrorList> {
    let mut engine = Engine::new();
    if let Some(exts) = config.extensions {
        for ext in exts.into_iter() {
            ext(&mut engine);
        }
    }
    let reader = MapReader::new(map, config.working_dir, config.base_dir, temp_source);
    engine.run_source(config.trace_parser, config.trace_checker, &reader, path)
}

pub struct MapReader<'a> {
    map: &'a Map<&'a Path, String>,
    working_dir: Option<&'a str>,
    base_dir: Option<&'a str>,
    temp_file: Option<&'a str>,
}

impl<'a> MapReader<'a> {
    fn new(
        map: &'a Map<&'a Path, String>,
        working_dir: Option<&'a str>,
        base_dir: Option<&'a str>,
        temp_file: Option<&'a str>,
    ) -> MapReader<'a> {
        MapReader {
            map,
            working_dir,
            base_dir,
            temp_file,
        }
    }

    fn temp_file_path() -> &'static str {
        &"temp_file_in_memory_for_testing_and_you_can_only_have_one____from_MapReader.gos"
    }

    fn is_temp_file(p: &Path) -> bool {
        p.to_string_lossy().ends_with(Self::temp_file_path())
    }
}

impl<'a> SourceRead for MapReader<'a> {
    fn working_dir(&self) -> io::Result<PathBuf> {
        Ok(Path::new(match self.working_dir {
            Some(p) => p,
            None => "working_dir/",
        })
        .to_path_buf())
    }

    fn base_dir(&self) -> Option<&str> {
        self.base_dir
    }

    fn read_file(&self, path: &Path) -> io::Result<String> {
        let file = if Self::is_temp_file(path) {
            self.temp_file
        } else {
            self.map.get(path).map(|x| x.as_str())
        };
        file.map(|x| x.to_owned())
            .ok_or(io::Error::from(io::ErrorKind::NotFound))
    }

    fn read_dir(&self, path: &Path) -> io::Result<Vec<PathBuf>> {
        let result: Vec<PathBuf> = self
            .map
            .iter()
            .filter_map(|(p, _)| p.starts_with(path).then(|| p.to_path_buf()))
            .collect();
        if result.is_empty() {
            Err(io::Error::from(io::ErrorKind::NotFound))
        } else {
            Ok(result)
        }
    }

    fn is_file(&self, path: &Path) -> bool {
        path.extension().is_some()
    }

    fn is_dir(&self, path: &Path) -> bool {
        path.extension().is_none()
    }

    fn canonicalize_path(&self, path: &PathBuf) -> io::Result<PathBuf> {
        Ok(path.clone())
    }
}
