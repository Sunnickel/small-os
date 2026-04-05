use core::fmt;

use hal::{fs::FsError, io::IoError};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum NtfsError {
    InvalidBootSector,
    InvalidMftRecord,
    InvalidAttribute,
    FileNotFound,
    InvalidPath,
    IoError,
    NotADirectory,
    NotAFile,
    CorruptedFilesystem,
    NoSpace,
    AlreadyExists,
    InvalidInput,
    NotSupported,
    NameTooLong,
}

impl fmt::Display for NtfsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidBootSector => write!(f, "invalid boot sector"),
            Self::InvalidMftRecord => write!(f, "invalid MFT record"),
            Self::InvalidAttribute => write!(f, "invalid attribute"),
            Self::FileNotFound => write!(f, "file not found"),
            Self::InvalidPath => write!(f, "invalid path"),
            Self::IoError => write!(f, "I/O error"),
            Self::NotADirectory => write!(f, "not a directory"),
            Self::NotAFile => write!(f, "not a file"),
            Self::CorruptedFilesystem => write!(f, "corrupted filesystem"),
            Self::NoSpace => write!(f, "no space left on device"),
            Self::AlreadyExists => write!(f, "file already exists"),
            Self::InvalidInput => write!(f, "Invalid Input given"),
            Self::NotSupported => write!(f, "not supported"),
            Self::NameTooLong => write!(f, "name too long"),
        }
    }
}

impl From<NtfsError> for FsError {
    fn from(e: NtfsError) -> Self {
        match e {
            NtfsError::FileNotFound => FsError::NotFound,
            NtfsError::NotADirectory => FsError::NotADirectory,
            NtfsError::NotAFile => FsError::NotAFile,
            NtfsError::InvalidPath => FsError::InvalidPath,
            NtfsError::CorruptedFilesystem => FsError::Corrupted,
            _ => FsError::Other,
        }
    }
}

impl From<NtfsError> for IoError {
    fn from(_: NtfsError) -> Self { IoError::Other }
}
