//! Types and trait for file paths.

pub use std::path::{Path, PathBuf};

/// A trait representing a path.
pub trait AsPath {
    /// Returns a shared reference to a [`Path`] slice.
    fn as_path(&self) -> &Path;

    /// Returns `true` if the path exists.
    ///
    /// Note that this does not follow symbolic links.
    fn exists(&self) -> bool {
        self.as_path().symlink_metadata().is_ok()
    }

    /// Returns `true` if the path is pointing at a directory.
    ///
    /// Note that this does not follow symbolic links.
    fn is_dir(&self) -> bool {
        self.as_path()
            .symlink_metadata()
            .map(|metadata| metadata.is_dir())
            .unwrap_or(false)
    }

    /// Returns `true` if the path is pointing at a regular file.
    ///
    /// Note that this does not follow symbolic links.
    fn is_file(&self) -> bool {
        self.as_path()
            .symlink_metadata()
            .map(|metadata| metadata.is_file())
            .unwrap_or(false)
    }

    /// Returns `true` if the path is pointing at a symbolic link.
    fn is_symlink(&self) -> bool {
        self.as_path().is_symlink()
    }
}

impl<P> AsPath for P
where
    P: AsRef<Path>,
{
    fn as_path(&self) -> &Path {
        self.as_ref()
    }
}
