// Copyright 2022 The Goscript Authors. All rights reserved.
// Use of this source code is governed by a BSD-style
// license that can be found in the LICENSE file.

use crate::vfs::VirtualFs;
use std::cell::RefCell;
use std::io;
use std::io::Read;
use std::path::Path;
use std::path::PathBuf;
use std::vec;
use zip::read::ZipArchive;
use zip::result::{ZipError, ZipResult};

pub struct VfsZip {
    archive: RefCell<ZipArchive<io::Cursor<Vec<u8>>>>,
}

impl VfsZip {
    pub fn new(archive: Vec<u8>) -> ZipResult<VfsZip> {
        let c = io::Cursor::new(archive);
        let archive = RefCell::new(ZipArchive::new(c)?);
        Ok(VfsZip { archive })
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

impl VirtualFs for VfsZip {
    fn read_file(&self, path: &Path) -> io::Result<String> {
        let mut borrow = self.archive.borrow_mut();
        let re = borrow.by_name(path.to_str().unwrap());
        let mut file = VfsZip::convert_err(re)?;
        let mut buf = String::new();
        file.read_to_string(&mut buf)?;
        Ok(buf)
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
