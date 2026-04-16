#![no_std]
extern crate alloc;

pub mod error;
pub mod pci;

use device::DeviceRegistry;
pub use error::BusError;

pub trait Bus {
    fn name(&self) -> &str;
    fn enumerate(&self, registry: &DeviceRegistry) -> Result<(), BusError>;
}
