use std::fmt;
use std::io;
use std::path::PathBuf;

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

    AtomicFailed {
        rename: Box<Self>,
        revert: Option<Box<Self>>,
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

            Self::AtomicFailed { rename, revert } => {
                writeln!(f, "atomic rename failed:")?;
                writeln!(f, "{INDENT}on rename:")?;
                for line in rename.to_string().lines() {
                    writeln!(f, "{INDENT}{INDENT}{line}")?;
                }
                if let Some(revert) = revert {
                    writeln!(f, "{INDENT}on revert:")?;
                    for line in revert.to_string().lines() {
                        writeln!(f, "{INDENT}{INDENT}{line}")?;
                    }
                }
            }
        }

        Ok(())
    }
}

impl std::error::Error for Error {}
