use x86_64::instructions::port::Port;

const CONFIG_ADDRESS: u16 = 0xCF8;
const CONFIG_DATA: u16 = 0xCFC;

// Well-known IDs
pub const VENDOR_VIRTIO: u16 = 0x1AF4;
pub const DEVICE_VIRTIO_BLK: u16 = 0x1001;
pub const CLASS_STORAGE: u8 = 0x01;
pub const SUBCLASS_AHCI: u8 = 0x06;

#[derive(Debug, Clone, Copy)]
pub struct PciDevice {
    pub bus: u8,
    pub device: u8,
    pub func: u8,
    pub vendor_id: u16,
    pub device_id: u16,
    pub class: u8,
    pub subclass: u8,
    pub bar0_phys: Option<u64>,
    pub bar0_io: Option<u16>,
}

pub fn scan() -> impl Iterator<Item = PciDevice> {
    let mut results = alloc::vec::Vec::new();

    for bus in 0u8..=255 {
        for device in 0u8..32 {
            for func in 0u8..8 {
                let val = pci_read32(bus, device, func, 0x00);
                let vendor_id = (val & 0xFFFF) as u16;
                if vendor_id == 0xFFFF {
                    continue;
                }
                let device_id = ((val >> 16) & 0xFFFF) as u16;
                let class_info = pci_read32(bus, device, func, 0x08);
                let class = ((class_info >> 24) & 0xFF) as u8;
                let subclass = ((class_info >> 16) & 0xFF) as u8;

                let mut virtio_bar_idx: Option<u8> = None;
                {
                    let status_cmd = pci_read32(bus, device, func, 0x04);
                    let has_caps = (status_cmd >> 16) & 0x10 != 0;
                    if has_caps {
                        let mut ptr = (pci_read32(bus, device, func, 0x34) & 0xFF) as u8;
                        let mut guard = 0u8;
                        while ptr >= 0x40 && guard < 16 {
                            let dw0 = pci_read32(bus, device, func, ptr);
                            let cap_id = (dw0 & 0xFF) as u8;
                            let next_ptr = ((dw0 >> 8) & 0xFF) as u8;
                            let cfg_type = ((dw0 >> 24) & 0xFF) as u8;
                            if cap_id == 0x09 && cfg_type == 0x01 {
                                // VirtIO CommonCfg — this BAR has the control registers
                                let dw1 = pci_read32(bus, device, func, ptr + 4);
                                virtio_bar_idx = Some((dw1 & 0xFF) as u8);
                                break;
                            }
                            ptr = next_ptr;
                            guard += 1;
                        }
                    }
                }

                // Now read the specific BAR indicated by CommonCfg, or fall back to BAR scan
                let (bar0_phys, bar0_io) = if let Some(bar_idx) = virtio_bar_idx {
                    // Read the 64-bit BAR pair directly
                    let bar_off = 0x10 + bar_idx * 4;
                    let lo = pci_read32(bus, device, func, bar_off);
                    let hi = pci_read32(bus, device, func, bar_off + 4);
                    let addr = ((hi as u64) << 32) | ((lo as u64) & !0xF);
                    (Some(addr), None)
                } else {
                    let mut bar_phys: Option<u64> = None;
                    let mut bar_io: Option<u16> = None;

                    let mut i = 0u8;
                    while i < 6 {
                        let bar_off = 0x10 + (i as u16 * 4) as u8;
                        let raw = pci_read32(bus, device, func, bar_off);

                        if raw & 0x1 != 0 {
                            // I/O port BAR
                            if bar_io.is_none() {
                                bar_io = Some((raw & !0x3) as u16);
                            }
                            i += 1;
                        } else {
                            let bar_type = (raw >> 1) & 0x3;
                            let addr_low = (raw as u64) & !0xF;

                            if bar_type == 0x2 {
                                // 64-bit BAR — BAR[i+1] is the high half, must skip it
                                let high = if i < 5 {
                                    pci_read32(bus, device, func, bar_off + 4) as u64
                                } else {
                                    0
                                };
                                let full = (high << 32) | addr_low;
                                if bar_phys.is_none() && full != 0 {
                                    bar_phys = Some(full);
                                }
                                i += 2; // consume both halves
                            } else {
                                // 32-bit BAR
                                if bar_phys.is_none() && addr_low != 0 {
                                    bar_phys = Some(addr_low);
                                }
                                i += 1;
                            }
                        }
                    }
                    (bar_phys, bar_io)
                };
                results.push(PciDevice {
                    bus,
                    device,
                    func,
                    vendor_id,
                    device_id,
                    class,
                    subclass,
                    bar0_phys,
                    bar0_io,
                });
            }
        }
    }

    results.into_iter()
}

pub fn assign_bar64(bus: u8, device: u8, func: u8, bar_offset: u8, assign_addr: u64) {
    let cmd = pci_read32(bus, device, func, 0x04);
    // Disable Memory Space while reprogramming
    pci_write32(bus, device, func, 0x04, cmd & !0x2);

    let flags = pci_read32(bus, device, func, bar_offset) & 0xF;
    pci_write32(bus, device, func, bar_offset, (assign_addr & 0xFFFF_FFFF) as u32 | flags);
    pci_write32(bus, device, func, bar_offset + 4, (assign_addr >> 32) as u32);

    // Re-enable Memory Space + Bus Master
    pci_write32(bus, device, func, 0x04, cmd | 0x6);
}

pub fn pci_write32(bus: u8, device: u8, func: u8, offset: u8, value: u32) {
    let address: u32 = (1 << 31)
        | ((bus as u32) << 16)
        | ((device as u32) << 11)  // ← must be << 11, not << 8
        | ((func as u32) << 8)
        | ((offset as u32) & 0xFC);
    unsafe {
        Port::<u32>::new(CONFIG_ADDRESS).write(address); // 0xCF8
        Port::<u32>::new(CONFIG_DATA).write(value); // 0xCFC
    }
}

pub fn pci_read32(bus: u8, device: u8, func: u8, offset: u8) -> u32 {
    let address: u32 = (1 << 31)
        | ((bus as u32) << 16)
        | ((device as u32) << 11)
        | ((func as u32) << 8)
        | ((offset as u32) & 0xFC);

    unsafe {
        Port::<u32>::new(CONFIG_ADDRESS).write(address);
        Port::<u32>::new(CONFIG_DATA).read()
    }
}

pub fn pci_read_bar_io(bus: u8, device: u8, func: u8, bar: u8) -> Option<u16> {
    let offset = 0x10 + bar * 4;
    let raw = pci_read32(bus, device, func, offset);

    if raw & 0x1 == 0 {
        return None; // MMIO, not I/O port
    }

    Some((raw & !0x3) as u16) // mask off the low 2 flag bits
}
