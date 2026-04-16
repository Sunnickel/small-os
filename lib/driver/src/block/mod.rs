use alloc::boxed::Box;

use hal::block::{BlockDevice, BlockError};

use crate::block::{
    ahci::port::PortState,
    virtio::VirtioBlkState,
};

pub mod ahci;
pub mod virtio;

pub enum BlockDeviceEnum {
    Virtio(Box<VirtioBlkState>),
    Ahci(Box<AhciPortWrapper>),
}

pub struct AhciPortWrapper {
    port_idx: u8,
    state: PortState,
    sector_count: u64,
}

impl BlockDeviceEnum {
    /// Create from VirtIO driver
    pub fn from_virtio(state: VirtioBlkState) -> Self { Self::Virtio(Box::new(state)) }

    /// Create from AHCI port (extract from AhciState)
    pub fn from_ahci_port(port_idx: u8, state: PortState, sectors: u64) -> Self {
        Self::Ahci(Box::new(AhciPortWrapper { port_idx, state, sector_count: sectors }))
    }

    pub fn sector_count(&self) -> u64 {
        match self {
            Self::Virtio(v) => v.sector_count,
            Self::Ahci(a) => a.sector_count,
        }
    }
}

impl BlockDevice for BlockDeviceEnum {
    fn read_blocks(&mut self, lba: u64, buf: &mut [u8]) -> Result<(), BlockError> {
        match self {
            Self::Virtio(v) => v.read_sectors(lba, buf),
            Self::Ahci(a) => a.read_blocks(lba, buf),
        }
    }

    fn write_blocks(&mut self, lba: u64, buf: &[u8]) -> Result<(), BlockError> {
        match self {
            Self::Virtio(v) => v.write_sectors(lba, buf),
            Self::Ahci(a) => a.write_blocks(lba, buf),
        }
    }

    fn block_size(&self) -> usize { 512 }

    fn block_count(&self) -> u64 { self.sector_count() }
}

impl AhciPortWrapper {
    fn read_blocks(&mut self, lba: u64, buf: &mut [u8]) -> Result<(), BlockError> {
        unsafe { self.state.read_sectors(lba, buf).map_err(|_| BlockError::ReadError) }
    }

    fn write_blocks(&mut self, lba: u64, buf: &[u8]) -> Result<(), BlockError> {
        unsafe { self.state.write_sectors(lba, buf).map_err(|_| BlockError::WriteError) }
    }
}
