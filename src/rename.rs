//! Utilities for batch rename.

pub use self::error::Error;
pub use self::mapping::Mapping;
pub use self::queue::RenameQueue;

mod error;
mod mapping;
mod queue;
