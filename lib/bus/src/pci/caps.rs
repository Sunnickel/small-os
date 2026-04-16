use hal::pci::PciDeviceInfo;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CapabilityId {
    Msi,
    MsiX,
    Pcie,
    VendorSpecific,
    PowerManagement,
    Unknown(u8),
}

impl CapabilityId {
    fn from_u8(v: u8) -> Self {
        match v {
            0x01 => Self::PowerManagement,
            0x05 => Self::Msi,
            0x10 => Self::Pcie,
            0x11 => Self::MsiX,
            0x09 => Self::VendorSpecific,
            other => Self::Unknown(other),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Capability {
    pub id: CapabilityId,
    pub offset: u16, // offset in config space where this cap starts
}

/// Walk the capability linked list and return all caps found.
pub fn parse_capabilities(dev: &PciDeviceInfo) -> alloc::vec::Vec<Capability> {
    let mut caps = alloc::vec::Vec::new();

    // Status register bit 4 — capabilities list present
    let status = dev.read16(0x06);
    if status & (1 << 4) == 0 {
        return caps;
    }

    let mut offset = (dev.read8(0x34) & !0x3) as u16;

    // Guard against malformed cap lists — max 48 caps in config space
    for _ in 0..48 {
        if offset < 0x40 || offset > 0xFC {
            break;
        }
        let id = dev.read8(offset);
        let next = dev.read8(offset + 1) & !0x3;

        caps.push(Capability { id: CapabilityId::from_u8(id), offset });

        if next == 0 {
            break;
        }
        offset = next as u16;
    }

    caps
}

/// Find the config space offset of a specific capability.
pub fn find_capability(dev: &PciDeviceInfo, id: CapabilityId) -> Option<u16> {
    parse_capabilities(dev).into_iter().find(|c| c.id == id).map(|c| c.offset)
}
