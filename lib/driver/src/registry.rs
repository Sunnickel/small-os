use alloc::{collections::BTreeMap, sync::Arc, vec::Vec};
use bus::pci::PciBusDevice;
use device::{DeviceId, DeviceRegistry};
use spin::RwLock;

use crate::{Binding, Driver, DriverError, DriverState};
use crate::block::BlockDeviceEnum;

pub struct DriverRegistry {
    drivers: RwLock<Vec<Arc<dyn Driver>>>,
    bindings: RwLock<BTreeMap<DeviceId, Binding>>,
}


impl DriverRegistry {
    pub const fn new() -> Self {
        Self { drivers: RwLock::new(Vec::new()), bindings: RwLock::new(BTreeMap::new()) }
    }

    /// Register a driver. Called once per driver at kernel init.
    pub fn register_driver(&self, driver: Arc<dyn Driver>) { self.drivers.write().push(driver); }

    /// Try to find and bind a driver for one device.
    /// Called by bind_all() and can also be called on hotplug.
    pub fn try_bind(
        &self,
        device_id: DeviceId,
        device: Arc<dyn device::Device>,
    ) -> Result<(), DriverError> {
        if self.bindings.read().contains_key(&device_id) {
            return Err(DriverError::AlreadyBound);
        }

        let driver = self.find_driver(&device)?;
        let name = driver.name();
        let state = driver.bind(device_id, Arc::clone(&device))?;

        let _ = self.bindings.write().insert(device_id, Binding::new(device_id, device, name, state));

        Ok(())
    }

    /// Walk every device in the device registry and try to bind a driver.
    /// Call this once after all drivers are registered and PCI is enumerated.
    pub fn bind_all(&self, devices: &DeviceRegistry) {
        // Collect PCI devices first to avoid holding the registry lock
        let pci_devices = devices.by_type(device::DeviceType::Bus);

        for (id, dev) in pci_devices {
            // Non-fatal — just means no driver for this device yet
            let _ = self.try_bind(id, dev);
        }
    }

    /// Unbind the driver from a device, dropping all its state.
    pub fn unbind(&self, device_id: DeviceId) -> Result<(), DriverError> {
        self.bindings.write().remove(&device_id).ok_or(DriverError::NotBound)?;
        // Binding::drop() calls state.stop() automatically
        Ok(())
    }

    /// Get the binding for a device, if any.
    pub fn binding_for(&self, device_id: DeviceId) -> Option<DeviceId> {
        if self.bindings.read().contains_key(&device_id) { Some(device_id) } else { None }
    }

    /// How many devices currently have a driver bound.
    pub fn bound_count(&self) -> usize { self.bindings.read().len() }

    /// Extract the first available block device from bound drivers.
    /// Returns (device_id, block_device) or None if no block devices found.
    pub fn take_block_device(&self) -> Option<(DeviceId, BlockDeviceEnum)> {
        let mut bindings = self.bindings.write();
        let device_ids: Vec<DeviceId> = bindings.keys().copied().collect();

        for id in device_ids {
            if let Some(binding) = bindings.remove(&id) {
                // Try to convert this binding to a block device
                if let Some(block_dev) = binding.into_block_device() {
                    return Some((id, block_dev));
                }
                // Not a block device - you may want to re-insert it or drop it
                // For now, we drop non-block drivers (they're removed from
                // registry)
            }
        }
        None
    }

    /// Alternative: Get all block devices (for multi-disk systems)
    pub fn take_all_block_devices(&self) -> Vec<(DeviceId, BlockDeviceEnum)> {
        let mut bindings = self.bindings.write();
        let ids: Vec<DeviceId> = bindings.keys().copied().collect();
        let mut result = Vec::new();

        for id in ids {
            if let Some(binding) = bindings.remove(&id) {
                let devs = binding.into_block_devices();
                for dev in devs {
                    result.push((id, dev));
                }
            }
        }
        result
    }


    pub fn for_each_block<F>(&self, mut f: F)
    where
        F: FnMut(DeviceId, &str, &mut dyn hal::block::BlockDevice),
    {
        // Collect raw pointers while lock is held
        struct BlockEntry {
            id: DeviceId,
            ptr: *mut dyn hal::block::BlockDevice,
            name: &'static str,
        }
        // SAFETY: BlockEntry holds no references to bindings, just raw pointers
        // to the BlockDevice trait objects which are heap-allocated via Box in state
        unsafe impl Send for BlockEntry {}

        let entries: Vec<BlockEntry> = {
            let mut bindings = self.bindings.write();
            let mut vec = Vec::new();

            for (id, binding) in bindings.iter_mut() {
                if let Some(ptr) = binding
                    .state
                    .as_mut()
                    .and_then(|s| s.as_block_device_ptr())
                {
                    vec.push(BlockEntry {
                        id: *id,
                        ptr,
                        name: binding.driver_name,
                    });
                }
            }
            vec
        }; // <-- lock dropped here

        // Now safe to call f with lock released
        for entry in entries {
            unsafe {
                f(entry.id, entry.name, &mut *entry.ptr);
            }
        }
    }

    // ── Private ───────────────────────────────────────────────────────────────

    fn find_driver(
        &self,
        device: &Arc<dyn device::Device>,
    ) -> Result<Arc<dyn Driver>, DriverError> {
        // Downcast to PciBusDevice to extract match info
        // Platform devices would have their own branch here later
        let pci = device.as_any().downcast_ref::<PciBusDevice>();

        let drivers = self.drivers.read();

        let mut best: Option<Arc<dyn Driver>> = None;
        let mut best_priority = i32::MIN;

        if let Some(pci_dev) = pci {
            let info = pci_dev.info();
            let vendor = info.vendor_id;
            let dev_id = info.device_id;
            let class = info.class;
            let subclass = info.subclass;

            for driver in drivers.iter() {
                let matches = driver
                    .rules()
                    .iter()
                    .any(|rule| rule.matches_pci(vendor, dev_id, class, subclass));

                if matches && driver.priority() > best_priority {
                    best = Some(Arc::clone(driver));
                    best_priority = driver.priority();
                }
            }
        }

        best.ok_or(DriverError::NoDriverFound)
    }
}
