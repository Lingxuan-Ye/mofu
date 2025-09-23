use super::error::Error;
use std::collections::{HashMap, HashSet, VecDeque};
use std::fs;
use std::path::{Path, PathBuf};
use std::rc::Rc;

#[derive(Debug)]
pub struct NameMap {
    entries: Vec<Mapping>,
    renamed: usize,
}

impl NameMap {
    pub fn resolve<I>(map: I) -> Result<Self, Error>
    where
        I: IntoIterator<Item = Mapping>,
    {
        let map: HashMap<Rc<PathBuf>, Rc<PathBuf>> = map
            .into_iter()
            .map(|mapping| (mapping.src, mapping.dst))
            .collect();
        let len = map.len();

        let mut rev_map: HashMap<&Path, &Path> = HashMap::with_capacity(len);
        for (src, dst) in map.iter() {
            // Ensure that each vertex has both in-degree and out-degree
            // less than or equal to 1.
            if let Some(collided) = rev_map.get(dst.as_path()) {
                return Err(Error::ManyToOne {
                    src: (collided.to_path_buf(), src.to_path_buf()),
                    dst: dst.to_path_buf(),
                });
            }
            rev_map.insert(dst, src);
        }
        drop(rev_map);

        let mut visited = HashSet::with_capacity(len);
        let mut graph = Vec::with_capacity(len);
        // May not be a complete component but a partially truncated path.
        let mut walk = VecDeque::with_capacity(len);

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

    pub fn rename_atomic(&mut self) -> Result<&mut Self, Error> {
        if let Err(error) = self.rename() {
            let rename = Box::new(error);
            let revert = self.revert().err().map(Box::new);
            Err(Error::AtomicFailed { rename, revert })
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
