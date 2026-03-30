pub const SECTOR_SIZE: usize = 512;

// HBA memory layout
pub const HBA_GHC: usize = 0x04;
pub const HBA_PI: usize = 0x0C;
pub const HBA_PORTS: usize = 0x100;
pub const PORT_SIZE: usize = 0x80;

// Port registers
pub const PORT_CLB: usize = 0x00;
pub const PORT_CLBU: usize = 0x04;
pub const PORT_FB: usize = 0x08;
pub const PORT_FBU: usize = 0x0C;
pub const PORT_IS: usize = 0x10;
pub const PORT_IE: usize = 0x14;
pub const PORT_CMD: usize = 0x18;
pub const PORT_TFD: usize = 0x20;
pub const PORT_SIG: usize = 0x24;
pub const PORT_SSTS: usize = 0x28;
pub const PORT_SCTL: usize = 0x2C;
pub const PORT_SERR: usize = 0x30;
pub const PORT_SACT: usize = 0x34;
pub const PORT_CI: usize = 0x38;
pub const PORT_SNTF: usize = 0x3C;

// ATA commands
pub const ATA_CMD_READ_SECTORS: u8 = 0x20;
pub const ATA_CMD_WRITE_SECTORS: u8 = 0x30;
