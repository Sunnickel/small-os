use alloc::boxed::Box;
// src/fs/vfs/file.rs
use alloc::sync::Arc;

use hal::fs::File as FileTrait;

use super::{FsError, OpenFlags, inode::Inode};

pub struct File {
    inode: Arc<dyn Inode>,
    flags: OpenFlags,
    offset: u64,
}

impl File {
    pub fn new(inode: Arc<dyn Inode>, flags: OpenFlags) -> Self { Self { inode, flags, offset: 0 } }

    pub fn read(&mut self, buf: &mut [u8]) -> Result<usize, FsError> {
        if !self.flags.contains(OpenFlags::READ) {
            return Err(FsError::PermissionDenied);
        }
        let bytes = self.inode.read_at(self.offset, buf)?;
        self.offset += bytes as u64;
        Ok(bytes)
    }

    pub fn write(&mut self, buf: &[u8]) -> Result<usize, FsError> {
        if !self.flags.contains(OpenFlags::WRITE) {
            return Err(FsError::PermissionDenied);
        }
        if self.flags.contains(OpenFlags::APPEND) {
            self.offset = self.inode.metadata().size;
        }
        let bytes = self.inode.write_at(self.offset, buf)?;
        self.offset += bytes as u64;
        Ok(bytes)
    }

    pub fn seek(&mut self, pos: u64) { self.offset = pos; }

    pub fn metadata(&self) -> super::inode::Metadata { self.inode.metadata() }
}

pub struct FileHandle {
    inner: Box<dyn FileTrait>,
}

impl FileHandle {
    pub fn new(file: Box<dyn FileTrait>) -> Self { Self { inner: file } }

    pub fn read(&mut self, buf: &mut [u8]) -> Result<usize, hal::io::IoError> {
        self.inner.read(buf)
    }

    pub fn write(&mut self, buf: &[u8]) -> Result<usize, hal::io::IoError> {
        Err(hal::io::IoError::Other)
    }
}
