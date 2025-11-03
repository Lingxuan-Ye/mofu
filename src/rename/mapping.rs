use super::error::Error;
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
