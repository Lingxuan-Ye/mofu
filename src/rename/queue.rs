use super::error::Error;
use super::mapping::Mapping;
use serde::de::{Deserialize, Deserializer, Error as DeError, MapAccess, Visitor};
use serde::ser::{Serialize, SerializeStruct, Serializer};
use std::collections::hash_map::Entry;
use std::collections::{HashMap, HashSet, VecDeque};
use std::fmt;
use std::path;
use std::path::Path;
use std::rc::Rc;

/// A queue for batch renaming operations.
///
/// # Serialization & Deserialization
///
/// This type implements [`Serialize`] and [`Deserialize`]. Notably, the
/// [`Deserialize`] implementation preserves the renaming order as-is. If
/// the serialized output is modified or reordered, or if any relevant files
/// are added, removed, or moved, it will no longer be possible to revert
/// to the initial state.
#[derive(Debug)]
pub struct RenameQueue {
    queue: Vec<Mapping>,
    renamed: usize,
}

impl RenameQueue {
    /// Creates a new [`RenameQueue`] from an iterator over source–destination
    /// mapping pairs.
    ///
    /// The renaming order is not determined by the given iterator. To see the
    /// exact execution order, use [`RenameQueue::pending`].
    ///
    /// # Panics
    ///
    /// May panic if any path is empty.
    ///
    /// # Errors
    ///
    /// - [`Error::Io`] if an I/O error occurs.
    /// - [`Error::OneToMany`] if a single source maps to multiple destinations.
    /// - [`Error::ManyToOne`] if multiple sources map to the same destination.
    /// - [`Error::NonLeafNode`] if any two paths have an ancestor–descendant
    ///   relationship, regardless of whether they are sources or destinations.
    ///
    /// On case-insensitive file systems, [`Error::OneToMany`] and [`Error::ManyToOne`]
    /// are not detected for paths that differ only in letter case. However, the
    /// renaming process will stop at such conflicts. If no concurrent file access
    /// occurs, it can be safely reverted.
    ///
    /// If any path has a symlink ancestor, or contains `..`, [`Error::NonLeafNode`]
    /// may not be detected. In that case, the renaming process is considered to
    /// be in an ambiguous state and may or may not stop executing. Regardless,
    /// the execution is considered incorrect. If no concurrent file access occurs,
    /// it can be safely reverted.
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

    /// Renames the pending mappings atomically.
    ///
    /// # Errors
    ///
    /// - [`Error::AlreadyExists`] if any destination already exists.
    /// - [`Error::Io`] if an I/O error occurs.
    /// - [`Error::AtomicActionFailed`] if the rename attempt fails and the
    ///   subsequent rollback also fails.
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

    /// Reverts the renamed mappings atomically.
    ///
    /// # Errors
    ///
    /// - [`Error::AlreadyExists`] if any destination already exists.
    /// - [`Error::Io`] if an I/O error occurs.
    /// - [`Error::AtomicActionFailed`] if the revert attempt fails and the
    ///   subsequent rollback also fails.
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

    /// Renames the pending mappings.
    ///
    /// # Errors
    ///
    /// - [`Error::AlreadyExists`] if any destination already exists.
    /// - [`Error::Io`] if an I/O error occurs.
    pub fn rename(&mut self) -> Result<&mut Self, Error> {
        for mapping in self.queue.iter().skip(self.renamed) {
            mapping.rename()?;
            self.renamed += 1;
        }
        Ok(self)
    }

    /// Reverts the renamed mappings.
    ///
    /// # Errors
    ///
    /// - [`Error::AlreadyExists`] if any destination already exists.
    /// - [`Error::Io`] if an I/O error occurs.
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

    /// Returns the renamed mappings.
    #[inline]
    pub fn renamed(&self) -> &[Mapping] {
        &self.queue[..self.renamed]
    }

    /// Returns the pending mappings.
    #[inline]
    pub fn pending(&self) -> &[Mapping] {
        &self.queue[self.renamed..]
    }
}

const FIELDS: &[&str] = &["renamed", "pending"];

impl Serialize for RenameQueue {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut queue = serializer.serialize_struct("RenameQueue", 2)?;
        queue.serialize_field("renamed", self.renamed())?;
        queue.serialize_field("pending", self.pending())?;
        queue.end()
    }
}

impl<'de> Deserialize<'de> for RenameQueue {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_struct("RenameQueue", FIELDS, RenameQueueVisitor)
    }
}

#[derive(Debug)]
struct RenameQueueVisitor;

impl<'de> Visitor<'de> for RenameQueueVisitor {
    type Value = RenameQueue;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("struct RenameQueue")
    }

    fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
    where
        A: MapAccess<'de>,
    {
        let mut renamed: Option<Vec<Mapping>> = None;
        let mut pending: Option<Vec<Mapping>> = None;

        while let Some(key) = map.next_key()? {
            match key {
                Field::Renamed => {
                    if renamed.is_some() {
                        return Err(DeError::duplicate_field("renamed"));
                    }
                    renamed = Some(map.next_value()?);
                }
                Field::Pending => {
                    if pending.is_some() {
                        return Err(DeError::duplicate_field("pending"));
                    }
                    pending = Some(map.next_value()?);
                }
            }
        }

        let renamed = renamed.ok_or_else(|| DeError::missing_field("renamed"))?;
        let pending = pending.ok_or_else(|| DeError::missing_field("pending"))?;

        let mut queue = renamed;
        let renamed = queue.len();
        queue.extend(pending);

        Ok(RenameQueue { queue, renamed })
    }
}

#[derive(Debug)]
enum Field {
    Renamed,
    Pending,
}

impl<'de> Deserialize<'de> for Field {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_identifier(FieldVisitor)
    }
}

#[derive(Debug)]
struct FieldVisitor;

impl Visitor<'_> for FieldVisitor {
    type Value = Field;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("`renamed` or `pending`")
    }

    fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
    where
        E: DeError,
    {
        match value {
            "renamed" => Ok(Field::Renamed),
            "pending" => Ok(Field::Pending),
            _ => Err(DeError::unknown_field(value, FIELDS)),
        }
    }
}
