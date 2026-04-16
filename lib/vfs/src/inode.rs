// src/fs/vfs/inode.rs
use alloc::{string::String, sync::Arc, vec::Vec};
use core::fmt::Debug;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum InodeType {
    File,
    Directory,
    Symlink,
    BlockDevice,
    CharDevice,
    Socket,
}

#[derive(Debug, Clone)]
pub struct Metadata {
    pub inode_num: u64,
    pub size: u64,
    pub typ: InodeType,
    pub permissions: u16,
    pub created: u64,
    pub modified: u64,
    pub accessed: u64,
    pub links: u32,
}

pub trait Inode: Send + Sync {
    /// Get metadata
    fn metadata(&self) -> Metadata;

    /// Lookup entry in directory
    fn lookup(&self, name: &str) -> Result<Arc<dyn Inode>, super::FsError>;

    /// Read symlink target
    fn readlink(&self) -> Result<String, super::FsError>;

    /// Read data at offset
    fn read_at(&self, offset: u64, buf: &mut [u8]) -> Result<usize, super::FsError>;

    /// Write data at offset
    fn write_at(&self, offset: u64, buf: &[u8]) -> Result<usize, super::FsError>;

    /// List directory entries
    fn readdir(&self) -> Result<Vec<DirectoryEntry>, super::FsError>;

    /// Get inode type
    fn inode_type(&self) -> InodeType;
}

#[derive(Debug, Clone)]
pub struct DirectoryEntry {
    pub name: String,
    pub inode_num: u64,
    pub typ: InodeType,
}
