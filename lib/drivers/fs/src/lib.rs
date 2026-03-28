#![no_std]

use core::result::Result;

pub use ahci_driver::AhciDriver;
use driver_core::{partition::Partition, pci};
use hal::{block::BlockDevice, dma::DmaAllocator, io::IoError};
use ntfs_driver::NtfsDriver;
pub use ntfs_driver::*;
use spin::{Mutex, Once};
pub use virtio_driver::VirtioBlkDevice;
pub type NtfsFs = NtfsDriver<BlockDeviceEnum>;

const PARTITION_OFFSET: u64 = 512;
const PARTITION_SIZE: u64 = 63 * 1024 * 1024;

pub static FS: Once<Mutex<NtfsFs>> = Once::new();

pub fn fs_mutex() -> &'static Mutex<NtfsFs> { FS.get().expect("filesystem not initialized") }

pub enum BlockDeviceEnum {
    Virtio(Partition<VirtioBlkDevice>),
    Ahci(Partition<AhciDriver>),
}

impl BlockDevice for BlockDeviceEnum {
    fn read_at(&mut self, offset: u64, buf: &mut [u8]) -> Result<(), IoError> {
        match self {
            Self::Virtio(d) => d.read_at(offset, buf),
            Self::Ahci(d) => d.read_at(offset, buf),
        }
    }
    fn write_at(&mut self, offset: u64, buf: &[u8]) -> Result<(), IoError> {
        match self {
            Self::Virtio(d) => d.write_at(offset, buf),
            Self::Ahci(d) => d.write_at(offset, buf),
        }
    }
    fn size(&self) -> u64 {
        match self {
            Self::Virtio(d) => d.size(),
            Self::Ahci(d) => d.size(),
        }
    }
    fn sector_size(&self) -> usize {
        match self {
            Self::Virtio(d) => d.sector_size(),
            Self::Ahci(d) => d.sector_size(),
        }
    }
}

pub fn init_auto(phys_mem_offset: u64, dma: &mut impl DmaAllocator) -> Result<(), &'static str> {
    if FS.is_completed() {
        return Ok(());
    }

    for dev in pci::scan() {
        let is_virtio = dev.vendor_id == pci::VENDOR_VIRTIO
            && (dev.device_id == pci::DEVICE_VIRTIO_BLK || dev.device_id == 0x1042);

        if is_virtio {
            if let Some(phys) = dev.bar0_phys {
                let virt = phys + phys_mem_offset;
                if let Ok(blk) = unsafe { VirtioBlkDevice::new(virt as usize, dma) } {
                    return mount(BlockDeviceEnum::Virtio(Partition {
                        inner: blk,
                        start_offset: PARTITION_OFFSET,
                        size: PARTITION_SIZE,
                    }));
                }
            }
        }

        let is_ahci = dev.class == pci::CLASS_STORAGE && dev.subclass == pci::SUBCLASS_AHCI;

        if is_ahci {
            if let Some(phys) = dev.bar0_phys {
                let virt = phys + phys_mem_offset;
                if let Ok(blk) = unsafe { AhciDriver::init(virt as usize, dma) } {
                    return mount(BlockDeviceEnum::Ahci(Partition {
                        inner: blk,
                        start_offset: PARTITION_OFFSET,
                        size: PARTITION_SIZE,
                    }));
                }
            }
        }
    }

    Err("no block device found")
}

fn mount(device: BlockDeviceEnum) -> Result<(), &'static str> {
    let driver = NtfsFs::mount(device).map_err(|_| "NtfsFs::mount failed")?;
    FS.call_once(|| Mutex::new(driver));
    Ok(())
}

pub fn is_initialized() -> bool { FS.is_completed() }
