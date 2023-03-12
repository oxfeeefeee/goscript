// Copyright 2022 The Goscript Authors. All rights reserved.
// Use of this source code is governed by a BSD-style
// license that can be found in the LICENSE file.

use crate::engine::{Config, Engine, SourceRead};
use crate::vfs::VirtualFs;
use crate::ErrorList;
use goscript_parser::Map;
use std::io;
use std::path::{Path, PathBuf};

const VIRTUAL_LOCAL_PATH_PREFIX: &str = "vfs_local_";

pub fn run(config: Config, source: &SourceReader, path: &Path) -> Result<(), ErrorList> {
    let engine = Engine::new();
    #[cfg(feature = "go_std")]
    engine.set_std_io(config.std_in, config.std_out, config.std_err);
    engine.run_source(config.trace_parser, config.trace_checker, source, path)
}

pub struct SourceReader {
    /// working directory
    working_dir: PathBuf,
    /// base directory for non-local imports
    base_dir: Option<PathBuf>,
    /// The virtual file system from which to read files.
    vfs: Box<dyn VirtualFs>,
}

impl SourceReader {
    pub fn new(
        working_dir: PathBuf,
        base_dir: Option<PathBuf>,
        vfs: Box<dyn VirtualFs>,
    ) -> SourceReader {
        SourceReader {
            working_dir,
            base_dir,
            vfs,
        }
    }

    #[cfg(feature = "read_fs")]
    pub fn local_fs(working_dir: PathBuf, base_dir: PathBuf) -> SourceReader {
        SourceReader::new(working_dir, Some(base_dir), Box::new(crate::VfsFs {}))
    }

    #[cfg(feature = "read_fs")]
    pub fn fs_lib_and_string(base_dir: PathBuf, source: String) -> (SourceReader, PathBuf) {
        let temp_file_name = "temp_file.gos";
        // must start with VIRTUAL_LOCAL_PATH_PREFIX to be recognized as a local(as opposed to lib file) file
        let vfs_map_name = format!("{}map", VIRTUAL_LOCAL_PATH_PREFIX);
        let vfs_fs_name = "vfs_fs";
        (
            SourceReader::new(
                PathBuf::from(format!("{}/", vfs_map_name)),
                Some(Path::new("vfs_fs").join(base_dir)),
                Box::new(crate::CompoundFs::new(Map::from([
                    (
                        vfs_fs_name.to_owned(),
                        Box::new(crate::VfsFs {}) as Box<dyn VirtualFs>,
                    ),
                    (
                        vfs_map_name.to_owned(),
                        Box::new(crate::VfsMap::new(Map::from([(
                            PathBuf::from(temp_file_name),
                            source,
                        )]))),
                    ),
                ]))),
            ),
            PathBuf::from(format!("./{}", temp_file_name)),
        )
    }

    #[cfg(feature = "read_zip")]
    pub fn zip_and_string(
        archive: Vec<u8>,
        base_dir: PathBuf,
        source: String,
    ) -> (SourceReader, PathBuf) {
        let temp_file_name = "temp_file.gos";
        // must start with VIRTUAL_LOCAL_PATH_PREFIX to be recognized as a local(as opposed to lib file) file
        let vfs_map_name = format!("{}map", VIRTUAL_LOCAL_PATH_PREFIX);
        let vfs_zip_name = "vfs_zip";
        (
            SourceReader::new(
                PathBuf::from(format!("{}/", vfs_map_name)),
                Some(Path::new("vfs_zip").join(base_dir)),
                Box::new(crate::CompoundFs::new(Map::from([
                    (
                        vfs_zip_name.to_owned(),
                        Box::new(crate::VfsZip::new(archive).unwrap()) as Box<dyn VirtualFs>,
                    ),
                    (
                        vfs_map_name.to_owned(),
                        Box::new(crate::VfsMap::new(Map::from([(
                            PathBuf::from(temp_file_name),
                            source,
                        )]))),
                    ),
                ]))),
            ),
            PathBuf::from(format!("./{}", temp_file_name)),
        )
    }
}

impl SourceRead for SourceReader {
    fn working_dir(&self) -> &Path {
        &self.working_dir
    }

    fn base_dir(&self) -> Option<&Path> {
        self.base_dir.as_ref().map(|x| x.as_path())
    }

    fn read_file(&self, path: &Path) -> io::Result<String> {
        self.vfs.read_file(path)
    }

    fn read_dir(&self, path: &Path) -> io::Result<Vec<PathBuf>> {
        self.vfs.read_dir(path)
    }

    fn is_file(&self, path: &Path) -> bool {
        self.vfs.is_file(path)
    }

    fn is_dir(&self, path: &Path) -> bool {
        self.vfs.is_dir(path)
    }

    fn canonicalize_path(&self, path: &PathBuf) -> io::Result<PathBuf> {
        self.vfs.canonicalize_path(path)
    }
}
