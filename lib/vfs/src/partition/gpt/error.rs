use hal::block::BlockError;
use hal::io::IoError;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum GptError {
    InvalidSignature,
    InvalidHeaderCrc,
    InvalidEntriesCrc,
    InvalidHeaderSize,
    InvalidEntrySize,
    IoError,
    Overflow,
    NoSpace,
    NotFound,
}

impl From<IoError> for GptError {
    fn from(_: IoError) -> Self { Self::IoError }
}

impl From<BlockError> for GptError {
    fn from(_: BlockError) -> Self { Self::IoError }
}
