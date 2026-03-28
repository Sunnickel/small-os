#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FsError {
    NotFound,
    NotADirectory,
    NotAFile,
    PermissionDenied,
    InvalidPath,
    Corrupted,
    Other,
}
