mod bar;
mod ecam;

pub use bar::Bar;
pub use ecam::init_ecam;

use crate::PhysAddr;

const HEADER_TYPE_MF: u8 = 0x80;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PciAddress {
    pub bus: u8,
    pub device: u8,
    pub function: u8,
}

#[derive(Debug, Clone)]
pub struct PciDeviceInfo {
    pub address: PciAddress,
    pub vendor_id: u16,
    pub device_id: u16,
    pub class: u8,
    pub subclass: u8,
    pub prog_if: u8,
    pub revision: u8,
    pub bars: [Bar; 6],
    pub irq_line: Option<u8>,
}

impl PciDeviceInfo {
    /// Shorthand: BAR n MMIO base.
    pub fn bar_mmio(&self, idx: usize) -> Option<PhysAddr> { self.bars.get(idx)?.mmio_phys() }

    /// Shorthand: BAR n I/O port.
    pub fn bar_io(&self, idx: usize) -> Option<u16> { self.bars.get(idx)?.io_port() }

    /// Config space read — drivers call this, not ecam directly.
    pub fn read8(&self, off: u16) -> u8 {
        unsafe { ecam::read8(self.address.bus, self.address.device, self.address.function, off) }
    }
    pub fn read16(&self, off: u16) -> u16 {
        unsafe { ecam::read16(self.address.bus, self.address.device, self.address.function, off) }
    }
    pub fn read32(&self, off: u16) -> u32 {
        unsafe { ecam::read32(self.address.bus, self.address.device, self.address.function, off) }
    }
    pub fn write8(&self, off: u16, val: u8) {
        unsafe {
            ecam::write8(self.address.bus, self.address.device, self.address.function, off, val)
        }
    }
    pub fn write16(&self, off: u16, val: u16) {
        unsafe {
            ecam::write16(self.address.bus, self.address.device, self.address.function, off, val)
        }
    }
    pub fn write32(&self, off: u16, val: u32) {
        unsafe {
            ecam::write32(self.address.bus, self.address.device, self.address.function, off, val)
        }
    }

    /// Enable bus-mastering DMA in the command register.
    pub fn enable_bus_master(&self) {
        let cmd = self.read16(0x04);
        self.write16(0x04, cmd | (1 << 2));
    }

    /// Enable MMIO decoding.
    pub fn enable_mmio(&self) {
        let cmd = self.read16(0x04);
        self.write16(0x04, cmd | (1 << 1));
    }
}

/// Walk every PCI function and call `f` for each one found.
/// Must call `init_ecam()` before this.
pub fn enumerate<F>(mut f: F)
where
    F: FnMut(PciDeviceInfo),
{
    assert!(ecam::is_ready(), "pci::enumerate() called before pci::init_ecam()");

    for bus in 0u8..=255 {
        for device in 0u8..32 {
            // Check function 0 exists
            let val0 = unsafe { ecam::read32(bus, device, 0, 0x00) };
            if val0 & 0xFFFF == 0xFFFF {
                continue;
            }

            let ht0 = unsafe { ecam::read8(bus, device, 0, 0x0E) };
            let funcs = if ht0 & HEADER_TYPE_MF != 0 { 8u8 } else { 1u8 };

            for func in 0..funcs {
                let val = unsafe { ecam::read32(bus, device, func, 0x00) };
                if val & 0xFFFF == 0xFFFF {
                    continue;
                }

                let vendor_id = (val & 0xFFFF) as u16;
                let device_id = (val >> 16) as u16;

                let class_dw = unsafe { ecam::read32(bus, device, func, 0x08) };
                let revision = (class_dw & 0xFF) as u8;
                let prog_if = ((class_dw >> 8) & 0xFF) as u8;
                let subclass = ((class_dw >> 16) & 0xFF) as u8;
                let class = ((class_dw >> 24) & 0xFF) as u8;

                let header_type = unsafe { ecam::read8(bus, device, func, 0x0E) } & !HEADER_TYPE_MF;

                let bars = if header_type == 0 {
                    unsafe { bar::parse_all(bus, device, func) }
                } else {
                    [Bar::Empty; 6]
                };

                let irq_line = {
                    let v = unsafe { ecam::read8(bus, device, func, 0x3C) };
                    if v == 0xFF { None } else { Some(v) }
                };

                f(PciDeviceInfo {
                    address: PciAddress { bus, device, function: func },
                    vendor_id,
                    device_id,
                    class,
                    subclass,
                    prog_if,
                    revision,
                    bars,
                    irq_line,
                });
            }
        }
    }
}

pub fn is_ready() -> bool { ecam::is_ready() }

pub unsafe fn config_read8(bus: u8, dev: u8, func: u8, offset: u16) -> u8 {
    unsafe { ecam::read8(bus, dev, func, offset) }
}

pub unsafe fn config_read16(bus: u8, dev: u8, func: u8, offset: u16) -> u16 {
    unsafe { ecam::read16(bus, dev, func, offset) }
}

pub unsafe fn config_read32(bus: u8, dev: u8, func: u8, offset: u16) -> u32 {
    unsafe { ecam::read32(bus, dev, func, offset) }
}
