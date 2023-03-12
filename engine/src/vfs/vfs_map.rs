// Copyright 2022 The Goscript Authors. All rights reserved.
// Use of this source code is governed by a BSD-style
// license that can be found in the LICENSE file.

use crate::vfs::VirtualFs;
use goscript_parser::Map;
use std::io;
use std::path::{Path, PathBuf};

pub struct VfsMap {
    map: Map<PathBuf, String>,
}

impl VfsMap {
    pub fn new(map: Map<PathBuf, String>) -> VfsMap {
        VfsMap { map }
    }
}

impl VirtualFs for VfsMap {
    fn read_file(&self, path: &Path) -> io::Result<String> {
        self.map
            .get(path)
            .map(|x| x.as_str().to_owned())
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
