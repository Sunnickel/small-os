#![no_std]
extern crate alloc;

use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use spin::{Mutex, Once};

use ahci_driver::AhciDriver;
use driver_core::dma_allocator::DmaAllocator;
use driver_core::partition::Partition;
use driver_core::pci;
use virtio_driver::VirtioBlkDevice;

pub use ntfs_driver::*;

pub enum BlockDeviceEnum {
    Virtio(Partition<VirtioBlkDevice>),
    Ahci(Partition<AhciDriver>),
}

impl BlockDevice for BlockDeviceEnum {
    fn read_at(&mut self, offset: u64, buf: &mut [u8]) -> Result<(), BlockError> {
        match self {
            BlockDeviceEnum::Virtio(d) => d.read_at(offset, buf),
            BlockDeviceEnum::Ahci(d) => d.read_at(offset, buf),
        }
    }

    fn write_at(&mut self, offset: u64, buf: &[u8]) -> Result<(), BlockError> {
        match self {
            BlockDeviceEnum::Virtio(d) => d.write_at(offset, buf),
            BlockDeviceEnum::Ahci(d) => d.write_at(offset, buf),
        }
    }

    fn size(&self) -> u64 {
        match self {
            BlockDeviceEnum::Virtio(d) => d.size(),
            BlockDeviceEnum::Ahci(d) => d.size(),
        }
    }
    fn sector_size(&self) -> usize {
        match self {
            BlockDeviceEnum::Virtio(d) => d.sector_size(),
            BlockDeviceEnum::Ahci(d) => d.sector_size(),
        }
    }
}

pub type NtfsFs = NtfsDriver<BlockDeviceEnum>;

static FS: Once<Mutex<NtfsFs>> = Once::new();

fn fs_mutex() -> &'static Mutex<NtfsFs> {
    FS.get().expect("filesystem not initialized")
}

/// Try to auto-detect and initialize whichever block device is available.
/// Probes VirtIO first, then AHCI. Returns an error if nothing is found.
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

                let blk = unsafe { VirtioBlkDevice::new(virt as usize, dma) }
                    .map_err(|_| "VirtioBlkDevice::new failed")?;

                let partition = Partition {
                    inner: blk,
                    start_offset: 512,
                    size: 63 * 1024 * 1024,
                };

                let driver = NtfsFs::mount(BlockDeviceEnum::Virtio(partition))
                    .map_err(|_| "NtfsFs::mount failed")?;

                FS.call_once(|| Mutex::new(driver));
                return Ok(());
            }
        }

        let is_ahci = dev.class == pci::CLASS_STORAGE && dev.subclass == pci::SUBCLASS_AHCI;

        if is_ahci {
            if let Some(phys) = dev.bar0_phys {
                let virt = phys + phys_mem_offset;
                if let Ok(blk) = unsafe { AhciDriver::init(virt as usize, dma) } {
                    let partition = Partition {
                        inner: blk,
                        start_offset: 512,
                        size: 63 * 1024 * 1024,
                    };

                    let driver = NtfsFs::mount(BlockDeviceEnum::Ahci(partition))
                        .map_err(|_| "NtfsFs::mount failed")?;

                    FS.call_once(|| Mutex::new(driver));
                    return Ok(());
                }
            }
        }
    }

    Err("no block device found")
}

pub fn is_initialized() -> bool {
    FS.is_completed()
}

// Convenience wrappers - lock for the duration of the call
pub fn root_directory() -> Result<NtfsFile, FsError> {
    fs_mutex().lock().root_directory().map_err(|e| e.into())
}

pub fn open(path: &str) -> Result<NtfsFile, NtfsError> {
    if path == "/" {
        return fs_mutex().lock().root_directory();
    }

    fs_mutex().lock().open(path)
}

pub fn read_file(file: &NtfsFile, buf: &mut [u8]) -> Result<usize, FsError> {
    fs_mutex().lock().read_file(file, buf).map_err(|e| e.into())
}

pub fn read_file_all(file: &NtfsFile) -> Result<Vec<u8>, FsError> {
    fs_mutex().lock().read_file_all(file).map_err(|e| e.into())
}

pub fn list_directory(dir: &NtfsFile) -> Result<Vec<String>, FsError> {
    fs_mutex().lock().list_directory(dir).map_err(|e| e.into())
}

// ===== WRITE OPERATIONS =====

/// Create a new file or directory
pub fn create_file(parent: &NtfsFile, name: &str, options: CreateOptions) -> Result<NtfsFile, FsError> {
    fs_mutex().lock().create_file(parent, name, options).map_err(|e| e.into())
}

/// Write to an existing file (overwrite only, resident files only)
pub fn write_file(file: &mut NtfsFile, data: &[u8]) -> Result<(), FsError> {
    fs_mutex().lock().write_file(file, data).map_err(|e| e.into())
}

/// Find file in directory by name (helper for create)
pub fn find_in_directory(dir: &NtfsFile, name: &str) -> Result<u64, FsError> {
    fs_mutex().lock().find_in_directory(dir, name).map_err(|e| e.into())
}