use alloc::{borrow::ToOwned, collections::BTreeMap, string::String, sync::Arc, vec::Vec};

use spin::RwLock;

use crate::{Device, DeviceError, DeviceHandle, DeviceId, DeviceType};

struct Entry {
    name: String,
    device: Arc<dyn Device>,
}

pub struct DeviceRegistry {
    devices: RwLock<BTreeMap<DeviceId, Entry>>,
    name_index: RwLock<BTreeMap<String, DeviceId>>,
}

impl DeviceRegistry {
    pub const fn new() -> Self {
        Self { devices: RwLock::new(BTreeMap::new()), name_index: RwLock::new(BTreeMap::new()) }
    }

    /// Register a device. Calls probe() before inserting.
    /// Returns a handle the caller can store.
    pub fn register<D: Device + 'static>(
        &self,
        device: Arc<D>,
    ) -> Result<DeviceHandle<D>, DeviceError> {
        let name = device.name().to_owned();

        // Reject duplicate names
        if self.name_index.read().contains_key(&name) {
            return Err(DeviceError::AlreadyRegistered);
        }

        device.probe().map_err(|_| DeviceError::ProbeFailed)?;

        let id = DeviceId::allocate();

        self.devices.write().insert(
            id,
            Entry { name: name.clone(), device: Arc::clone(&device) as Arc<dyn Device> },
        );
        self.name_index.write().insert(name, id);

        Ok(DeviceHandle { id, inner: device })
    }

    /// Unregister by ID. Calls remove() on the device.
    pub fn unregister(&self, id: DeviceId) -> Result<(), DeviceError> {
        let entry = self.devices.write().remove(&id).ok_or(DeviceError::NotFound)?;

        self.name_index.write().remove(&entry.name);
        entry.device.remove();
        Ok(())
    }

    /// Look up by ID — returns the type-erased Arc.
    pub fn get(&self, id: DeviceId) -> Option<Arc<dyn Device>> {
        self.devices.read().get(&id).map(|e| Arc::clone(&e.device))
    }

    /// Look up by name.
    pub fn get_by_name(&self, name: &str) -> Option<Arc<dyn Device>> {
        let id = *self.name_index.read().get(name)?;
        self.get(id)
    }

    /// All devices of a given type.
    pub fn by_type(&self, ty: DeviceType) -> Vec<(DeviceId, Arc<dyn Device>)> {
        self.devices
            .read()
            .iter()
            .filter(|(_, e)| e.device.device_type() == ty)
            .map(|(id, e)| (*id, Arc::clone(&e.device)))
            .collect()
    }

    /// Total number of registered devices.
    pub fn len(&self) -> usize { self.devices.read().len() }
}
