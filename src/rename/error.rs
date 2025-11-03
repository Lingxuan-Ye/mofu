use std::error;
use std::fmt;
use std::io;
use std::path::PathBuf;
use std::rc::Rc;

/// A enum for error handling.
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
