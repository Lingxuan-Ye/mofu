//! Tools for directory traversal.

use std::fs;
use std::fs::{Metadata, ReadDir};
use std::io::{Error, ErrorKind, Result};
use std::num::NonZero;
use std::path::{Path, PathBuf};

/// Returns an iterator that recursively traverses the specified directory.
///
/// Note that the iterator will skip any entries that produce errors. To
/// handle errors explicitly, see [`WalkDir`].
///
/// # Parameters
///
/// - `path`: The directory path to traverse.
/// - `max_depth`: Maximum depth to traverse:
///   - `0` means unlimited depth.
///   - `1` means only traverse top-level entries.
///
/// # Errors
///
/// This function will return an error in the following situations, but is not
/// limited to just these cases:
///
/// - The provided `path` doesn't exist.
/// - The process lacks permissions to view the contents.
/// - The `path` points at a non-directory file.
pub fn walk_dir<P>(path: P, max_depth: usize) -> Result<impl Iterator<Item = DirEntry>>
where
    P: AsRef<Path>,
{
    let max_depth = NonZero::new(max_depth);
    let iter = WalkDir::new(path)?
        .max_depth(max_depth)
        .filter_map(Result::ok);
    Ok(iter)
}

/// An iterator that recursively traverses the specified directory.
///
/// # Examples
///
/// ```
/// use mofu::walk_dir::WalkDir;
/// use std::num::NonZero;
///
/// let max_depth = NonZero::new(3);
/// let iter = WalkDir::new(".")
///     .unwrap()
///     .max_depth(max_depth)
///     .filter_map(Result::ok);
/// for entry in iter {
///     println!("{}", entry.path().display());
/// }
/// ```
#[derive(Debug)]
pub struct WalkDir {
    stack: Vec<StackItem>,
    max_depth: Option<NonZero<usize>>,
}

impl WalkDir {
    /// Creates a new [`WalkDir`].
    ///
    /// # Errors
    ///
    /// This function will return an error in the following situations, but is not
    /// limited to just these cases:
    ///
    /// - The provided `path` doesn't exist.
    /// - The process lacks permissions to view the contents.
    /// - The `path` points at a non-directory file.
    pub fn new<P>(path: P) -> Result<Self>
    where
        P: AsRef<Path>,
    {
        let depth = unsafe { NonZero::new_unchecked(1) };
        let iter = fs::read_dir(path)?;
        let item = StackItem { depth, iter };
        let stack = vec![item];
        let max_depth = None;
        Ok(Self { stack, max_depth })
    }

    /// Sets the maximum depth for traversal.
    pub fn max_depth(mut self, max_depth: Option<NonZero<usize>>) -> Self {
        self.max_depth = max_depth;
        self
    }
}

impl Iterator for WalkDir {
    type Item = Result<DirEntry>;

    fn next(&mut self) -> Option<Self::Item> {
        let (depth, entry) = loop {
            let item = self.stack.last_mut()?;
            match item.iter.next() {
                None => self.stack.pop(),
                Some(Err(error)) => return Some(Err(error)),
                Some(Ok(entry)) => break (item.depth, entry),
            };
        };

        let entry = match DirEntry::try_from(entry) {
            Err(error) => return Some(Err(error)),
            Ok(entry) => entry,
        };

        if entry.metadata.is_dir() && self.max_depth.is_none_or(|max_depth| depth < max_depth) {
            match fs::read_dir(entry.path()) {
                // Yes, this branch is still reachable.
                Err(error) if error.kind() == ErrorKind::NotADirectory => (),
                Err(error) => return Some(Err(error)),
                Ok(iter) => {
                    // Will not overflow because `depth < max_depth`.
                    let depth = unsafe { NonZero::new_unchecked(depth.get() + 1) };
                    let item = StackItem { depth, iter };
                    self.stack.push(item);
                }
            }
        }

        Some(Ok(entry))
    }
}

/// A directory entry returned by [`WalkDir`].
///
/// Each entry provides a path along with cached metadata to help avoid
/// redundant system calls. However, due to possible concurrent file access,
/// the cached metadata may degrade in validity over time.
///
/// Note that the metadata does not follow symlinks.
#[derive(Debug)]
pub struct DirEntry {
    path: PathBuf,
    metadata: Metadata,
}

impl DirEntry {
    /// Returns the path.
    #[inline]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Returns the cached metadata.
    ///
    /// Due to possible concurrent file access, the cached metadata may degrade in
    /// validity over time.
    ///
    /// Note that the metadata does not follow symbolic links.
    #[inline]
    pub fn metadata(&self) -> &Metadata {
        &self.metadata
    }
}

impl TryFrom<fs::DirEntry> for DirEntry {
    type Error = Error;

    #[inline]
    fn try_from(value: fs::DirEntry) -> Result<Self> {
        let path = value.path();
        let metadata = value.metadata()?;
        Ok(Self { path, metadata })
    }
}

impl TryFrom<PathBuf> for DirEntry {
    type Error = Error;

    #[inline]
    fn try_from(value: PathBuf) -> Result<Self> {
        let path = value;
        let metadata = path.symlink_metadata()?;
        Ok(Self { path, metadata })
    }
}

impl From<DirEntry> for PathBuf {
    #[inline]
    fn from(value: DirEntry) -> Self {
        value.path
    }
}

#[derive(Debug)]
struct StackItem {
    depth: NonZero<usize>,
    iter: ReadDir,
}
