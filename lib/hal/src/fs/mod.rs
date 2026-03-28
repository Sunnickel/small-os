mod error;

pub use crate::fs::error::FsError;
use crate::io::{Read, Seek};

pub trait FileSystem {
    type File: Read + Seek;

    fn open(&mut self, path: &str) -> Result<Self::File, FsError>;
    fn exists(&mut self, path: &str) -> bool;
    fn read_dir(&mut self, path: &str) -> Result<DirIter, FsError>;
}

pub struct DirEntry {
    pub name: &'static str,
    pub size: u64,
    pub kind: EntryKind,
}

pub enum EntryKind {
    File,
    Directory,
}

pub struct DirIter {
    // concrete impls will fill this
}
