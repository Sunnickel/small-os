use device::DeviceRegistry;

use crate::flags::DEVICE_REGISTRY;

pub fn registry() -> &'static DeviceRegistry { DEVICE_REGISTRY.call_once(DeviceRegistry::new) }
