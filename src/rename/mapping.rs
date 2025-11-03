use super::error::Error;
use serde::de::{Deserialize, Deserializer, Error as DeError, MapAccess, Visitor};
use serde::ser::{Serialize, SerializeStruct, Serializer};
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use std::rc::Rc;

/// A struct representing a single source-destination mapping.
#[derive(Debug)]
pub struct Mapping {
    pub(super) src: Rc<PathBuf>,
    pub(super) dst: Rc<PathBuf>,
}

impl Mapping {
    /// Returns the source.
    #[inline]
    pub fn src(&self) -> &Path {
        self.src.as_path()
    }

    /// Returns the destination.
    #[inline]
    pub fn dst(&self) -> &Path {
        self.dst.as_path()
    }

    pub(super) fn rename(&self) -> Result<(), Error> {
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

    pub(super) fn invert(&self) -> Self {
        let src = Rc::clone(&self.dst);
        let dst = Rc::clone(&self.src);
        Self { src, dst }
    }
}

const FIELDS: &[&str] = &["src", "dst"];

impl Serialize for Mapping {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut mapping = serializer.serialize_struct("Mapping", 2)?;
        mapping.serialize_field("src", self.src())?;
        mapping.serialize_field("dst", self.dst())?;
        mapping.end()
    }
}

impl<'de> Deserialize<'de> for Mapping {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_struct("Mapping", FIELDS, MappingVisitor)
    }
}

#[derive(Debug)]
struct MappingVisitor;

impl<'de> Visitor<'de> for MappingVisitor {
    type Value = Mapping;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("struct Mapping")
    }

    fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
    where
        A: MapAccess<'de>,
    {
        let mut src: Option<PathBuf> = None;
        let mut dst: Option<PathBuf> = None;

        while let Some(key) = map.next_key()? {
            match key {
                Field::Src => {
                    if src.is_some() {
                        return Err(DeError::duplicate_field("src"));
                    }
                    src = Some(map.next_value()?);
                }
                Field::Dst => {
                    if dst.is_some() {
                        return Err(DeError::duplicate_field("dst"));
                    }
                    dst = Some(map.next_value()?);
                }
            }
        }

        let src = src
            .map(Rc::new)
            .ok_or_else(|| DeError::missing_field("src"))?;
        let dst = dst
            .map(Rc::new)
            .ok_or_else(|| DeError::missing_field("dst"))?;

        Ok(Mapping { src, dst })
    }
}

#[derive(Debug)]
enum Field {
    Src,
    Dst,
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
        formatter.write_str("`src` or `dst`")
    }

    fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
    where
        E: DeError,
    {
        match value {
            "src" => Ok(Field::Src),
            "dst" => Ok(Field::Dst),
            _ => Err(DeError::unknown_field(value, FIELDS)),
        }
    }
}
