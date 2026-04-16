use alloc::boxed::Box;

use crate::io::{IoError, Read, Seek};

pub trait FileSystem: Send + Sync {
    fn open(&self, path: &str) -> Result<Box<dyn File>, FsError>;
    fn exists(&self, path: &str) -> bool;
}

pub trait File: Read + Seek {
    fn size(&self) -> u64;
    fn path(&self) -> &str;
}

#[derive(Debug)]
pub enum FsError {
    NotFound,
    NotAFile,
    NotADirectory,
    Io(IoError),
    InvalidPath,
    Corrupted,
    Other,
}
