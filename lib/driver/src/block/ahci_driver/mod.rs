use core::ptr;
use hal::block::{BlockDevice, BlockError};
use hal::dma::DmaAllocator;
use hal::io::IoError;
use crate::block::ahci_driver::constants::{HBA_GHC, HBA_PI, SECTOR_SIZE};
use crate::block::ahci_driver::port::PortState;

mod constants;
mod fis;
mod port;

pub struct AhciDriver {
    port: PortState,
    sector_count: u64,
}

impl AhciDriver {
    pub unsafe fn init(mmio_base: usize, dma: &mut impl DmaAllocator) -> Result<Self, BlockError> {
        unsafe {
            // Enable AHCI mode
            let ghc = (mmio_base + HBA_GHC) as *mut u32;
            ptr::write_volatile(ghc, ptr::read_volatile(ghc) | (1 << 31));

            // Find first implemented port
            let pi = ptr::read_volatile((mmio_base + HBA_PI) as *const u32);
            let port = (0..32).find(|&i| pi & (1 << i) != 0).ok_or(BlockError::DeviceError)?;

            let port_state = PortState::init(mmio_base, port, dma)?;

            Ok(Self {
                port: port_state,
                sector_count: 131_071, // TODO: issue IDENTIFY DEVICE
            })
        }
    }
}

impl BlockDevice for AhciDriver {
    fn read_at(&mut self, offset: u64, buf: &mut [u8]) -> Result<(), IoError> {
        if offset + buf.len() as u64 > self.size() {
            return Err(BlockError::OutOfBounds.into());
        }

        let mut remaining = buf;
        let mut current_offset = offset;

        while !remaining.is_empty() {
            let lba = current_offset / SECTOR_SIZE as u64;
            let sector_off = (current_offset % SECTOR_SIZE as u64) as usize;
            let can_read = (SECTOR_SIZE - sector_off).min(remaining.len());

            let mut sector_buf = [0u8; SECTOR_SIZE];
            unsafe {
                self.port.read_sectors(lba, &mut sector_buf)?;
            }

            remaining[..can_read].copy_from_slice(&sector_buf[sector_off..sector_off + can_read]);

            remaining = &mut remaining[can_read..];
            current_offset += can_read as u64;
        }

        Ok(())
    }

    fn write_at(&mut self, offset: u64, buf: &[u8]) -> Result<(), IoError> {
        if offset + buf.len() as u64 > self.size() {
            return Err(BlockError::OutOfBounds.into());
        }

        let mut remaining = buf;
        let mut current_offset = offset;

        while !remaining.is_empty() {
            let lba = current_offset / SECTOR_SIZE as u64;
            let sector_off = (current_offset % SECTOR_SIZE as u64) as usize;
            let can_write = (SECTOR_SIZE - sector_off).min(remaining.len());

            if sector_off == 0 && can_write == SECTOR_SIZE {
                unsafe {
                    self.port.write_sectors(lba, &remaining[..SECTOR_SIZE])?;
                }
                remaining = &remaining[SECTOR_SIZE..];
                current_offset += SECTOR_SIZE as u64;
            } else {
                // Read-modify-write for unaligned access
                let mut sector_buf = [0u8; SECTOR_SIZE];
                unsafe {
                    self.port.read_sectors(lba, &mut sector_buf)?;
                    sector_buf[sector_off..sector_off + can_write]
                        .copy_from_slice(&remaining[..can_write]);
                    self.port.write_sectors(lba, &sector_buf)?;
                }
                remaining = &remaining[can_write..];
                current_offset += can_write as u64;
            }
        }

        Ok(())
    }

    fn size(&self) -> u64 { self.sector_count * SECTOR_SIZE as u64 }
    fn sector_size(&self) -> usize { SECTOR_SIZE }
}
