use std::collections::{HashMap, HashSet, VecDeque};
use std::error::Error;
use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

#[derive(Debug)]
pub struct NameMap {
    entries: HashMap<PathBuf, PathBuf>,
    temp_cache: Vec<*mut PathBuf>,
}

#[derive(Debug)]
pub struct RenameQueue<'a> {
    entries: Vec<Entry<'a>>,
    renamed: usize,
}

#[derive(Debug)]
pub struct Entry<'a> {
    pub src: &'a Path,
    pub dst: &'a Path,
}

impl TryFrom<HashMap<PathBuf, PathBuf>> for NameMap {
    type Error = RenameError;

    fn try_from(value: HashMap<PathBuf, PathBuf>) -> Result<Self, Self::Error> {
        let len = value.len();
        let mut rev_map: HashMap<&Path, &Path> = HashMap::with_capacity(len);

        for (src, dst) in value.iter() {
            // Ensure that each vertex has both in-degree and out-degree
            // less than or equal to 1.
            if let Some(collided) = rev_map.get(dst.as_path()) {
                return Err(RenameError::ManyToOne {
                    src: (collided.to_path_buf(), src.to_path_buf()),
                    dst: dst.to_path_buf(),
                });
            }
            rev_map.insert(dst, src);
        }

        Ok(Self {
            entries: value,
            temp_cache: Vec::new(),
        })
    }
}

impl NameMap {
    pub fn resolve(&mut self) -> RenameQueue<'_> {
        let len = self.entries.len();
        let mut visited: HashSet<&Path> = HashSet::with_capacity(len);
        let mut graph = Vec::with_capacity(len);
        // May not be a complete component but a partially truncated path.
        let mut walk = VecDeque::with_capacity(len);

        for (src, dst) in self.entries.iter() {
            if src == dst || visited.contains(src.as_path()) {
                continue;
            }
            visited.insert(src);
            walk.push_front(Entry { src, dst });
            let mut next_src = dst;
            while let Some(next_dst) = self.entries.get(next_src) {
                visited.insert(next_src);
                if next_dst != src {
                    walk.push_front(Entry {
                        src: next_src,
                        dst: next_dst,
                    });
                } else {
                    let temp = {
                        let mut temp = next_src.to_path_buf();
                        for i in 0.. {
                            temp.set_extension(format!("temp_{i}"));
                            if !temp.exists() {
                                break;
                            }
                        }
                        Box::into_raw(Box::new(temp))
                    };
                    self.temp_cache.push(temp);
                    walk.push_front(Entry {
                        src: next_src,
                        dst: unsafe { &*temp },
                    });
                    walk.push_back(Entry {
                        src: unsafe { &*temp },
                        dst: src,
                    });
                    break;
                }
                next_src = next_dst;
            }
            graph.extend(walk.drain(..));
        }

        RenameQueue {
            entries: graph,
            renamed: 0,
        }
    }
}

impl Drop for NameMap {
    fn drop(&mut self) {
        for &temp in self.temp_cache.iter() {
            let _ = unsafe { Box::from_raw(temp) };
        }
    }
}

impl RenameQueue<'_> {
    pub fn rename(&mut self) -> Result<&mut Self, RenameError> {
        for entry in self.entries.iter().skip(self.renamed) {
            // Ensure that each path is either a file or a symlink,
            // regardless of what the symlink points to.
            let metadata = fs::symlink_metadata(entry.src)?;
            if !metadata.is_file() && !metadata.is_symlink() {
                return Err(RenameError::NotFileOrSymlink(entry.src.to_path_buf()));
            }
            if entry.dst.exists() {
                return Err(RenameError::AlreadyExists {
                    src: entry.src.to_path_buf(),
                    dst: entry.dst.to_path_buf(),
                });
            }
        }

        for entry in self.entries.iter().skip(self.renamed) {
            if let Some(parent) = entry.dst.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::rename(entry.src, entry.dst)?;
            self.renamed += 1;
        }

        Ok(self)
    }

    pub fn revert(&mut self) -> Result<&mut Self, RenameError> {
        for entry in self.entries.iter().take(self.renamed).rev() {
            let metadata = fs::symlink_metadata(entry.dst)?;
            if !metadata.is_file() && !metadata.is_symlink() {
                return Err(RenameError::NotFileOrSymlink(entry.dst.to_path_buf()));
            }
            if entry.src.exists() {
                return Err(RenameError::AlreadyExists {
                    src: entry.dst.to_path_buf(),
                    dst: entry.src.to_path_buf(),
                });
            }
        }

        for entry in self.entries.iter().take(self.renamed).rev() {
            if let Some(parent) = entry.src.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::rename(entry.dst, entry.src)?;
            self.renamed -= 1;
        }

        Ok(self)
    }

    pub fn renamed(&self) -> &[Entry<'_>] {
        &self.entries[..self.renamed]
    }

    pub fn pending(&self) -> &[Entry<'_>] {
        &self.entries[self.renamed..]
    }
}

#[derive(Debug)]
pub enum RenameError {
    Io(io::Error),

    NotFileOrSymlink(PathBuf),

    ManyToOne {
        src: (PathBuf, PathBuf),
        dst: PathBuf,
    },

    AlreadyExists {
        src: PathBuf,
        dst: PathBuf,
    },
}

impl From<io::Error> for RenameError {
    fn from(value: io::Error) -> Self {
        Self::Io(value)
    }
}

impl fmt::Display for RenameError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => {
                write!(f, "{error}")
            }

            Self::NotFileOrSymlink(path) => {
                write!(f, "not a file or a symlink: {}", path.display())
            }

            Self::ManyToOne { src, dst } => {
                write!(
                    f,
                    "\
collision detected (many-to-one):

src: {}
dst: {}

src: {}
dst: {}",
                    src.0.display(),
                    dst.display(),
                    src.1.display(),
                    dst.display()
                )
            }

            Self::AlreadyExists { src, dst } => {
                write!(
                    f,
                    "\
collision detected (already exist):

src: {}
dst: {}
",
                    src.display(),
                    dst.display()
                )
            }
        }
    }
}

impl Error for RenameError {}
