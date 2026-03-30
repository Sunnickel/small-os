#![no_std]
extern crate alloc;

use alloc::{
    format,
    string::{String, ToString},
};
use core::result::Result;

pub use ahci_driver::AhciDriver;
use driver_core::{partition::Partition, pci};
use hal::{block::BlockDevice, dma::DmaAllocator, io::IoError};
use ntfs_driver::NtfsDriver;
pub use ntfs_driver::*;
use spin::{Mutex, Once};
pub use virtio_driver::VirtioBlkDevice;
pub type NtfsFs = NtfsDriver<BlockDeviceEnum>;

const PARTITION_OFFSET: u64 = 34 * 512;
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

pub fn init_auto(phys_mem_offset: u64, dma: &mut impl DmaAllocator) -> Result<(), String> {
    if FS.is_completed() {
        return Ok(());
    }

    for dev in pci::scan() {
        driver_core::debug(&format!(
            "Checking device {:02x}:{:02x}.{} vid={:#x} did={:#x}",
            dev.bus, dev.device, dev.func, dev.vendor_id, dev.device_id
        ));

        let is_virtio = dev.vendor_id == pci::VENDOR_VIRTIO
            && (dev.device_id == pci::DEVICE_VIRTIO_BLK
                || dev.device_id == pci::DEVICE_VIRTIO_BLK_M);
        let is_ahci = dev.class == pci::CLASS_STORAGE && dev.subclass == pci::SUBCLASS_AHCI;

        if is_virtio {
            driver_core::debug("Found VirtIO block device!");

            // Use new PCI-based initialization
            let mut blk = unsafe { VirtioBlkDevice::new(&dev, phys_mem_offset, dma) }
                .map_err(|e| format!("VirtIO init failed: {}", e))?;

            driver_core::debug("VirtIO initialized");

            let part_offset = driver_core::partition::gpt::first_ntfs_partition_offset(&mut blk)
                .unwrap_or(PARTITION_OFFSET);

            driver_core::debug(&format!("VirtIO part_offset={:#x}", part_offset));

            let mut boot = [0u8; 512];
            blk.read_at(part_offset, &mut boot)
                .map_err(|_| "VirtIO: failed to read boot sector")?;

            let oem = &boot[3..11];
            driver_core::debug(&format!(
                "VirtIO OEM: {:02x?} ({:?})",
                oem,
                core::str::from_utf8(oem).unwrap_or("???")
            ));

            if oem != b"NTFS    " {
                driver_core::debug("VirtIO: not NTFS → skipping");
                continue;
            }

            driver_core::debug("VirtIO: NTFS detected ✅ mounting...");

            // Get actual device size
            let size = blk.size();

            return mount(BlockDeviceEnum::Virtio(Partition {
                inner: blk,
                start_offset: part_offset,
                size: size.saturating_sub(part_offset),
            }));
        }

        if is_ahci {
            driver_core::debug("Found AHCI device!");

            let bar_addr = dev.bar5_phys.ok_or_else(|| {
                driver_core::debug("AHCI: no BAR5, skipping");
                "no BAR5"
            })?;

            let virt = bar_addr + phys_mem_offset;
            driver_core::debug(&format!("AHCI at phys={:#x} virt={:#x}", bar_addr, virt));

            let mut blk = unsafe { AhciDriver::init(virt as usize, dma) }
                .map_err(|e| format!("AHCI init failed: {:?}", e))?;

            driver_core::debug("AHCI initialized");

            let part_offset = driver_core::partition::gpt::first_ntfs_partition_offset(&mut blk)
                .unwrap_or(PARTITION_OFFSET);

            driver_core::debug(&format!("AHCI part_offset={:#x}", part_offset));

            let mut boot = [0u8; 512];
            blk.read_at(part_offset, &mut boot).map_err(|_| "AHCI: failed to read boot sector")?;

            let oem = &boot[3..11];
            driver_core::debug(&format!(
                "AHCI OEM: {:02x?} ({:?})",
                oem,
                core::str::from_utf8(oem).unwrap_or("???")
            ));

            driver_core::debug(&format!(
                "boot[0x1FE..0x200]={:02x?} (want 55 aa)",
                &boot[0x1FE..0x200]
            ));

            if oem != b"NTFS    " {
                driver_core::debug("AHCI: not NTFS → skipping (likely boot disk)");
                continue;
            }

            driver_core::debug("AHCI: NTFS detected ✅ mounting...");

            return mount(BlockDeviceEnum::Ahci(Partition {
                inner: blk,
                start_offset: part_offset,
                size: PARTITION_SIZE,
            }));
        }
    }

    Err("no block device found".to_string())
}

fn mount(device: BlockDeviceEnum) -> Result<(), String> {
    let driver = NtfsDriver::mount(device, 0)
        .map_err(|e| format!("NtfsFs::mount failed, [fs] mount error: {:?}", e));
    if driver.is_ok() {
        FS.call_once(|| Mutex::new(driver.unwrap()));
        Ok(())
    } else {
        Err(driver.err().unwrap())
    }
}

pub fn is_initialized() -> bool { FS.is_completed() }

fn detect_fs(oem: &[u8]) -> &'static str {
    match oem {
        b"NTFS    " => "NTFS",
        b"MSDOS5.0" => "FAT12/16",
        b"MSWIN4.1" => "FAT32",
        _ => "UNKNOWN",
    }
}
