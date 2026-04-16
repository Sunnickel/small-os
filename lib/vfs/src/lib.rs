#![no_std]

extern crate alloc;

use alloc::{
    collections::BTreeMap,
    string::{String, ToString},
    sync::Arc,
};
use core::fmt::Debug;

use spin::{Mutex, RwLock};

pub mod file;
pub mod fs;
pub mod inode;
pub mod mount;
pub mod path;

pub use file::{File, FileHandle};
use hal::{fs::FileSystem, io::IoError};
pub use inode::{Inode, InodeType, Metadata};
pub use mount::{MountPoint, MountTable};
pub use path::Path;

/// Virtual File System manager
pub struct Vfs {
    mount_table: RwLock<MountTable>,
    fd_table: Mutex<BTreeMap<u64, FileHandle>>,
    next_fd: Mutex<u64>,
}

impl Vfs {
    pub const fn new() -> Self {
        Self {
            mount_table: RwLock::new(MountTable::new()),
            fd_table: Mutex::new(BTreeMap::new()),
            next_fd: Mutex::new(0),
        }
    }

    /// Mount a filesystem at a path
    pub fn mount(&self, fs: Arc<dyn FileSystem>, mount_point: Path) -> Result<(), FsError> {
        self.mount_table.write().mount(fs, mount_point)
    }

    /// Open a file by path
    pub fn open(&self, path: &str, flags: OpenFlags) -> Result<u64, FsError> {
        let path = Path::new(path)?;
        let fs = self.find_fs_for_path(&path)?;

        // Convert path to string for HAL filesystem
        let path_str = path.to_string();
        let file = fs.open(&path_str).map_err(|_| FsError::NotFound)?;

        let fd = self.alloc_fd();
        self.fd_table.lock().insert(fd, FileHandle::new(file));
        Ok(fd)
    }

    /// Read from file descriptor
    pub fn read(&self, fd: u64, buf: &mut [u8]) -> Result<usize, FsError> {
        let mut table = self.fd_table.lock();
        let handle = table.get_mut(&fd).ok_or(FsError::BadFd)?;
        Ok(handle.read(buf)?)
    }

    /// Write to file descriptor
    pub fn write(&self, fd: u64, buf: &[u8]) -> Result<usize, FsError> {
        let mut table = self.fd_table.lock();
        let handle = table.get_mut(&fd).ok_or(FsError::BadFd)?;
        Ok(handle.write(buf)?)
    }

    /// Close file descriptor
    pub fn close(&self, fd: u64) -> Result<(), FsError> {
        self.fd_table.lock().remove(&fd).ok_or(FsError::BadFd)?;
        Ok(())
    }

    /// Find the filesystem that handles this path
    fn find_fs_for_path(&self, path: &Path) -> Result<Arc<dyn FileSystem>, FsError> {
        self.mount_table.read().find_mount(path)
    }

    fn alloc_fd(&self) -> u64 {
        let mut next = self.next_fd.lock();
        let fd = *next;
        *next += 1;
        fd
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FsError {
    NotFound,
    NotDir,
    IsDir,
    PermissionDenied,
    BadFd,
    InvalidPath,
    MountFailed,
    IoError,
    NotImplemented,
}

impl From<IoError> for FsError {
    fn from(err: IoError) -> Self { FsError::IoError }
}

#[derive(Debug, Clone, Copy)]
pub struct FsStat {
    pub total_blocks: u64,
    pub free_blocks: u64,
    pub block_size: u32,
}

bitflags::bitflags! {
    pub struct OpenFlags: u32 {
        const READ = 1;
        const WRITE = 2;
        const CREATE = 4;
        const APPEND = 8;
        const TRUNC = 16;
    }
}
