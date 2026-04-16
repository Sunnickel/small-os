pub mod caps;
pub mod device;

use alloc::sync::Arc;

use ::device::DeviceRegistry;
pub use device::PciBusDevice;

use crate::{Bus, BusError};

// Well-known vendor/device IDs for logging
pub const VENDOR_INTEL: u16 = 0x8086;
pub const VENDOR_VIRTIO: u16 = 0x1AF4;
pub const CLASS_STORAGE: u8 = 0x01;
pub const CLASS_NETWORK: u8 = 0x02;
pub const CLASS_DISPLAY: u8 = 0x03;
pub const SUBCLASS_AHCI: u8 = 0x06;
pub const SUBCLASS_NVME: u8 = 0x08;

pub struct PciBus;

impl Bus for PciBus {
    fn name(&self) -> &str { "pci" }

    fn enumerate(&self, registry: &DeviceRegistry) -> Result<(), BusError> {
        if !hal::pci::is_ready() {
            return Err(BusError::NotInitialized);
        }

        hal::pci::enumerate(|info| {
            let dev = Arc::new(PciBusDevice::new(info));
            if let Err(e) = registry.register(dev) {
                let _ = e;
            }
        });

        Ok(())
    }
}
