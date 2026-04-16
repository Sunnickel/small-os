use driver::DriverRegistry;
use crate::flags::DRIVER_REGISTRY;

pub fn registry() -> &'static DriverRegistry { DRIVER_REGISTRY.call_once(DriverRegistry::new) }
