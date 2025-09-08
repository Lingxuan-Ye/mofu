use std::fs;
use std::fs::DirEntry;
use std::io::{Error, ErrorKind};
use std::path::{Path, PathBuf};

pub fn walk_dir<P>(
    dir: P,
    yield_file: bool,
    yield_dir: bool,
    yield_symlink: bool,
) -> Result<WalkDir, Error>
where
    P: AsRef<Path>,
{
    let mut iter = WalkDir::new(dir)?;
    iter.yield_file = yield_file;
    iter.yield_dir = yield_dir;
    iter.yield_symlink = yield_symlink;
    Ok(iter)
}

#[derive(Debug)]
pub struct WalkDir {
    stack: Vec<Result<DirEntry, Error>>,
    yield_file: bool,
    yield_dir: bool,
    yield_symlink: bool,
}

impl WalkDir {
    pub fn new<P>(dir: P) -> Result<Self, Error>
    where
        P: AsRef<Path>,
    {
        Ok(Self {
            stack: fs::read_dir(dir)?.collect(),
            yield_file: true,
            yield_dir: false,
            yield_symlink: false,
        })
    }

    pub fn yield_file(mut self, value: bool) -> Self {
        self.yield_file = value;
        self
    }

    pub fn yield_dir(mut self, value: bool) -> Self {
        self.yield_dir = value;
        self
    }

    pub fn yield_symlink(mut self, value: bool) -> Self {
        self.yield_symlink = value;
        self
    }
}

impl Iterator for WalkDir {
    type Item = Result<PathBuf, Error>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let entry = match self.stack.pop()? {
                Err(error) => return Some(Err(error)),
                Ok(entry) => entry,
            };
            let file_type = match entry.file_type() {
                Err(error) => return Some(Err(error)),
                Ok(file_type) => file_type,
            };
            let path = entry.path();
            if (file_type.is_file() && self.yield_file)
                || (file_type.is_symlink() && self.yield_symlink)
            {
                return Some(Ok(path));
            }
            if file_type.is_dir() {
                match fs::read_dir(&path) {
                    // Yes, this arm is reachable.
                    Err(error) if error.kind() == ErrorKind::NotADirectory => continue,
                    Err(error) => return Some(Err(error)),
                    Ok(iter) => self.stack.extend(iter),
                }
                if self.yield_dir {
                    return Some(Ok(path));
                };
            }
        }
    }
}
