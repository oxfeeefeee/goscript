// Copyright 2022 The Goscript Authors. All rights reserved.
// Use of this source code is governed by a BSD-style
// license that can be found in the LICENSE file.

/// run_zip runs an engine with a zip file.
use crate::Engine;
use crate::ErrorList;
use crate::SourceRead;
use std::cell::RefCell;
use std::io;
use std::path::Path;
use std::vec;
use std::{io::prelude::*, path::PathBuf};
use zip::read::ZipArchive;
use zip::result::{ZipError, ZipResult};

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

pub fn run(archive: &[u8], config: Config, path: &str) -> Result<(), ErrorList> {
    run_zip_impl(archive, config, None, path)
}

pub fn run_string(archive: &[u8], config: Config, source: &str) -> Result<(), ErrorList> {
    run_zip_impl(archive, config, Some(source), ZipReader::temp_file_path())
}

fn run_zip_impl(
    archive: &[u8],
    config: Config,
    temp_source: Option<&str>,
    path: &str,
) -> Result<(), ErrorList> {
    let engine = Engine::new();
    engine.set_std_io(config.std_in, config.std_out, config.std_err);
    match ZipReader::new(archive, config.working_dir, config.base_dir, temp_source) {
        Ok(reader) => engine.run(config.trace_parser, config.trace_checker, &reader, path),
        Err(e) => {
            let err = ErrorList::new();
            err.add(None, format!("{}", e), false, false);
            Err(err)
        }
    }
}

pub struct ZipReader<'a> {
    archive: RefCell<ZipArchive<io::Cursor<&'a [u8]>>>,
    working_dir: Option<&'a str>,
    base_dir: Option<&'a str>,
    temp_file: Option<&'a str>,
}

impl<'a> ZipReader<'a> {
    fn new(
        archive: &'a [u8],
        working_dir: Option<&'a str>,
        base_dir: Option<&'a str>,
        temp_file: Option<&'a str>,
    ) -> ZipResult<ZipReader<'a>> {
        let c = io::Cursor::new(archive);
        let archive = RefCell::new(ZipArchive::new(c)?);
        Ok(ZipReader {
            archive,
            working_dir,
            base_dir,
            temp_file,
        })
    }

    fn temp_file_path() -> &'static str {
        &"temp_file_in_memory_for_testing_and_you_can_only_have_one_ZipReader.gos"
    }

    fn is_temp_file(p: &Path) -> bool {
        p.to_string_lossy().ends_with(Self::temp_file_path())
    }

    fn convert_err<T>(err: ZipResult<T>) -> io::Result<T> {
        match err {
            Ok(v) => Ok(v),
            Err(e) => match e {
                ZipError::Io(ioe) => Err(ioe),
                _ => Err(io::Error::new(io::ErrorKind::Other, format!("{}", e))),
            },
        }
    }
}

impl<'a> SourceRead for ZipReader<'a> {
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
        if Self::is_temp_file(path) {
            self.temp_file
                .map(|x| x.to_owned())
                .ok_or(io::Error::from(io::ErrorKind::NotFound))
        } else {
            let mut borrow = self.archive.borrow_mut();
            let re = borrow.by_name(path.to_str().unwrap());
            let mut file = ZipReader::convert_err(re)?;
            let mut buf = String::new();
            file.read_to_string(&mut buf)?;
            Ok(buf)
        }
    }

    fn read_dir(&self, path: &Path) -> io::Result<Vec<PathBuf>> {
        let mut result = vec![];
        let len = self.archive.borrow().len();
        for i in 0..len {
            let mut borrow = self.archive.borrow_mut();
            let file = borrow.by_index(i)?;
            let fp = Path::new(file.name());
            if fp.starts_with(path)
                && !fp
                    .strip_prefix(path)
                    .unwrap()
                    .to_string_lossy()
                    .contains("/")
            {
                result.push(fp.to_path_buf())
            }
        }
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
