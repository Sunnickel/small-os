use hal::io::IoError;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DeviceError {
	AlreadyRegistered,
	NotFound,
	ProbeFailed,
	Unsupported,
	Io(IoError),
}

impl DeviceError {
	pub const fn as_str(self) -> &'static str {
		match self {
			Self::AlreadyRegistered => "device already registered",
			Self::NotFound          => "device not found",
			Self::ProbeFailed       => "device probe failed",
			Self::Unsupported       => "unsupported operation",
			Self::Io(_)             => "I/O error",
		}
	}
}