use std::fs;
use std::fs::{DirEntry, FileType, Metadata};
use std::io::{Error, ErrorKind};
use std::path::{Path, PathBuf};

pub fn walk_dir<P>(dir: P) -> Result<WalkDir, Error>
where
    P: AsRef<Path>,
{
    WalkDir::new(dir)
}

#[derive(Debug)]
pub struct WalkDir(Vec<Result<DirEntry, Error>>);

pub struct Entry {
    path: PathBuf,
    metadata: Metadata,
}

impl WalkDir {
    pub fn new<P>(dir: P) -> Result<Self, Error>
    where
        P: AsRef<Path>,
    {
        Ok(Self(fs::read_dir(dir)?.collect()))
    }
}

impl Iterator for WalkDir {
    type Item = Result<Entry, Error>;

    fn next(&mut self) -> Option<Self::Item> {
        let entry = match self.0.pop()? {
            Err(error) => return Some(Err(error)),
            Ok(entry) => entry,
        };
        let path = entry.path();
        let metadata = match fs::symlink_metadata(&path) {
            Err(error) => return Some(Err(error)),
            Ok(metadata) => metadata,
        };
        // Do not use `fs::DirEntry::file_type` here. Although it does not
        // follow symlinks, it may be a cached value that is out of date.
        // In that case, the path passed to `fs::read_dir` may still be a
        // symlink.
        if metadata.is_dir() {
            match fs::read_dir(&path) {
                // Yes, this branch is reachable.
                Err(error) if error.kind() == ErrorKind::NotADirectory => (),
                Err(error) => return Some(Err(error)),
                Ok(iter) => self.0.extend(iter),
            }
        }
        Some(Ok(Entry { path, metadata }))
    }
}

impl Entry {
    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn metadata(&self) -> &Metadata {
        &self.metadata
    }

    pub fn file_type(&self) -> FileType {
        self.metadata.file_type()
    }

    pub fn into_path_buf(self) -> PathBuf {
        self.path
    }
}
