pub(super) const SECTOR_SIZE: usize = 512;

// HBA memory layout
pub(super) const HBA_GHC: usize = 0x04;
pub(super) const HBA_PI: usize = 0x0C;
pub(super) const HBA_PORTS: usize = 0x100;
pub(super) const PORT_SIZE: usize = 0x80;

// Port registers
pub(super) const PORT_CLB: usize = 0x00;
pub(super) const PORT_CLBU: usize = 0x04;
pub(super) const PORT_FB: usize = 0x08;
pub(super) const PORT_FBU: usize = 0x0C;
pub(super) const PORT_IS: usize = 0x10;
pub(super) const PORT_IE: usize = 0x14;
pub(super) const PORT_CMD: usize = 0x18;
pub(super) const PORT_TFD: usize = 0x20;
pub(super) const PORT_SIG: usize = 0x24;
pub(super) const PORT_SSTS: usize = 0x28;
pub(super) const PORT_SCTL: usize = 0x2C;
pub(super) const PORT_SERR: usize = 0x30;
pub(super) const PORT_SACT: usize = 0x34;
pub(super) const PORT_CI: usize = 0x38;
pub(super) const PORT_SNTF: usize = 0x3C;

// ATA commands
pub(super) const ATA_CMD_READ_SECTORS: u8 = 0x20;
pub(super) const ATA_CMD_WRITE_SECTORS: u8 = 0x30;
