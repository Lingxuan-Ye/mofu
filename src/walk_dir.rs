//! Tools for directory traversal.

use crate::path::{AsPath, Path, PathBuf};
use std::fs::{self, DirEntry, Metadata};
use std::io::{Error, ErrorKind, Result};
use std::num::NonZero;

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
pub fn walk_dir<P>(path: P, max_depth: usize) -> Result<impl Iterator<Item = Entry>>
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
        P: AsPath,
    {
        let path = path.as_path();
        let stack = fs::read_dir(path)?.map(StackItem::from_top_level).collect();
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
    type Item = Result<Entry>;

    fn next(&mut self) -> Option<Self::Item> {
        let item = self.stack.pop()?;
        let entry = match item.entry {
            Err(error) => return Some(Err(error)),
            Ok(entry) => entry,
        };
        let entry = match Entry::try_from(entry.path()) {
            Err(error) => return Some(Err(error)),
            Ok(entry) => entry,
        };
        if entry.is_dir()
            && self
                .max_depth
                .is_none_or(|max_depth| item.depth < max_depth)
        {
            match fs::read_dir(entry.path()) {
                // Yes, this branch is still reachable.
                Err(error) if error.kind() == ErrorKind::NotADirectory => (),
                Err(error) => return Some(Err(error)),
                Ok(iter) => {
                    self.stack.extend(iter.map(|entry| {
                        // Will not wrap around because `item.depth < max_depth`.
                        let depth = item.depth.get() + 1;
                        let depth = unsafe { NonZero::new_unchecked(depth) };
                        StackItem { depth, entry }
                    }));
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
/// Note that the metadata does not follow symbolic links.
#[derive(Debug)]
pub struct Entry {
    path: PathBuf,
    metadata: Metadata,
}

impl Entry {
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

    /// Returns `true` if the cached metadata is for a directory.
    ///
    /// Note that this will be `false` for symbolic links.
    #[inline]
    pub fn is_dir(&self) -> bool {
        self.metadata.is_dir()
    }

    /// Returns `true` if the cached metadata is for a regular file.
    ///
    /// Note that this will be `false` for symbolic links.
    #[inline]
    pub fn is_file(&self) -> bool {
        self.metadata.is_file()
    }

    /// Returns `true` if the cached metadata is for a symbolic link.
    #[inline]
    pub fn is_symlink(&self) -> bool {
        self.metadata.is_symlink()
    }
}

impl TryFrom<PathBuf> for Entry {
    type Error = Error;

    #[inline]
    fn try_from(value: PathBuf) -> Result<Self> {
        let path = value;
        let metadata = path.symlink_metadata()?;
        Ok(Self { path, metadata })
    }
}

impl From<Entry> for PathBuf {
    #[inline]
    fn from(value: Entry) -> Self {
        value.path
    }
}

impl AsPath for Entry {
    #[inline]
    fn as_path(&self) -> &Path {
        self.path()
    }

    /// Returns `true` if the cached metadata is for a directory.
    ///
    /// Note that this will be `false` for symbolic links.
    #[inline]
    fn is_dir(&self) -> bool {
        self.is_dir()
    }

    /// Returns `true` if the cached metadata is for a regular file.
    ///
    /// Note that this will be `false` for symbolic links.
    #[inline]
    fn is_file(&self) -> bool {
        self.is_file()
    }

    /// Returns `true` if the cached metadata is for a symbolic link.
    #[inline]
    fn is_symlink(&self) -> bool {
        self.is_symlink()
    }
}

#[derive(Debug)]
struct StackItem {
    depth: NonZero<usize>,
    entry: Result<DirEntry>,
}

impl StackItem {
    fn from_top_level(entry: Result<DirEntry>) -> Self {
        let depth = unsafe { NonZero::new_unchecked(1) };
        Self { depth, entry }
    }
}
