use std::fs;
use std::fs::{DirEntry, FileType, Metadata};
use std::io;
use std::marker::PhantomData;
use std::path::{Path, PathBuf};

/// Returns a [`WalkDir`] that recursively traverses all entries
/// in `dir`, skipping errors.
///
/// # Errors
///
/// This function will return an error in the following situations,
/// but is not limited to just these cases:
///
/// - The provided path doesn't exist.
/// - The process lacks permissions to view the contents.
/// - The path points at a non-directory file.
pub fn walk_dir<P>(dir: P) -> Result<WalkDir<SkipError>, io::Error>
where
    P: AsRef<Path>,
{
    WalkDir::new(dir)
}

/// An iterator that recursively traverses all entries in a directory.
#[derive(Debug)]
pub struct WalkDir<Policy> {
    stack: Vec<Result<DirEntry, io::Error>>,
    policy: PhantomData<Policy>,
}

/// A [`WalkDir`] policy that keeps errors during iteration.
#[derive(Debug)]
pub struct KeepError;

/// A [`WalkDir`] policy that skips errors during iteration.
#[derive(Debug)]
pub struct SkipError;

/// A directory entry returned by [`WalkDir`].
///
/// Each entry provides a path along with cached metadata to help avoid
/// redundant system calls. However, due to possible concurrent file
/// access, the cached metadata may degrade in validity over time.
pub struct Entry {
    path: PathBuf,
    metadata: Metadata,
}

impl<Policy> WalkDir<Policy> {
    /// Returns a [`WalkDir`] that recursively traverses all entries
    /// in `dir`.
    ///
    /// # Available Policies
    ///
    /// - [`KeepError`]
    /// - [`SkipError`]
    ///
    /// # Errors
    ///
    /// This function will return an error in the following situations,
    /// but is not limited to just these cases:
    ///
    /// - The provided path doesn't exist.
    /// - The process lacks permissions to view the contents.
    /// - The path points at a non-directory file.
    pub fn new<P>(dir: P) -> Result<Self, io::Error>
    where
        P: AsRef<Path>,
    {
        Ok(Self {
            stack: fs::read_dir(dir)?.collect(),
            policy: PhantomData,
        })
    }
}

impl Iterator for WalkDir<KeepError> {
    type Item = Result<Entry, io::Error>;

    fn next(&mut self) -> Option<Self::Item> {
        let entry = match self.stack.pop()? {
            Err(error) => return Some(Err(error)),
            Ok(entry) => entry,
        };
        let path = entry.path();
        let metadata = match fs::symlink_metadata(&path) {
            Err(error) => return Some(Err(error)),
            Ok(metadata) => metadata,
        };
        // Do not use `DirEntry::file_type` here. Although it does not
        // follow symlinks, it may be a cached value that is out of date.
        // In that case, the path passed to `fs::read_dir` may still be
        // a symlink.
        if metadata.is_dir() {
            match fs::read_dir(&path) {
                // Yes, this branch is reachable.
                Err(error) if error.kind() == io::ErrorKind::NotADirectory => (),
                Err(error) => return Some(Err(error)),
                Ok(iter) => self.stack.extend(iter),
            }
        }
        Some(Ok(Entry { path, metadata }))
    }
}

impl Iterator for WalkDir<SkipError> {
    type Item = Entry;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let entry = match self.stack.pop()? {
                Err(_) => continue,
                Ok(entry) => entry,
            };
            let path = entry.path();
            let metadata = match fs::symlink_metadata(&path) {
                Err(_) => continue,
                Ok(metadata) => metadata,
            };
            if metadata.is_dir() {
                match fs::read_dir(&path) {
                    Err(error) if error.kind() == io::ErrorKind::NotADirectory => (),
                    Err(_) => continue,
                    Ok(iter) => self.stack.extend(iter),
                }
            }
            return Some(Entry { path, metadata });
        }
    }
}

impl Entry {
    /// Returns the path of this entry.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Returns the cached metadata for this entry.
    ///
    /// Due to possible concurrent file access, the cached metadata
    /// may degrade in validity over time. In addition, it does not
    /// follow symlinks.
    pub fn metadata(&self) -> &Metadata {
        &self.metadata
    }

    /// Returns the cached file type of this entry.
    ///
    /// This is derived from the cached metadata, which may degrade
    /// in validity over time.
    pub fn file_type(&self) -> FileType {
        self.metadata.file_type()
    }
}

impl From<Entry> for PathBuf {
    fn from(value: Entry) -> Self {
        value.path
    }
}
