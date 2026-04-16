use alloc::{boxed::Box, sync::Arc, vec::Vec};
use core::ptr;

use bus::pci::PciBusDevice;
use device::{Device, DeviceId};
use hal::{
    block::{BlockDevice, BlockError},
    dma::DmaAllocator,
};

use crate::{Driver, DriverError, DriverState, MatchRule};

mod constants;
mod fis;
pub(crate) mod port;

use constants::*;
use port::PortState;

use crate::block::{AhciPortWrapper, BlockDeviceEnum};

pub struct AhciDriver;

impl Driver for AhciDriver {
    fn name(&self) -> &'static str { "ahci" }

    fn rules(&self) -> &[MatchRule] { &[MatchRule::PciClass { class: 0x01, subclass: 0x06 }] }

    fn bind(
        &self,
        _device_id: DeviceId,
        device: Arc<dyn Device>,
    ) -> Result<Box<dyn DriverState>, DriverError> {
        let pci = device.as_any().downcast_ref::<PciBusDevice>().ok_or(DriverError::BindFailed)?;

        pci.enable_dma();
        pci.enable_mmio();

        let abar_phys = pci.info().bar_mmio(5).ok_or(DriverError::BindFailed)?;
        let phys_offset = crate::phys_offset();
        let mmio_base = (abar_phys.as_u64() + phys_offset) as usize;

        let mut dma_guard = crate::dma().lock();
        let state = unsafe {
            AhciState::init(mmio_base, &mut *dma_guard).map_err(|_| DriverError::BindFailed)?
        };

        Ok(Box::new(state))
    }
}

// ── Driver state (one per controller) ────────────────────────────────────────

pub struct AhciState {
    mmio_base: usize,
    ports: Vec<AhciPortWrapper>,
}

impl AhciState {
    pub unsafe fn init(mmio_base: usize, dma: &mut dyn DmaAllocator) -> Result<Self, &'static str> {
        unsafe {
            // Enable AHCI mode
            let ghc = (mmio_base + HBA_GHC) as *mut u32;
            ptr::write_volatile(ghc, ptr::read_volatile(ghc) | (1 << 31));

            // Walk all implemented ports
            let pi = ptr::read_volatile((mmio_base + HBA_PI) as *const u32);
            let mut ports = Vec::new();

            for i in 0u8..32 {
                if pi & (1 << i) == 0 {
                    continue;
                }

                // Check port is connected (SATA device present)
                let port_base = mmio_base + 0x100 + (i as usize) * 0x80;
                let ssts = ptr::read_volatile((port_base + PORT_SSTS) as *const u32);
                let det = ssts & 0xF;
                let ipm = (ssts >> 8) & 0xF;

                // DET=1|3 means device present, IPM=1 means active
                if det != 1 && det != 3 {
                    continue;
                }
                if ipm != 1 {
                    continue;
                }

                match PortState::init(mmio_base, i as usize, dma) {
                    Ok(mut state) => {
                        // Issue IDENTIFY to get real sector count
                        let sector_count = unsafe {
                            identify_sector_count(&mut state).unwrap_or(131_071) // safe fallback
                        };

                        ports.push(AhciPortWrapper { port_idx: i as u8, state, sector_count });
                    }
                    Err(_) => continue, // port init failed, skip
                }
            }

            if ports.is_empty() {
                return Err("AHCI: no usable ports found");
            }

            Ok(Self { mmio_base, ports })
        }
    }

    /// Number of discovered ports with attached devices
    pub fn port_count(&self) -> usize { self.ports.len() }

    /// Get a block device handle for port n
    pub fn port(&mut self, n: usize) -> Option<AhciBlockDevice<'_>> {
        self.ports.get_mut(n).map(|p| AhciBlockDevice { port: p })
    }
}

impl DriverState for AhciState {
    fn stop(&self) {
        for port in &self.ports {
            unsafe {
                let port_base = self.mmio_base + 0x100 + (port.port_idx as usize) * 0x80;
                let cmd = (port_base + PORT_CMD) as *mut u32;
                let val = ptr::read_volatile(cmd);
                ptr::write_volatile(cmd, val & !((1 << 0) | (1 << 4)));
            }
        }
    }

    fn as_block_device(self: Box<Self>) -> Option<BlockDeviceEnum> { None }

    fn into_block_devices(self: Box<Self>) -> Vec<BlockDeviceEnum> {
        let mut this = *self;
        this.ports
            .drain(..)
            .map(|w| BlockDeviceEnum::Ahci(Box::new(w)))
            .collect()
    }

    fn as_block_device_ref(&mut self) -> Option<&mut dyn BlockDevice> {
        self.ports.first_mut().map(|w| w as &mut dyn BlockDevice)
    }
}

// ── Per-port BlockDevice wrapper

pub struct AhciBlockDevice<'a> {
    port: &'a mut AhciPortWrapper,
}


// ── IDENTIFY DEVICE
// ───────────────────────────────────────────────────────────

unsafe fn identify_sector_count(port: &mut PortState) -> Option<u64> {
    unsafe {
        let mut buf = [0u8; 512];
        port.identify(&mut buf).ok()?;

        // Words 100-103: 48-bit LBA total sectors
        let lo = u16::from_le_bytes([buf[200], buf[201]]) as u64;
        let hi = u16::from_le_bytes([buf[202], buf[203]]) as u64;
        let count = lo | (hi << 16);

        if count == 0 { None } else { Some(count) }
    }
}
