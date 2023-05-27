// Copyright 2022 The Goscript Authors. All rights reserved.
// Use of this source code is governed by a BSD-style
// license that can be found in the LICENSE file.

use crate::vfs::VirtualFs;
use go_parser::Map;
use std::io;
use std::path::{Path, PathBuf};

const ABS_PREFIX: &str = "__abs__";

pub struct CompoundFs {
    sub_fs: Map<String, Box<dyn VirtualFs>>,
}

impl CompoundFs {
    pub fn new(sub_fs: Map<String, Box<dyn VirtualFs>>) -> Self {
        Self { sub_fs }
    }

    fn to_inner(&self, path: &Path) -> io::Result<(&Box<dyn VirtualFs>, String, PathBuf)> {
        let mut path = path.to_path_buf();
        let fs_name = path
            .components()
            .next()
            .map(|x| x.as_os_str().to_str().unwrap().to_owned())
            .ok_or(io::Error::from(io::ErrorKind::NotFound))?;
        let fs = self
            .sub_fs
            .get(&fs_name)
            .ok_or(io::Error::from(io::ErrorKind::NotFound))?;
        path = path.strip_prefix(&fs_name).unwrap().to_path_buf();
        if let Ok(p) = path.strip_prefix(ABS_PREFIX) {
            path = Path::new("/").join(p);
        }
        Ok((fs, fs_name, path))
    }

    fn to_outer(fs_name: &str, path: &Path) -> PathBuf {
        let full_path = if path.is_absolute() {
            format!("{}/{}{}", fs_name, ABS_PREFIX, path.to_string_lossy())
        } else {
            format!("{}/{}", fs_name, path.to_string_lossy())
        };
        PathBuf::from(full_path)
    }
}

impl VirtualFs for CompoundFs {
    fn read_file(&self, path: &Path) -> io::Result<String> {
        let (fs, _, path) = self.to_inner(path)?;
        fs.read_file(&path)
    }

    fn read_dir(&self, path: &Path) -> io::Result<Vec<PathBuf>> {
        let (fs, fs_name, path) = self.to_inner(path)?;
        fs.read_dir(&path).map(|x| {
            x.into_iter()
                .map(|p| Self::to_outer(&fs_name, &p))
                .collect()
        })
    }

    fn is_file(&self, path: &Path) -> bool {
        if let Ok((fs, _, path)) = self.to_inner(path) {
            fs.is_file(&path)
        } else {
            false
        }
    }

    fn is_dir(&self, path: &Path) -> bool {
        if let Ok((fs, _, path)) = self.to_inner(path) {
            fs.is_dir(&path)
        } else {
            false
        }
    }

    fn canonicalize_path(&self, path: &PathBuf) -> io::Result<PathBuf> {
        let (fs, name, inner) = self.to_inner(path)?;
        let p = fs.canonicalize_path(&inner)?;
        Ok(CompoundFs::to_outer(&name, &p))
    }

    fn strip_prefix<'a>(&'a self, path: &'a Path) -> &'a Path {
        let path_str = path.to_str().unwrap();
        let (_, suffix) = path_str.split_once('/').unwrap_or(("", path_str));
        Path::new(suffix)
    }

    fn is_local(&self, path: &str) -> bool {
        let local = |p: &str| p == "." || p == ".." || p.starts_with("./") || p.starts_with("../");
        if local(path) {
            return true;
        }
        let (_, path) = path.split_once('/').unwrap_or(("", path));
        local(path)
    }
}
