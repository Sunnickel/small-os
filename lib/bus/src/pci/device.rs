use alloc::string::{String, ToString};
use alloc::vec::Vec;

use hal::pci::PciDeviceInfo;
use device::{Device, DeviceError, DeviceType};

use super::caps::{Capability, parse_capabilities};

/// A PCI function registered into the device registry.
/// Owns the HAL device info and the parsed capability list.
pub struct PciBusDevice {
	info:  PciDeviceInfo,
	name:  String,
	caps:  Vec<Capability>,
}

impl PciBusDevice {
	pub fn new(info: PciDeviceInfo) -> Self {
		let name = alloc::format!(
			"pci:{:02x}:{:02x}.{}",
			info.address.bus,
			info.address.device,
			info.address.function,
		);
		let caps = parse_capabilities(&info);
		Self { info, name, caps }
	}

	/// Access the underlying HAL device info.
	/// Drivers call this after downcasting to get BARs, config space access, etc.
	pub fn info(&self) -> &PciDeviceInfo { &self.info }

	/// Parsed capability list.
	pub fn capabilities(&self) -> &[Capability] { &self.caps }

	/// Shorthand: is this a storage controller?
	pub fn is_storage(&self) -> bool { self.info.class == 0x01 }

	/// Shorthand: is this a network controller?
	pub fn is_network(&self) -> bool { self.info.class == 0x02 }

	/// Vendor + device ID pair for driver matching.
	pub fn id_pair(&self) -> (u16, u16) {
		(self.info.vendor_id, self.info.device_id)
	}

	/// Enable bus mastering DMA — most drivers need this.
	pub fn enable_dma(&self) { self.info.enable_bus_master(); }

	/// Enable MMIO decoding.
	pub fn enable_mmio(&self) { self.info.enable_mmio(); }
}

impl Device for PciBusDevice {
	fn name(&self) -> &str { &self.name }
	fn device_type(&self) -> DeviceType { DeviceType::Bus }
	fn probe(&self) -> Result<(), DeviceError> {
		if self.info.vendor_id == 0xFFFF {
			return Err(DeviceError::ProbeFailed);
		}
		Ok(())
	}

	fn remove(&self) {}

	fn as_any(&self) -> &dyn core::any::Any { self }
}