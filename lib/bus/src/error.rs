#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BusError {
	NotInitialized,
	EnumerationFailed,
	ResourceConflict,
	DeviceRegistrationFailed,
}

impl BusError {
	pub const fn as_str(self) -> &'static str {
		match self {
			Self::NotInitialized          => "bus not initialized",
			Self::EnumerationFailed       => "bus enumeration failed",
			Self::ResourceConflict        => "resource conflict",
			Self::DeviceRegistrationFailed => "device registration failed",
		}
	}
}