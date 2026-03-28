#[derive(Debug, Clone, Copy, PartialEq)]
pub enum IoError {
    UnexpectedEof,
    InvalidInput,
    InvalidData,
    TimedOut,
    Interrupted,
    NotFound,
    PermissionDenied,
    OutOfMemory,
    Unsupported,
    Other,
}

impl IoError {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::UnexpectedEof => "unexpected EOF",
            Self::InvalidInput => "invalid input",
            Self::InvalidData => "invalid data",
            Self::TimedOut => "timed out",
            Self::Interrupted => "interrupted",
            Self::NotFound => "not found",
            Self::PermissionDenied => "permission denied",
            Self::OutOfMemory => "out of memory",
            Self::Unsupported => "unsupported",
            Self::Other => "other",
        }
    }
}
