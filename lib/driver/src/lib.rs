#![no_std]
#![no_main]
extern crate alloc;

pub mod binding;
pub mod error;
pub mod registry;

pub mod r#match;
pub mod block;

use alloc::{boxed::Box, sync::Arc};
use spin::{Once, Mutex};
pub use binding::{Binding, DriverState};
use device::{Device, DeviceId};
pub use error::DriverError;
use hal::dma::DmaAllocator;
pub use r#match::MatchRule;
pub use registry::DriverRegistry;

static PHYS_OFFSET: Once<u64> = Once::new();
static DMA: Once<&'static Mutex<dyn DmaAllocator + Send + Sync>> = Once::new();

pub fn init(phys_offset: u64, dma: &'static Mutex<dyn DmaAllocator + Send + Sync>) {
    PHYS_OFFSET.call_once(|| phys_offset);
    DMA.call_once(|| dma);
}

pub(crate) fn phys_offset() -> u64 {
    *PHYS_OFFSET.get().expect("driver::init() not called")
}

pub(crate) fn dma() -> &'static Mutex<dyn DmaAllocator + Send + Sync> {
    DMA.get().expect("driver::init() not called")
}

/// The core trait every driver implements.
pub trait Driver: Send + Sync {
    /// Human-readable name, e.g. "ahci", "virtio-blk"
    fn name(&self) -> &'static str;

    /// Rules checked during device matching.
    /// If any rule matches, bind() is called.
    fn rules(&self) -> &[MatchRule];

    /// Called when a matching device is found.
    /// Allocate hardware resources here and return driver state.
    /// Dropping the returned state must release all resources.
    fn bind(
        &self,
        device_id: DeviceId,
        device: Arc<dyn Device>,
    ) -> Result<Box<dyn DriverState>, DriverError>;

    /// Priority — higher wins when multiple drivers match.
    /// Default 0. Vendor-specific drivers can return 10 to beat generic ones.
    fn priority(&self) -> i32 { 0 }
}
