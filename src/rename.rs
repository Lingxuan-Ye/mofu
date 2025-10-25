use std::collections::{HashMap, HashSet, VecDeque};
use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::rc::Rc;

#[derive(Debug)]
pub struct RenameQueue {
    entries: Vec<Mapping>,
    renamed: usize,
}

impl RenameQueue {
    pub fn try_from_map(map: HashMap<Rc<PathBuf>, Rc<PathBuf>>) -> Result<Self, Error> {
        Self::try_from(map)
    }

    pub fn try_from_iter<I>(iter: I) -> Result<Self, Error>
    where
        I: IntoIterator<Item = Mapping>,
    {
        let map = iter
            .into_iter()
            .map(|mapping| (mapping.src, mapping.dst))
            .collect();
        Self::try_from_map(map)
    }

    pub fn rename_atomic(&mut self) -> Result<&mut Self, Error> {
        if let Err(rename_error) = self.rename() {
            if let Err(revert_error) = self.revert() {
                Err(Error::AtomicActionFailed {
                    during_attempt: Box::new(rename_error),
                    during_rollback: Box::new(revert_error),
                })
            } else {
                Err(rename_error)
            }
        } else {
            Ok(self)
        }
    }

    pub fn revert_atomic(&mut self) -> Result<&mut Self, Error> {
        if let Err(revert_error) = self.revert() {
            if let Err(rename_error) = self.rename() {
                Err(Error::AtomicActionFailed {
                    during_attempt: Box::new(revert_error),
                    during_rollback: Box::new(rename_error),
                })
            } else {
                Err(revert_error)
            }
        } else {
            Ok(self)
        }
    }

    pub fn rename(&mut self) -> Result<&mut Self, Error> {
        for mapping in self.entries.iter().skip(self.renamed) {
            mapping.rename()?;
            self.renamed += 1;
        }
        Ok(self)
    }

    pub fn revert(&mut self) -> Result<&mut Self, Error> {
        for mapping in self
            .entries
            .iter()
            .take(self.renamed)
            .rev()
            .map(Mapping::invert)
        {
            mapping.rename()?;
            self.renamed -= 1;
        }
        Ok(self)
    }

    pub fn renamed(&self) -> &[Mapping] {
        &self.entries[..self.renamed]
    }

    pub fn pending(&self) -> &[Mapping] {
        &self.entries[self.renamed..]
    }
}

impl TryFrom<HashMap<Rc<PathBuf>, Rc<PathBuf>>> for RenameQueue {
    type Error = Error;

    fn try_from(map: HashMap<Rc<PathBuf>, Rc<PathBuf>>) -> Result<Self, Self::Error> {
        let mut capacity = map.len();

        let mut rev_map: HashMap<&Path, &Path> = HashMap::with_capacity(capacity);
        for (src, dst) in map.iter() {
            // Ensure that each vertex has both in-degree and out-degree
            // less than or equal to 1.
            if let Some(collided) = rev_map.get(dst.as_path()) {
                return Err(Error::ManyToOne {
                    src: (collided.to_path_buf(), src.to_path_buf()),
                    dst: dst.to_path_buf(),
                });
            }
            // There is no need to reserve capacity for self-loops, as they
            // are treated as noops. However, they should still be considered
            // in collision detection.
            if src == dst {
                capacity -= 1;
            }
            rev_map.insert(dst, src);
        }
        drop(rev_map);

        let mut visited = HashSet::with_capacity(capacity);
        // In the extreme case where every two mappings form a cycle, one
        // extra slot is needed for each temporary path.
        let mut graph = Vec::with_capacity(capacity / 2 * 3 + capacity % 2);
        // In the extreme case where the whole graph forms a cycle, one
        // extra slot is needed for the temporary path. Additionally,
        // `walk` may represent a partially truncated path rather than
        // a complete component, which does not affect correctness.
        let mut walk = VecDeque::with_capacity(capacity + 1);

        for (src, dst) in map.iter() {
            if src == dst || visited.contains(src) {
                continue;
            }
            visited.insert(Rc::clone(src));
            walk.push_front(Mapping {
                src: Rc::clone(src),
                dst: Rc::clone(dst),
            });
            let mut next_src = dst;
            while let Some(next_dst) = map.get(next_src) {
                visited.insert(Rc::clone(next_src));
                if next_dst != src {
                    walk.push_front(Mapping {
                        src: Rc::clone(next_src),
                        dst: Rc::clone(next_dst),
                    });
                } else {
                    let mut temp = next_src.to_path_buf();
                    for i in 0.. {
                        temp.set_extension(format!("temp_{i}"));
                        if !temp.exists() {
                            break;
                        }
                    }
                    let temp = Rc::new(temp);
                    walk.push_front(Mapping {
                        src: Rc::clone(next_src),
                        dst: Rc::clone(&temp),
                    });
                    walk.push_back(Mapping {
                        src: temp,
                        dst: Rc::clone(src),
                    });
                    break;
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
}

#[derive(Debug)]
pub struct Mapping {
    src: Rc<PathBuf>,
    dst: Rc<PathBuf>,
}

impl Mapping {
    pub fn new(src: PathBuf, dst: PathBuf) -> Self {
        Self {
            src: Rc::new(src),
            dst: Rc::new(dst),
        }
    }

    pub fn src(&self) -> &Path {
        self.src.as_path()
    }

    pub fn dst(&self) -> &Path {
        self.dst.as_path()
    }

    fn rename(&self) -> Result<(), Error> {
        let src = self.src();
        let dst = self.dst();
        if self.dst.exists() {
            return Err(Error::AlreadyExists {
                src: src.to_path_buf(),
                dst: dst.to_path_buf(),
            });
        }
        if let Some(parent) = dst.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::rename(src, dst)?;
        Ok(())
    }

    fn invert(&self) -> Self {
        Self {
            src: Rc::clone(&self.dst),
            dst: Rc::clone(&self.src),
        }
    }
}

#[derive(Debug)]
pub enum Error {
    Io(io::Error),

    ManyToOne {
        src: (PathBuf, PathBuf),
        dst: PathBuf,
    },

    AlreadyExists {
        src: PathBuf,
        dst: PathBuf,
    },

    AtomicActionFailed {
        during_attempt: Box<Self>,
        during_rollback: Box<Self>,
    },
}

impl From<io::Error> for Error {
    fn from(value: io::Error) -> Self {
        Self::Io(value)
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        const INDENT: &str = "    ";

        match self {
            Self::Io(error) => {
                writeln!(f, "{error}")?;
            }

            Self::ManyToOne { src, dst } => {
                writeln!(f, "collision detected (many-to-one):")?;
                writeln!(f, "{INDENT}src: {}", src.0.display())?;
                writeln!(f, "{INDENT}     {}", src.1.display())?;
                writeln!(f, "{INDENT}dst: {}", dst.display())?;
            }

            Self::AlreadyExists { src, dst } => {
                writeln!(f, "collision detected (already exists):")?;
                writeln!(f, "{INDENT}src: {}", src.display())?;
                writeln!(f, "{INDENT}dst: {}", dst.display())?;
            }

            Self::AtomicActionFailed {
                during_attempt,
                during_rollback,
            } => {
                writeln!(f, "atomic action failed:")?;
                writeln!(f, "{INDENT}during attempt:")?;
                for line in during_attempt.to_string().lines() {
                    writeln!(f, "{INDENT}{INDENT}{line}")?;
                }
                writeln!(f, "{INDENT}during rollback:")?;
                for line in during_rollback.to_string().lines() {
                    writeln!(f, "{INDENT}{INDENT}{line}")?;
                }
            }
        }

        Ok(())
    }
}

impl std::error::Error for Error {}
