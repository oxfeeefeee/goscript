use std::io;
use std::path::{Path, PathBuf};

pub(crate) mod compound;
pub(crate) mod vfs_map;

#[cfg(feature = "read_fs")]
pub(crate) mod vfs_fs;
#[cfg(feature = "read_zip")]
pub(crate) mod vfs_zip;

pub trait VirtualFs {
    fn read_file(&self, path: &Path) -> io::Result<String>;

    fn read_dir(&self, path: &Path) -> io::Result<Vec<PathBuf>>;

    fn is_file(&self, path: &Path) -> bool;

    fn is_dir(&self, path: &Path) -> bool;

    fn canonicalize_path(&self, path: &PathBuf) -> io::Result<PathBuf>;

    fn is_local(&self, path: &str) -> bool {
        path == "." || path == ".." || path.starts_with("./") || path.starts_with("../")
    }

    fn strip_prefix<'a>(&'a self, path: &'a Path) -> &'a Path {
        path
    }
}
