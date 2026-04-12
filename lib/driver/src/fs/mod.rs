mod ntfs;

use alloc::{
    boxed::Box,
    format,
    string::{String, ToString},
    vec::Vec,
};
use core::result::Result;

use hal::{block::BlockDevice, dma::DmaAllocator, io::IoError};
use spin::{Mutex, Once};

pub use crate::{
    block::{ahci_driver::AhciDriver, virtio_driver::VirtioDriver},
    fs::ntfs::{CreateOptions, NtfsFile, NtfsStat, VolumeInfo},
};
use crate::{
    core::{
        partition::{
            Partition,
            gpt::{
                GPT_FIRST_USABLE_LBA,
                manager::{GptManager, PartitionInfo},
            },
        },
        pci,
    },
    fs::ntfs::NtfsDriver,
    util::debug,
};

pub type NtfsFs = NtfsDriver<BlockDeviceEnum>;

pub static FS: Once<Mutex<NtfsFs>> = Once::new();

pub fn fs_mutex() -> &'static Mutex<NtfsFs> { FS.get().expect("filesystem not initialized") }

pub enum BlockDeviceEnum {
    Virtio(Partition<VirtioDriver>),
    Ahci(Partition<AhciDriver>),
}

#[derive(Debug)]
pub enum DiskType {
    Virtio,
    Ahci,
}

#[derive(Debug)]
pub struct DiskInfo {
    pub id: String,
    pub disk_type: DiskType,
    pub sector_count: u64,
}

pub struct DiskRegistry {
    pub disks: Vec<DiskHandle>,
}

pub struct DiskHandle {
    pub info: DiskInfo,
    pub device: BlockDeviceEnum,
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
        let is_virtio = dev.vendor_id == pci::VENDOR_VIRTIO
            && (dev.device_id == pci::DEVICE_VIRTIO_BLK
            || dev.device_id == pci::DEVICE_VIRTIO_BLK_M);

        if !is_virtio {
            continue;
        }

        debug("VirtIO disk found");

        let blk = unsafe { VirtioDriver::init(&dev, phys_mem_offset, dma)? };
        let size = blk.size();

        let mut device = BlockDeviceEnum::Virtio(Partition {
            inner: blk,
            start_offset: 0,
            size,
        });

        // IMPORTANT: create a temporary mutable borrow ONCE
        let tmp = match &mut device {
            BlockDeviceEnum::Virtio(p) => &mut p.inner,
            _ => unreachable!(),
        };

        let gpt = GptManager::read_disk(&mut *tmp);

        if let Ok(gpt) = gpt {
            for part in gpt.partitions {
                let offset = part.start_lba * gpt.sector_size as u64;

                let mut buf = [0u8; 512];
                let _ = device.read_at(offset, &mut buf);

                if &buf[3..11] == b"NTFS    " {
                    debug("NTFS found on VirtIO");
                    return mount(device, phys_mem_offset);
                }
            }
        }
    }

    Err("No VirtIO NTFS disk found".to_string())
}

pub fn detect_disks() -> Vec<DiskInfo> {
    let mut disks = Vec::new();

    for dev in pci::scan() {
        let is_virtio = dev.vendor_id == pci::VENDOR_VIRTIO
            && (dev.device_id == pci::DEVICE_VIRTIO_BLK
            || dev.device_id == pci::DEVICE_VIRTIO_BLK_M);

        if is_virtio {
            disks.push(DiskInfo {
                id: dev.location(),
                disk_type: DiskType::Virtio,
                sector_count: 0,
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

            let mut blk = unsafe { VirtioDriver::init(&dev, phys_mem_offset, dma) }
                .map_err(|_| "VirtIO init failed")?;
            let size = blk.size();
            let part_offset =
                GptManager::find_ntfs_offset(&mut blk).unwrap_or(GPT_FIRST_USABLE_LBA * 512);
            mount(
                BlockDeviceEnum::Virtio(Partition { inner: blk, start_offset: part_offset, size }),
                phys_mem_offset,
            )
        }
        DiskType::Ahci => {
            let dev = pci::scan()
                .into_iter()
                .find(|d| format!("{:02x}:{:02x}.{}", d.bus, d.device, d.func) == disk.id)
                .ok_or("AHCI device not found")?;

            let bar_addr = dev.bar5_phys.ok_or("AHCI no BAR5")?;
            let virt = bar_addr + phys_mem_offset;

            let mut blk =
                unsafe { AhciDriver::init(virt as usize, dma) }.map_err(|_| "AHCI init failed")?;
            let size = blk.size();
            let part_offset =
                GptManager::find_ntfs_offset(&mut blk).unwrap_or(GPT_FIRST_USABLE_LBA * 512);
            mount(
                BlockDeviceEnum::Ahci(Partition { inner: blk, start_offset: part_offset, size }),
                phys_mem_offset,
            )
        }
    }
}

fn mount(device: BlockDeviceEnum, offset: u64) -> Result<(), String> {
    let driver = NtfsDriver::mount(device, offset)
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

            let size = driver.size();
            Ok(BlockDeviceEnum::Virtio(Partition { inner: driver, start_offset: 0, size }))
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

            let size = driver.size();
            Ok(BlockDeviceEnum::Ahci(Partition { inner: driver, start_offset: 0, size }))
        }
    }
}

