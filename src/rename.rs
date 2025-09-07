use std::collections::{HashMap, HashSet, VecDeque};
use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

#[derive(Debug)]
pub struct NameMap {
    entries: Vec<Entry>,
    renamed: usize,
}

#[derive(Debug)]
pub struct Entry {
    src: PathBuf,
    dst: PathBuf,
}

impl NameMap {
    pub fn resolve(map: &HashMap<PathBuf, PathBuf>) -> Result<Self, Error> {
        let mut size = map.len();

        let mut rev_map: HashMap<&Path, &Path> = HashMap::with_capacity(size);
        for (src, dst) in map.iter() {
            // Ensure that each path is either a file or a symlink,
            // regardless of the target.
            let metadata = fs::symlink_metadata(src)?;
            if !metadata.is_file() && !metadata.is_symlink() {
                return Err(Error::NotFileOrSymlink(src.to_path_buf()));
            }
            // Ensure that each vertex has both in-degree and out-degree
            // less than or equal to 1.
            if let Some(collided) = rev_map.get(dst.as_path()) {
                return Err(Error::Collision {
                    src: (collided.to_path_buf(), src.to_path_buf()),
                    dst: dst.to_path_buf(),
                });
            }
            if src == dst {
                size -= 1;
            }
            rev_map.insert(dst, src);
        }

        let mut visited = HashSet::with_capacity(size);
        let mut graph = Vec::with_capacity(size);
        // May not be a complete component but a partially truncated path.
        let mut walk = VecDeque::with_capacity(size);

        for (src, dst) in map.iter() {
            if src == dst || visited.contains(src) {
                continue;
            }
            visited.insert(src);
            walk.push_front(Entry {
                src: src.to_path_buf(),
                dst: dst.to_path_buf(),
            });
            let mut next_src = dst;
            while let Some(next_dst) = map.get(next_src) {
                visited.insert(next_src);
                if next_dst == src {
                    let temp = temp_path(next_src);
                    walk.push_front(Entry {
                        src: next_src.to_path_buf(),
                        dst: temp.clone(),
                    });
                    walk.push_back(Entry {
                        src: temp,
                        dst: src.to_path_buf(),
                    });
                    break;
                } else {
                    walk.push_front(Entry {
                        src: next_src.to_path_buf(),
                        dst: next_dst.to_path_buf(),
                    });
                }
                next_src = next_dst;
            }
            graph.extend(walk.drain(..));
        }

        Ok(Self {
            entries: graph,
            renamed: 0,
        })
    }

    pub fn rename(&mut self) -> Result<(), Error> {
        for entry in self.entries.iter().skip(self.renamed) {
            fs::rename(&entry.src, &entry.dst)?;
            self.renamed += 1;
        }
        Ok(())
    }
}

fn temp_path(base: &Path) -> PathBuf {
    let mut temp = base.to_path_buf();
    for i in 0.. {
        temp.set_extension(format!("temp_{i}"));
        if !temp.exists() {
            break;
        }
    }
    temp
}

#[derive(Debug)]
pub enum Error {
    Io(io::Error),

    NotFileOrSymlink(PathBuf),

    Collision {
        src: (PathBuf, PathBuf),
        dst: PathBuf,
    },
}

impl From<io::Error> for Error {
    fn from(value: io::Error) -> Self {
        Self::Io(value)
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => {
                write!(f, "{error}")
            }

            Self::NotFileOrSymlink(path) => {
                write!(f, "not a file or a symlink: {}", path.display())
            }

            Self::Collision { src, dst } => {
                write!(
                    f,
                    "\
rename collision detected:

from
- {}
- {}

to
- {}",
                    src.0.display(),
                    src.1.display(),
                    dst.display()
                )
            }
        }
    }
}

impl std::error::Error for Error {}
