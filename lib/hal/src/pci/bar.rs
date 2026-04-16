use super::ecam;
use crate::PhysAddr;

#[derive(Debug, Clone, Copy)]
pub enum Bar {
    Memory { base: PhysAddr, size: usize, prefetchable: bool },
    Io { port: u16, size: usize },
    Empty,
}

impl Bar {
    pub fn mmio_phys(&self) -> Option<PhysAddr> {
        match self {
            Bar::Memory { base, .. } => Some(*base),
            _ => None,
        }
    }

    pub fn io_port(&self) -> Option<u16> {
        match self {
            Bar::Io { port, .. } => Some(*port),
            _ => None,
        }
    }

    pub fn size(&self) -> usize {
        match self {
            Bar::Memory { size, .. } => *size,
            Bar::Io { size, .. } => *size as usize,
            Bar::Empty => 0,
        }
    }
}

/// Returns (bar, slots_consumed). 64-bit BARs consume 2 slots.
pub(super) unsafe fn probe(bus: u8, dev: u8, func: u8, idx: usize) -> (Bar, usize) {
    let offset = (0x10 + idx * 4) as u16;
    let orig = unsafe { ecam::read32(bus, dev, func, offset) };

    if orig == 0 || orig == 0xFFFF_FFFF {
        return (Bar::Empty, 1);
    }

    if orig & 1 != 0 {
        // I/O BAR
        unsafe { ecam::write32(bus, dev, func, offset, 0xFFFF_FFFF) };
        let mask = unsafe { ecam::read32(bus, dev, func, offset) };
        unsafe { ecam::write32(bus, dev, func, offset, orig) };
        let size = (!(mask & !0xF_u32)).wrapping_add(1) as usize;
        return (Bar::Io { port: (orig & 0xFFFC) as u16, size }, 1);
    }

    let bar_type = (orig >> 1) & 0x3;
    let prefetchable = (orig >> 3) & 1 != 0;

    match bar_type {
        0x0 => {
            // 32-bit MMIO
            unsafe { ecam::write32(bus, dev, func, offset, 0xFFFF_FFFF) };
            let mask = unsafe { ecam::read32(bus, dev, func, offset) };
            unsafe { ecam::write32(bus, dev, func, offset, orig) };
            let size = (!(mask & !0xF_u32)).wrapping_add(1) as usize;
            let base = PhysAddr(orig as u64 & !0xF);
            (Bar::Memory { base, size, prefetchable }, 1)
        }
        0x2 => {
            // 64-bit MMIO — consumes this slot + the next
            let offset_hi = offset + 4;
            let orig_hi = unsafe { ecam::read32(bus, dev, func, offset_hi) };

            unsafe { ecam::write32(bus, dev, func, offset, 0xFFFF_FFFF) };
            unsafe { ecam::write32(bus, dev, func, offset_hi, 0xFFFF_FFFF) };
            let mask_lo = unsafe { ecam::read32(bus, dev, func, offset) } as u64;
            let mask_hi = unsafe { ecam::read32(bus, dev, func, offset_hi) } as u64;
            unsafe { ecam::write32(bus, dev, func, offset, orig) };
            unsafe { ecam::write32(bus, dev, func, offset_hi, orig_hi) };

            let mask64 = (mask_lo & !0xF) | (mask_hi << 32);
            let size = (!mask64).wrapping_add(1) as usize;
            let base = PhysAddr((orig as u64 & !0xF) | ((orig_hi as u64) << 32));
            (Bar::Memory { base, size, prefetchable }, 2)
        }
        _ => (Bar::Empty, 1),
    }
}

pub(super) unsafe fn parse_all(bus: u8, dev: u8, func: u8) -> [Bar; 6] {
    let mut bars = [Bar::Empty; 6];
    let mut i = 0;
    while i < 6 {
        let (bar, consumed) = unsafe { probe(bus, dev, func, i) };
        bars[i] = bar;
        i += consumed;
    }
    bars
}
