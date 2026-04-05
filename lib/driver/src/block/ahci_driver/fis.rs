/// Command header (one entry in the command list)
#[repr(C, align(128))]
pub struct CommandHeader {
    pub flags: u16,
    pub prdtl: u16,
    pub prdbc: u32,
    pub ctba: u32,
    pub ctbau: u32,
    pub reserved: [u32; 4],
}

/// Host-to-Device Register FIS
#[repr(C)]
pub struct FisRegH2D {
    pub fis_type: u8,
    pub pmport: u8,
    pub command: u8,
    pub featurel: u8,
    pub lba0: u8,
    pub lba1: u8,
    pub lba2: u8,
    pub device: u8,
    pub lba3: u8,
    pub lba4: u8,
    pub lba5: u8,
    pub featureh: u8,
    pub countl: u8,
    pub counth: u8,
    pub icc: u8,
    pub control: u8,
    pub reserved: [u8; 4],
}

/// Physical Region Descriptor Table entry
#[repr(C)]
pub struct PrdtEntry {
    pub dba: u32,
    pub dbau: u32,
    pub reserved: u32,
    pub dbc: u32,
}

/// Command table
#[repr(C, align(128))]
pub struct CommandTable {
    pub cfis: [u8; 64],
    pub acmd: [u8; 16],
    pub reserved: [u8; 48],
    pub prdt: [PrdtEntry; 1],
}
