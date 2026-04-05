#[repr(C)]
pub struct VirtqDesc {
    pub addr: u64,
    pub len: u32,
    pub flags: u16,
    pub next: u16,
}

#[repr(C)]
pub struct VirtqAvail {
    pub flags: u16,
    pub idx: u16,
    pub ring: [u16; 64],
    pub used_event: u16,
}

#[repr(C)]
pub struct VirtqUsedElem {
    pub id: u32,
    pub len: u32,
}

#[repr(C)]
pub struct VirtqUsed {
    pub flags: u16,
    pub idx: u16,
    pub ring: [VirtqUsedElem; 64],
    pub avail_event: u16,
}

pub struct VirtQueue {
    pub desc: usize,
    pub avail: usize,
    pub used: usize,
    pub queue_size: u16,
    pub last_used_idx: u16,
}

#[repr(C)]
pub struct VirtioBlkReq {
    pub type_: u32,
    pub reserved: u32,
    pub sector: u64,
}

/// Parsed VirtIO PCI capability
#[derive(Debug, Clone, Copy)]
pub struct VirtioCapInfo {
    pub cfg_type: u8,
    pub bar: u8,
    pub offset: u32,
    pub length: u32,
    pub cap_offset: u16,
}
