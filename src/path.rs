//! Types and trait for file paths.

pub use std::path::{Path, PathBuf};

/// A trait representing a path.
pub trait AsPath {
    /// Returns a shared reference to a [`Path`] slice.
    fn as_path(&self) -> &Path;

    /// Returns `true` if the path points to a directory.
    ///
    /// Note that this will be `false` for symbolic links.
    fn is_dir(&self) -> bool;

    /// Returns `true` if the path points to a regular file.
    ///
    /// Note that this will be `false` for symbolic links.
    fn is_file(&self) -> bool;

    /// Returns `true` if the path points to a symbolic link.
    fn is_symlink(&self) -> bool;
}

impl<P> AsPath for P
where
    P: AsRef<Path>,
{
    fn as_path(&self) -> &Path {
        self.as_ref()
    }

    fn is_dir(&self) -> bool {
        self.as_ref()
            .symlink_metadata()
            .map(|metadata| metadata.is_dir())
            .unwrap_or(false)
    }

    fn is_file(&self) -> bool {
        self.as_ref()
            .symlink_metadata()
            .map(|metadata| metadata.is_file())
            .unwrap_or(false)
    }

    fn is_symlink(&self) -> bool {
        self.as_ref().is_symlink()
    }
}
