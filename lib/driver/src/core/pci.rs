use alloc::vec::Vec;
use core::sync::atomic::{AtomicU64, Ordering};

pub const VENDOR_VIRTIO: u16 = 0x1AF4;
pub const DEVICE_VIRTIO_BLK: u16 = 0x1001; // legacy
pub const DEVICE_VIRTIO_BLK_M: u16 = 0x1042; // modern
pub const VENDOR_INTEL: u16 = 0x8086;
pub const CLASS_STORAGE: u8 = 0x01;
pub const SUBCLASS_AHCI: u8 = 0x06;
pub const HEADER_TYPE_MF: u8 = 0x80;

/// Physical base address of the ECAM region (segment 0, bus 0).
/// Must be set via init_ecam() before calling scan().
static ECAM_PHYS_BASE: AtomicU64 = AtomicU64::new(0);

/// Physical-to-virtual translation offset (phys + offset = virt).
/// Set alongside ECAM_PHYS_BASE.
static PHYS_OFFSET: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, Copy)]
pub enum Bar {
    Mmio32 { base: u32, size: u32, prefetchable: bool },
    Mmio64 { base: u64, size: u64, prefetchable: bool },
    Io { port: u16, size: u16 },
    Empty,
}

impl Bar {
    /// Physical MMIO base, regardless of width. None for I/O or empty.
    pub fn mmio_phys(&self) -> Option<u64> {
        match self {
            Bar::Mmio32 { base, .. } => Some(*base as u64),
            Bar::Mmio64 { base, .. } => Some(*base),
            _ => None,
        }
    }

    /// I/O port base. None for MMIO or empty.
    pub fn io_port(&self) -> Option<u16> {
        match self {
            Bar::Io { port, .. } => Some(*port),
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct PciDevice {
    pub bus: u8,
    pub device: u8,
    pub func: u8,
    pub vendor_id: u16,
    pub device_id: u16,
    pub class: u8,
    pub subclass: u8,
    pub prog_if: u8,
    pub revision: u8,
    pub header_type: u8,
    pub bars: [Bar; 6],
    // ── Legacy compat fields (used in driver::fs) ──────────────────────
    // Populated from bars[0] and bars[5] during scan.
    pub bar0_phys: Option<u64>, // BAR0 MMIO base (VirtIO uses this)
    pub bar5_phys: Option<u64>, // BAR5 MMIO base (AHCI ABAR)
    pub bar0_io: Option<u16>,   // BAR0 I/O port  (legacy VirtIO)
}

impl PciDevice {
    pub fn location(&self) -> alloc::string::String {
        alloc::format!("{:02x}:{:02x}.{}", self.bus, self.device, self.func)
    }

    /// Read byte from PCI config space
    pub fn read_config_byte(&self, offset: u16) -> u8 {
        unsafe { ecam_read8(self.bus, self.device, self.func, offset) }
    }

    /// Read word from PCI config space
    pub fn read_config_word(&self, offset: u16) -> u16 {
        unsafe { ecam_read16(self.bus, self.device, self.func, offset) }
    }

    /// Read dword from PCI config space
    pub fn read_config_dword(&self, offset: u16) -> u32 {
        unsafe { ecam_read32(self.bus, self.device, self.func, offset) }
    }

    /// Write byte to PCI config space
    pub fn write_config_byte(&self, offset: u16, value: u8) {
        unsafe { ecam_write8(self.bus, self.device, self.func, offset, value) }
    }

    /// Write word to PCI config space
    pub fn write_config_word(&self, offset: u16, value: u16) {
        unsafe { ecam_write16(self.bus, self.device, self.func, offset, value) }
    }

    /// Write dword to PCI config space
    pub fn write_config_dword(&self, offset: u16, value: u32) {
        unsafe { ecam_write32(self.bus, self.device, self.func, offset, value) }
    }

    /// Get BAR physical address
    pub fn bar_phys(&self, bar_idx: usize) -> Option<u64> { self.bars.get(bar_idx)?.mmio_phys() }
}

pub fn scan() -> impl Iterator<Item = PciDevice> {
    assert!(
        ECAM_PHYS_BASE.load(Ordering::Relaxed) != 0,
        "pci::scan() called before pci::init_ecam()"
    );

    let mut results = Vec::new();

    for bus in 0u8..=255 {
        for device in 0u8..32 {
            // Function 0 must exist
            let val0 = unsafe { read32(bus, device, 0, 0x00) };
            if (val0 & 0xFFFF) as u16 == 0xFFFF {
                continue;
            }

            let ht0 = unsafe { read8(bus, device, 0, 0x0E) };
            let multi = ht0 & HEADER_TYPE_MF != 0;
            let func_n = if multi { 8 } else { 1 };

            for func in 0..func_n {
                let val = unsafe { read32(bus, device, func, 0x00) };
                let vendor_id = (val & 0xFFFF) as u16;
                if vendor_id == 0xFFFF {
                    continue;
                }
                let device_id = (val >> 16) as u16;

                let class_dw = unsafe { read32(bus, device, func, 0x08) };
                let revision = (class_dw & 0xFF) as u8;
                let prog_if = ((class_dw >> 8) & 0xFF) as u8;
                let subclass = ((class_dw >> 16) & 0xFF) as u8;
                let class = ((class_dw >> 24) & 0xFF) as u8;

                let header_type = unsafe { read8(bus, device, func, 0x0E) } & !HEADER_TYPE_MF;

                let bars = if header_type == 0 {
                    unsafe { parse_bars(bus, device, func) }
                } else {
                    [Bar::Empty; 6]
                };

                // Populate legacy compat fields
                let bar5_phys = bars[5].mmio_phys();
                let bar0_phys = bars.iter().find_map(Bar::mmio_phys);
                let bar0_io = bars.iter().find_map(Bar::io_port);

                results.push(PciDevice {
                    bus,
                    device,
                    func,
                    vendor_id,
                    device_id,
                    class,
                    subclass,
                    prog_if,
                    revision,
                    header_type,
                    bars,
                    bar0_phys,
                    bar5_phys,
                    bar0_io,
                });
            }
        }
    }

    results.into_iter()
}

pub unsafe fn ecam_read32(bus: u8, device: u8, func: u8, offset: u16) -> u32 {
    read32(bus, device, func, offset)
}
pub unsafe fn ecam_read16(bus: u8, device: u8, func: u8, offset: u16) -> u16 {
    read16(bus, device, func, offset)
}
pub unsafe fn ecam_read8(bus: u8, device: u8, func: u8, offset: u16) -> u8 {
    read8(bus, device, func, offset)
}

pub unsafe fn ecam_write32(bus: u8, device: u8, func: u8, offset: u16, value: u32) {
    write32(bus, device, func, offset, value)
}
pub unsafe fn ecam_write16(bus: u8, device: u8, func: u8, offset: u16, value: u16) {
    let current = read32(bus, device, func, offset & !3);
    let shift = (offset & 2) * 8;
    let mask = !(0xFFFFu32 << shift);
    let new = (current & mask) | ((value as u32) << shift);
    write32(bus, device, func, offset & !3, new);
}
pub unsafe fn ecam_write8(bus: u8, device: u8, func: u8, offset: u16, value: u8) {
    let current = read32(bus, device, func, offset & !3);
    let shift = (offset & 3) * 8;
    let mask = !(0xFFu32 << shift);
    let new = (current & mask) | ((value as u32) << shift);
    write32(bus, device, func, offset & !3, new);
}

/// Call this once during kernel init, before scan().
///
/// `ecam_phys` — base address of ECAM region from MCFG (bus 0, segment 0).
/// `phys_offset` — your kernel's physical memory offset
///                 (boot_info.physical_memory_offset).
pub fn init_ecam(ecam_phys: u64, phys_offset: u64) {
    ECAM_PHYS_BASE.store(ecam_phys, Ordering::Relaxed);
    PHYS_OFFSET.store(phys_offset, Ordering::Relaxed);
}

unsafe fn ecam_ptr(bus: u8, device: u8, func: u8, offset: u16) -> *mut u32 {
    let base = ECAM_PHYS_BASE.load(Ordering::Relaxed);
    let phys_offset = PHYS_OFFSET.load(Ordering::Relaxed);
    let phys = base
        + ((bus as u64) << 20)
        + ((device as u64) << 15)
        + ((func as u64) << 12)
        + (offset as u64 & 0xFFC);
    (phys + phys_offset) as *mut u32
}

unsafe fn read32(bus: u8, device: u8, func: u8, offset: u16) -> u32 {
    core::ptr::read_volatile(ecam_ptr(bus, device, func, offset))
}

unsafe fn read16(bus: u8, device: u8, func: u8, offset: u16) -> u16 {
    let word = read32(bus, device, func, offset & !3);
    (word >> ((offset & 2) * 8)) as u16
}

unsafe fn read8(bus: u8, device: u8, func: u8, offset: u16) -> u8 {
    let word = read32(bus, device, func, offset & !3);
    (word >> ((offset & 3) * 8)) as u8
}

unsafe fn write32(bus: u8, device: u8, func: u8, offset: u16, value: u32) {
    core::ptr::write_volatile(ecam_ptr(bus, device, func, offset), value);
}

unsafe fn probe_bar(bus: u8, dev: u8, func: u8, bar_idx: usize) -> (Bar, usize) {
    let offset = (0x10 + bar_idx * 4) as u16;
    let orig = read32(bus, dev, func, offset);

    if orig == 0 || orig == 0xFFFF_FFFF {
        return (Bar::Empty, 1);
    }

    if orig & 1 != 0 {
        // I/O BAR
        write32(bus, dev, func, offset, 0xFFFF_FFFF);
        let mask = read32(bus, dev, func, offset);
        write32(bus, dev, func, offset, orig);
        let size = u16::try_from((!(mask & !0xF_u32)).wrapping_add(1)).unwrap();
        return (Bar::Io { port: (orig & 0xFFFC) as u16, size }, 1);
    }

    let bar_type = (orig >> 1) & 0x3;
    let prefetchable = (orig >> 3) & 1 != 0;

    match bar_type {
        0x0 => {
            // 32-bit MMIO
            write32(bus, dev, func, offset, 0xFFFF_FFFF);
            let mask = read32(bus, dev, func, offset);
            write32(bus, dev, func, offset, orig);
            let size = !(mask & !0xF_u32).wrapping_add(1);
            (Bar::Mmio32 { base: orig & !0xF, size, prefetchable }, 1)
        }
        0x2 => {
            // 64-bit MMIO — consumes this slot and the next
            let offset_hi = offset + 4;
            let orig_hi = read32(bus, dev, func, offset_hi);

            write32(bus, dev, func, offset, 0xFFFF_FFFF);
            write32(bus, dev, func, offset_hi, 0xFFFF_FFFF);
            let mask_lo = read32(bus, dev, func, offset) as u64;
            let mask_hi = read32(bus, dev, func, offset_hi) as u64;
            write32(bus, dev, func, offset, orig);
            write32(bus, dev, func, offset_hi, orig_hi);

            let mask64 = (mask_lo & !0xF) | (mask_hi << 32);
            let size = (!mask64).wrapping_add(1);
            let base = (orig as u64 & !0xF) | ((orig_hi as u64) << 32);
            (Bar::Mmio64 { base, size, prefetchable }, 2)
        }
        _ => (Bar::Empty, 1),
    }
}

unsafe fn parse_bars(bus: u8, dev: u8, func: u8) -> [Bar; 6] {
    let mut bars = [Bar::Empty; 6];
    let mut i = 0usize;
    while i < 6 {
        let (bar, consumed) = probe_bar(bus, dev, func, i);
        bars[i] = bar;
        i += consumed;
    }
    bars
}
