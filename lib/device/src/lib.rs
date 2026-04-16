#![no_std]
extern crate alloc;

mod device_id;
mod error;
mod registry;

pub use device_id::DeviceId;
pub use error::DeviceError;
pub use registry::DeviceRegistry;

use alloc::sync::Arc;
use core::any::Any;
// ── Device type taxonomy ─────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeviceType {
	Block,
	Character,
	Network,
	Display,
	Bus,
	Platform,
	Unknown,
}

// ── The core device trait ────────────────────────────────────────────────────

pub trait Device: Send + Sync {
	fn name(&self) -> &str;
	fn device_type(&self) -> DeviceType;

	/// Called once when the device is registered.
	/// Perform any hardware init here (reset, enable interrupts, etc).
	fn probe(&self) -> Result<(), DeviceError> { Ok(()) }

	/// Called when the device is unregistered or the system shuts down.
	fn remove(&self) {}

	fn as_any(&self) -> &dyn Any;
}

// ── Typed handle returned to callers ─────────────────────────────────────────

pub struct DeviceHandle<T: Device + ?Sized = dyn Device> {
	pub id:     DeviceId,
	pub inner:  Arc<T>,
}

impl<T: Device + ?Sized> Clone for DeviceHandle<T> {
	fn clone(&self) -> Self {
		Self { id: self.id, inner: Arc::clone(&self.inner) }
	}
}