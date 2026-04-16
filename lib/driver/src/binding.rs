use alloc::boxed::Box;
use alloc::sync::Arc;
use core::any::Any;

use device::{Device, DeviceId};
use crate::block::BlockDeviceEnum;

/// Driver-private state produced during bind().
/// The driver allocates this; the registry stores and later returns it on unbind.
pub trait DriverState: Any + Send + Sync {
	/// Called on clean shutdown or unregister.
	fn stop(&self) {}

	/// Try to get block device access (if this is a block driver)
	fn as_block_device(self: Box<Self>) -> Option<BlockDeviceEnum> {
		None
	}
}

/// A live binding between a driver and a device.
pub struct Binding {
	pub device_id:    DeviceId,
	pub device:       Arc<dyn Device>,
	pub driver_name:  &'static str,
	pub state:        Option<Box<dyn DriverState>>,
}

impl Binding {
	pub fn new(
		device_id:   DeviceId,
		device:      Arc<dyn Device>,
		driver_name: &'static str,
		state:       Box<dyn DriverState>,
	) -> Self {
		Self {
			device_id,
			device,
			driver_name,
			state: Some(state)
		}
	}

	pub fn into_block_device(mut self) -> Option<BlockDeviceEnum> {
		let state = self.state.take()?;
		state.as_block_device()
	}

	pub fn state(&self) -> &dyn DriverState {
		self.state.as_ref().unwrap().as_ref()
	}
}

impl Drop for Binding {
	fn drop(&mut self) {
		if let Some(state) = self.state.take() {
			state.stop();
		}
	}
}