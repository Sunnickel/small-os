#![no_std]
#![feature(error_in_core)]

extern crate alloc;

use alloc::string::{String, ToString};
use core::fmt;
pub use driver_core::block_device::BlockError;

pub use super::NtfsError;

// Unified error type for filesystem operations
#[derive(Debug)]
pub enum FsError {
    Ntfs(NtfsError),
    Block(BlockError),
    NotFound,
    InvalidPath,
    NotADirectory,
    IoError,
    CorruptedFilesystem,
    NoDevice,
    DriverInit
}

impl From<NtfsError> for FsError {
    fn from(e: NtfsError) -> Self {
        FsError::Ntfs(e)
    }
}

impl From<BlockError> for FsError {
    fn from(e: BlockError) -> Self {
        FsError::Block(e)
    }
}

impl Into<String> for FsError {
    fn into(self) -> String {
        match self {
            FsError::Ntfs(ref e) => e.to_string(),
            FsError::Block(ref e) => e.to_string(),
            FsError::NotFound => "not found".to_string(),
            FsError::InvalidPath => "invalid path".to_string(),
            FsError::NotADirectory => "not a directory".to_string(),
            FsError::IoError => "I/O error".to_string(),
            FsError::CorruptedFilesystem => "corrupted filesystem".to_string(),
            FsError::NoDevice => "no device".to_string(),
            FsError::DriverInit => "driver init".to_string(),
        }
    }
}

impl fmt::Display for FsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FsError::Ntfs(e) => write!(f, "NTFS error: {:?}", e),
            FsError::Block(e) => write!(f, "Block error: {}", e),
            FsError::NotFound => write!(f, "file not found"),
            FsError::InvalidPath => write!(f, "invalid path"),
            FsError::NotADirectory => write!(f, "not a directory"),
            FsError::IoError => write!(f, "I/O error"),
            FsError::CorruptedFilesystem => write!(f, "corrupted filesystem"),
            FsError::NoDevice => write!(f, "no device"),
            FsError::DriverInit => write!(f, "driver init"),
        }
    }
}

impl core::error::Error for FsError {}