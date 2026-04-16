use crate::io::IoError;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BlockError {
    DeviceNotReady,
    DeviceError,
    Timeout,
    NoMemory,
    OutOfBounds,
    BadSector,
    ReadError,
    WriteError,
    InvalidGeometry,
    UnsupportedSectorSize,
    Other,
    InvalidRequest,
    DeviceBusy,
    InvalidBuffer,
}

impl BlockError {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::DeviceNotReady => "device not ready",
            Self::DeviceError => "device error",
            Self::DeviceBusy => "device busy",
            Self::Timeout => "timeout",
            Self::NoMemory => "no memory",
            Self::OutOfBounds => "out of bounds",
            Self::BadSector => "bad sector",
            Self::ReadError => "read error",
            Self::WriteError => "write error",
            Self::InvalidGeometry => "invalid geometry",
            Self::UnsupportedSectorSize => "unsupported sector size",
            Self::InvalidRequest => "invalid request",
            Self::InvalidBuffer => "invalid buffer",
            Self::Other => "other",
        }
    }
}

impl From<BlockError> for IoError {
    fn from(e: BlockError) -> Self {
        match e {
            BlockError::OutOfBounds => IoError::InvalidInput,
            BlockError::ReadError => IoError::UnexpectedEof,
            BlockError::Timeout => IoError::TimedOut,
            BlockError::NoMemory => IoError::OutOfMemory,
            BlockError::UnsupportedSectorSize => IoError::InvalidInput,
            BlockError::InvalidGeometry => IoError::InvalidInput,
            BlockError::BadSector => IoError::InvalidData,
            _ => IoError::Other,
        }
    }
}
