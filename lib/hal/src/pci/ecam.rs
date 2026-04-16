use core::sync::atomic::{AtomicU64, Ordering};

static ECAM_PHYS_BASE: AtomicU64 = AtomicU64::new(0);
static PHYS_OFFSET:    AtomicU64 = AtomicU64::new(0);

pub fn init_ecam(ecam_phys: u64, phys_offset: u64) {
	ECAM_PHYS_BASE.store(ecam_phys, Ordering::Relaxed);
	PHYS_OFFSET.store(phys_offset, Ordering::Relaxed);
}

pub fn is_ready() -> bool {
	ECAM_PHYS_BASE.load(Ordering::Relaxed) != 0
}

unsafe fn ecam_ptr(bus: u8, device: u8, func: u8, offset: u16) -> *mut u32 {
	let base       = ECAM_PHYS_BASE.load(Ordering::Relaxed);
	let phys_offset = PHYS_OFFSET.load(Ordering::Relaxed);
	let phys = base
		+ ((bus    as u64) << 20)
		+ ((device as u64) << 15)
		+ ((func   as u64) << 12)
		+ (offset as u64 & 0xFFC);
	(phys + phys_offset) as *mut u32
}

// ── raw accessors (pub(super) — only pci:: internals use these) ──────────

pub(super) unsafe fn read32(bus: u8, dev: u8, func: u8, off: u16) -> u32 {
	unsafe { core::ptr::read_volatile(ecam_ptr(bus, dev, func, off)) }
}

pub(super) unsafe fn read16(bus: u8, dev: u8, func: u8, off: u16) -> u16 {
	let w = unsafe { read32(bus, dev, func, off & !3) };
	(w >> ((off & 2) * 8)) as u16
}

pub(super) unsafe fn read8(bus: u8, dev: u8, func: u8, off: u16) -> u8 {
	let w = unsafe { read32(bus, dev, func, off & !3) };
	(w >> ((off & 3) * 8)) as u8
}

pub(super) unsafe fn write32(bus: u8, dev: u8, func: u8, off: u16, val: u32) {
	unsafe { core::ptr::write_volatile(ecam_ptr(bus, dev, func, off), val) }
}

pub(super) unsafe fn write16(bus: u8, dev: u8, func: u8, off: u16, val: u16) {
	let cur   = unsafe { read32(bus, dev, func, off & !3) };
	let shift = (off & 2) * 8;
	let new   = (cur & !(0xFFFF << shift)) | ((val as u32) << shift);
	unsafe { write32(bus, dev, func, off & !3, new) }
}

pub(super) unsafe fn write8(bus: u8, dev: u8, func: u8, off: u16, val: u8) {
	let cur   = unsafe { read32(bus, dev, func, off & !3) };
	let shift = (off & 3) * 8;
	let new   = (cur & !(0xFF << shift)) | ((val as u32) << shift);
	unsafe { write32(bus, dev, func, off & !3, new) }
}