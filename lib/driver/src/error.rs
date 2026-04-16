#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DriverError {
	BindFailed,
	AlreadyBound,
	NotBound,
	NoDriverFound,
	Unsupported,
	Io(hal::io::IoError),
}

impl DriverError {
	pub const fn as_str(self) -> &'static str {
		match self {
			Self::BindFailed    => "driver bind failed",
			Self::AlreadyBound  => "device already has a driver bound",
			Self::NotBound      => "no driver bound to device",
			Self::NoDriverFound => "no matching driver found",
			Self::Unsupported   => "unsupported operation",
			Self::Io(_)         => "I/O error",
		}
	}
}