#![no_std]
extern crate alloc;

pub mod error;
pub mod pci;

pub use error::BusError;

use device::DeviceRegistry;

pub trait Bus {
	fn name(&self) -> &str;
	fn enumerate(&self, registry: &DeviceRegistry) -> Result<(), BusError>;
}