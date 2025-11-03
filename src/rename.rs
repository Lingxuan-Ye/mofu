use std::collections::hash_map::Entry;
use std::collections::{HashMap, HashSet, VecDeque};
use std::error;
use std::fmt;
use std::fs;
use std::io;
use std::path;
use std::path::{Path, PathBuf};
use std::rc::Rc;

#[derive(Debug)]
pub struct RenameQueue {
    queue: Vec<Mapping>,
    renamed: usize,
}

impl RenameQueue {
    pub fn new<I, S, D>(iter: I) -> Result<Self, Error>
    where
        I: IntoIterator<Item = (S, D)>,
        S: AsRef<Path>,
        D: AsRef<Path>,
    {
        let iter = iter.into_iter();
        let capacity = iter.size_hint().0;
        let mut map = HashMap::with_capacity(capacity);

        for (src, dst) in iter {
            let src = path::absolute(src).map(Rc::new)?;
            let dst = path::absolute(dst).map(Rc::new)?;
            match map.entry(src) {
                Entry::Occupied(entry) => {
                    // Duplicate mappings are ignored.
                    if entry.get() == &dst {
                        continue;
                    }
                    let (src, collided) = entry.remove_entry();
                    let dst = (collided, dst);
                    return Err(Error::OneToMany { src, dst });
                }
                Entry::Vacant(entry) => {
                    entry.insert(dst);
                }
            }
        }

        let mut capacity = map.len();
        let mut rev_map = HashMap::with_capacity(capacity);
        let mut paths = Vec::with_capacity(capacity);

        for (src, dst) in map.iter() {
            match rev_map.entry(dst) {
                Entry::Occupied(entry) => {
                    let collided = entry.remove();
                    let src = (Rc::clone(collided), Rc::clone(src));
                    let dst = Rc::clone(dst);
                    return Err(Error::ManyToOne { src, dst });
                }
                Entry::Vacant(entry) => {
                    entry.insert(src);
                }
            }
            if src == dst {
                capacity -= 1;
            } else {
                paths.push(src);
                paths.push(dst);
            }
        }
        drop(rev_map);

        paths.sort();
        for window in paths.windows(2) {
            let lower = window[0];
            let upper = window[1];
            if upper.starts_with(lower.as_path()) {
                let node = Rc::clone(lower);
                let child = Rc::clone(upper);
                return Err(Error::NonLeafNode {
                    node,
                    descendant: child,
                });
            }
        }
        drop(paths);

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
            if src == dst || visited.contains(src.as_path()) {
                continue;
            }

            visited.insert(src.as_path());

            walk.push_front(Mapping {
                src: Rc::clone(src),
                dst: Rc::clone(dst),
            });

            let mut next_src = dst;
            while let Some(next_dst) = map.get(next_src) {
                visited.insert(next_src.as_path());
                if next_dst == src {
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
                walk.push_front(Mapping {
                    src: Rc::clone(next_src),
                    dst: Rc::clone(next_dst),
                });
                next_src = next_dst;
            }

            graph.extend(walk.drain(..));
        }

        let queue = graph;
        let renamed = 0;
        Ok(Self { queue, renamed })
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
        for mapping in self.queue.iter().skip(self.renamed) {
            mapping.rename()?;
            self.renamed += 1;
        }
        Ok(self)
    }

    pub fn revert(&mut self) -> Result<&mut Self, Error> {
        for mapping in self
            .queue
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

    #[inline]
    pub fn renamed(&self) -> &[Mapping] {
        &self.queue[..self.renamed]
    }

    #[inline]
    pub fn pending(&self) -> &[Mapping] {
        &self.queue[self.renamed..]
    }
}

#[derive(Debug)]
pub struct Mapping {
    src: Rc<PathBuf>,
    dst: Rc<PathBuf>,
}

impl Mapping {
    #[inline]
    pub fn src(&self) -> &Path {
        self.src.as_path()
    }

    #[inline]
    pub fn dst(&self) -> &Path {
        self.dst.as_path()
    }

    fn rename(&self) -> Result<(), Error> {
        if self.dst.exists() {
            let src = Rc::clone(&self.src);
            let dst = Rc::clone(&self.dst);
            return Err(Error::AlreadyExists { src, dst });
        }
        if let Some(parent) = self.dst.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::rename(self.src(), self.dst())?;
        Ok(())
    }

    fn invert(&self) -> Self {
        let src = Rc::clone(&self.dst);
        let dst = Rc::clone(&self.src);
        Self { src, dst }
    }
}

#[derive(Debug)]
pub enum Error {
    Io(io::Error),

    OneToMany {
        src: Rc<PathBuf>,
        dst: (Rc<PathBuf>, Rc<PathBuf>),
    },

    ManyToOne {
        src: (Rc<PathBuf>, Rc<PathBuf>),
        dst: Rc<PathBuf>,
    },

    NonLeafNode {
        node: Rc<PathBuf>,
        descendant: Rc<PathBuf>,
    },

    AlreadyExists {
        src: Rc<PathBuf>,
        dst: Rc<PathBuf>,
    },

    AtomicActionFailed {
        during_attempt: Box<Self>,
        during_rollback: Box<Self>,
    },
}

impl From<io::Error> for Error {
    fn from(value: io::Error) -> Self {
        Error::Io(value)
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        const INDENT: &str = "  ";

        match self {
            Self::Io(error) => {
                writeln!(f, "{error}")?;
            }

            Self::OneToMany { src, dst } => {
                writeln!(f, "multiple destinations detected:")?;
                writeln!(f, "{INDENT}     source {}", src.display())?;
                writeln!(f, "{INDENT}destination {}", dst.0.display())?;
                writeln!(f, "{INDENT}            {}", dst.1.display())?;
            }

            Self::ManyToOne { src, dst } => {
                writeln!(f, "collision detected:")?;
                writeln!(f, "{INDENT}     source {}", src.0.display())?;
                writeln!(f, "{INDENT}            {}", src.1.display())?;
                writeln!(f, "{INDENT}destination {}", dst.display())?;
            }

            Self::NonLeafNode {
                node,
                descendant: child,
            } => {
                writeln!(f, "non-leaf node detected:")?;
                writeln!(f, "{INDENT}       node {}", node.display())?;
                writeln!(f, "{INDENT} descendant {}", child.display())?;
            }

            Self::AlreadyExists { src, dst } => {
                writeln!(f, "destination already exists:")?;
                writeln!(f, "{INDENT}     source {}", src.display())?;
                writeln!(f, "{INDENT}destination {}", dst.display())?;
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

impl error::Error for Error {}
