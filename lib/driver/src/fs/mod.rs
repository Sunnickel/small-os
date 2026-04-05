mod ntfs;

use alloc::{
    format,
    string::{String, ToString},
    vec::Vec,
};
use core::result::Result;

use hal::{block::BlockDevice, dma::DmaAllocator, io::IoError};
use spin::{Mutex, Once};

pub use crate::block::{ahci_driver::AhciDriver, virtio_driver::VirtioDriver};
use crate::{
    core::{
        partition::{
            Partition,
            gpt::manager::{GptManager, PartitionInfo},
        },
        pci,
    },
    fs::ntfs::NtfsDriver,
    util::debug,
};

pub type NtfsFs = NtfsDriver<BlockDeviceEnum>;

const PARTITION_OFFSET: u64 = 34 * 512; // Fallback raw offset
const PARTITION_SIZE: u64 = 63 * 1024 * 1024;

pub static FS: Once<Mutex<NtfsFs>> = Once::new();

pub fn fs_mutex() -> &'static Mutex<NtfsFs> { FS.get().expect("filesystem not initialized") }

pub enum BlockDeviceEnum {
    Virtio(Partition<VirtioDriver>),
    Ahci(Partition<AhciDriver>),
}

pub enum DiskType {
    Virtio,
    Ahci,
}

pub struct DiskInfo {
    pub id: String,
    pub disk_type: DiskType,
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
    fn block_count(&self) -> u64 {
        let sector_size = self.sector_size() as u64;
        if sector_size == 0 { 0 } else { self.size() / sector_size }
    }
}

pub fn init_auto(phys_mem_offset: u64, dma: &mut impl DmaAllocator) -> Result<(), String> {
    if FS.is_completed() {
        return Ok(());
    }

    for dev in pci::scan() {
        debug(&format!(
            "Checking device {:02x}:{:02x}.{} vid={:#x} did={:#x}",
            dev.bus, dev.device, dev.func, dev.vendor_id, dev.device_id
        ));

        let is_virtio = dev.vendor_id == pci::VENDOR_VIRTIO
            && (dev.device_id == pci::DEVICE_VIRTIO_BLK
                || dev.device_id == pci::DEVICE_VIRTIO_BLK_M);
        let is_ahci = dev.class == pci::CLASS_STORAGE && dev.subclass == pci::SUBCLASS_AHCI;

        if is_virtio {
            debug("Found VirtIO block device!");

            let mut blk = unsafe { VirtioDriver::init(&dev, phys_mem_offset, dma) }
                .map_err(|e| format!("VirtIO init failed: {}", e))?;

            debug("VirtIO initialized");

            // Use GPT manager to find NTFS partition
            let part_offset = match GptManager::find_ntfs_offset(&mut blk) {
                Ok(offset) => {
                    debug(&format!("GPT: Found NTFS partition at offset={:#x}", offset));
                    offset
                }
                Err(e) => {
                    debug(&format!("GPT: No NTFS partition found ({:?}), using raw offset", e));
                    PARTITION_OFFSET
                }
            };

            debug(&format!("VirtIO part_offset={:#x}", part_offset));

            let mut boot = [0u8; 512];
            blk.read_at(part_offset, &mut boot)
                .map_err(|_| "VirtIO: failed to read boot sector")?;

            let oem = &boot[3..11];
            debug(&format!(
                "VirtIO OEM: {:02x?} ({:?})",
                oem,
                core::str::from_utf8(oem).unwrap_or("???")
            ));

            if oem != b"NTFS    " {
                debug("VirtIO: not NTFS → skipping");
                continue;
            }

            debug("VirtIO: NTFS detected ✅ mounting...");

            let size = blk.size();

            return mount(BlockDeviceEnum::Virtio(Partition {
                inner: blk,
                start_offset: part_offset,
                size: size.saturating_sub(part_offset),
            }));
        }

        if is_ahci {
            debug("Found AHCI device!");

            let bar_addr = dev.bar5_phys.ok_or_else(|| {
                debug("AHCI: no BAR5, skipping");
                "no BAR5"
            })?;

            let virt = bar_addr + phys_mem_offset;
            debug(&format!("AHCI at phys={:#x} virt={:#x}", bar_addr, virt));

            let mut blk = unsafe { AhciDriver::init(virt as usize, dma) }
                .map_err(|e| format!("AHCI init failed: {:?}", e))?;

            debug("AHCI initialized");

            // Use GPT manager to find NTFS partition
            let part_offset = match GptManager::find_ntfs_offset(&mut blk) {
                Ok(offset) => {
                    debug(&format!("GPT: Found NTFS partition at offset={:#x}", offset));
                    offset
                }
                Err(e) => {
                    debug(&format!("GPT: No NTFS partition found ({:?}), using raw offset", e));
                    PARTITION_OFFSET
                }
            };

            debug(&format!("AHCI part_offset={:#x}", part_offset));

            let mut boot = [0u8; 512];
            blk.read_at(part_offset, &mut boot).map_err(|_| "AHCI: failed to read boot sector")?;

            let oem = &boot[3..11];
            debug(&format!(
                "AHCI OEM: {:02x?} ({:?})",
                oem,
                core::str::from_utf8(oem).unwrap_or("???")
            ));

            debug(&format!("boot[0x1FE..0x200]={:02x?} (want 55 aa)", &boot[0x1FE..0x200]));

            if oem != b"NTFS    " {
                debug("AHCI: not NTFS → skipping (likely boot disk)");
                continue;
            }

            debug("AHCI: NTFS detected ✅ mounting...");

            return mount(BlockDeviceEnum::Ahci(Partition {
                inner: blk,
                start_offset: part_offset,
                size: PARTITION_SIZE,
            }));
        }
    }

    Err("no block device found".to_string())
}

pub fn detect_disks() -> Vec<DiskInfo> {
    let mut disks = Vec::new();

    for dev in pci::scan() {
        let is_virtio = dev.vendor_id == pci::VENDOR_VIRTIO
            && (dev.device_id == pci::DEVICE_VIRTIO_BLK
                || dev.device_id == pci::DEVICE_VIRTIO_BLK_M);

        let is_ahci = dev.class == pci::CLASS_STORAGE && dev.subclass == pci::SUBCLASS_AHCI;

        if is_virtio {
            disks.push(DiskInfo {
                id: format!("{:02x}:{:02x}.{}", dev.bus, dev.device, dev.func),
                disk_type: DiskType::Virtio,
            });
        } else if is_ahci {
            disks.push(DiskInfo {
                id: format!("{:02x}:{:02x}.{}", dev.bus, dev.device, dev.func),
                disk_type: DiskType::Ahci,
            });
        }
    }

    disks
}

/// Format disk with GPT and create single NTFS partition
pub fn format_disk(
    disk: &DiskInfo,
    phys_mem_offset: u64,
    dma: &mut impl DmaAllocator,
) -> Result<PartitionInfo, String> {
    let mut blk = init_block_device(disk, phys_mem_offset, dma)?;
    let sector_size = blk.sector_size() as u64;

    // 1. Create GPT partition
    let (start_lba, size_lba) = GptManager::format_disk(&mut blk, 0)
        .map_err(|e| format!("Failed to create GPT: {:?}", e))?;

    let start_offset = start_lba * sector_size;
    let size_bytes = size_lba * sector_size;

    // 2. Write NTFS boot sector at partition start
    let boot_sector = GptManager::create_ntfs_boot_sector(size_lba, blk.sector_size()).unwrap();
    blk.write_at(start_offset, &boot_sector)
        .map_err(|e| format!("Failed to write boot sector: {:?}", e))?;

    // 3. Create minimal MFT record for root directory
    // Use your existing NtfsDriver::create_file logic or write.rs functions

    debug(&format!(
        "Formatted disk: NTFS partition at LBA {}-{} ({} MB)",
        start_lba,
        start_lba + size_lba - 1,
        size_bytes / 1024 / 1024
    ));

    Ok(PartitionInfo {
        type_guid: "EBD0A0A2-B9E5-4433-87C0-68B6B72699C7".to_string(),
        unique_guid: "".to_string(),
        start_lba,
        end_lba: start_lba + size_lba - 1,
        size_bytes,
        attributes: 0,
        name: "NTFS".to_string(),
    })
}

/// Get disk information using GPT manager
pub fn get_disk_info(
    disk: &DiskInfo,
    phys_mem_offset: u64,
    dma: &mut impl DmaAllocator,
) -> Result<String, String> {
    let mut blk = init_block_device(disk, phys_mem_offset, dma)?;

    let info =
        GptManager::read_disk(&mut blk).map_err(|e| format!("Failed to read GPT: {:?}", e))?;

    let mut output = format!(
        "Disk GUID: {}\nSector size: {} bytes\nTotal sectors: {}\nFirst usable LBA: {}\nLast usable LBA: {}\nPartitions:\n",
        info.disk_guid,
        info.sector_size,
        info.total_sectors,
        info.first_usable_lba,
        info.last_usable_lba
    );

    if info.partitions.is_empty() {
        output.push_str("  (none)\n");
    } else {
        for (i, part) in info.partitions.iter().enumerate() {
            let size_mb = part.size_bytes / 1024 / 1024;
            output.push_str(&format!(
                "  {}: {} ({} MB, LBA {}-{})\n",
                i, part.name, size_mb, part.start_lba, part.end_lba
            ));
        }
    }

    Ok(output)
}

pub fn init_driver(
    disk: &DiskInfo,
    phys_mem_offset: u64,
    dma: &mut impl DmaAllocator,
) -> Result<(), String> {
    match disk.disk_type {
        DiskType::Virtio => {
            let dev = pci::scan()
                .into_iter()
                .find(|d| format!("{:02x}:{:02x}.{}", d.bus, d.device, d.func) == disk.id)
                .ok_or("VirtIO device not found")?;

            let blk = unsafe { VirtioDriver::init(&dev, phys_mem_offset, dma) }
                .map_err(|_| "VirtIO init failed")?;
            let size = blk.size();

            mount(BlockDeviceEnum::Virtio(Partition { inner: blk, start_offset: 0, size }))
        }
        DiskType::Ahci => {
            let dev = pci::scan()
                .into_iter()
                .find(|d| format!("{:02x}:{:02x}.{}", d.bus, d.device, d.func) == disk.id)
                .ok_or("AHCI device not found")?;

            let bar_addr = dev.bar5_phys.ok_or("AHCI no BAR5")?;
            let virt = bar_addr + phys_mem_offset;

            let blk =
                unsafe { AhciDriver::init(virt as usize, dma) }.map_err(|_| "AHCI init failed")?;
            let size = blk.size();
            mount(BlockDeviceEnum::Ahci(Partition { inner: blk, start_offset: 0, size }))
        }
    }
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

fn init_block_device(
    disk: &DiskInfo,
    phys_mem_offset: u64,
    dma: &mut impl DmaAllocator,
) -> Result<BlockDeviceEnum, String> {
    match disk.disk_type {
        DiskType::Virtio => {
            let dev = pci::scan()
                .into_iter()
                .find(|d| format!("{:02x}:{:02x}.{}", d.bus, d.device, d.func) == disk.id)
                .ok_or("VirtIO device not found")?;

            let driver = unsafe { VirtioDriver::init(&dev, phys_mem_offset, dma) }
                .map_err(|e| format!("VirtIO init failed: {}", e))?;

            Ok(BlockDeviceEnum::Virtio(Partition { inner: driver, start_offset: 0, size: 0 }))
        }
        DiskType::Ahci => {
            let dev = pci::scan()
                .into_iter()
                .find(|d| format!("{:02x}:{:02x}.{}", d.bus, d.device, d.func) == disk.id)
                .ok_or("AHCI device not found")?;

            let bar_addr = dev.bar5_phys.ok_or("AHCI no BAR5")?;
            let virt = bar_addr + phys_mem_offset;

            let driver = unsafe { AhciDriver::init(virt as usize, dma) }
                .map_err(|e| format!("AHCI init failed: {:?}", e))?;

            Ok(BlockDeviceEnum::Ahci(Partition { inner: driver, start_offset: 0, size: 0 }))
        }
    }
}

fn detect_fs(oem: &[u8]) -> &'static str {
    match oem {
        b"NTFS    " => "NTFS",
        b"MSDOS5.0" => "FAT12/16",
        b"MSWIN4.1" => "FAT32",
        _ => "UNKNOWN",
    }
}
