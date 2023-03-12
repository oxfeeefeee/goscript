// Copyright 2022 The Goscript Authors. All rights reserved.
// Use of this source code is governed by a BSD-style
// license that can be found in the LICENSE file.

use crate::vfs::VirtualFs;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

pub struct VfsFs {}

impl VirtualFs for VfsFs {
    fn read_file(&self, path: &Path) -> io::Result<String> {
        fs::read_to_string(path)
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
        path.is_file()
    }

    fn is_dir(&self, path: &Path) -> bool {
        path.is_dir()
    }

    fn canonicalize_path(&self, path: &PathBuf) -> io::Result<PathBuf> {
        if !path.exists() {
            Err(io::Error::from(io::ErrorKind::NotFound))
        } else {
            path.canonicalize()
        }
    }
}
