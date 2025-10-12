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

    AtomicActionFailed {
        during_attempt: Box<Self>,
        during_rollback: Option<Box<Self>>,
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
                if let Some(during_rollback) = during_rollback {
                    writeln!(f, "{INDENT}during rollback:")?;
                    for line in during_rollback.to_string().lines() {
                        writeln!(f, "{INDENT}{INDENT}{line}")?;
                    }
                }
            }
        }

        Ok(())
    }
}

impl std::error::Error for Error {}
