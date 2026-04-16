use alloc::{format, string::String, vec, vec::Vec};

use hal::block::BlockDevice;
use crate::partition::gpt::error::GptError;
use crate::partition::gpt::{generate_guid, GPT_FIRST_USABLE_LBA, GPT_RESERVED_LBAS};
use crate::partition::gpt::read::read_gpt;
use crate::partition::gpt::structs::GptEntry;
use crate::partition::gpt::write::write_gpt;

/// NTFS partition type GUID
const NTFS_GUID: [u8; 16] = [
    0xEB, 0xD0, 0xA0, 0xA2, 0xB9, 0xE5, 0x44, 0x33, 0x87, 0xC0, 0x68, 0xB6, 0xB7, 0x26, 0x99, 0xC7,
];

pub(crate) struct GptManager;

impl GptManager {
    /// Format disk with fresh GPT and single NTFS partition
    pub(crate) fn format_disk(
        dev: &mut impl BlockDevice,
        partition_size_sectors: u64,
    ) -> Result<(u64, u64), GptError> {
        let total_sectors = dev.block_count();

        if total_sectors < 64 {
            return Err(GptError::NoSpace);
        }

        let first_usable = GPT_FIRST_USABLE_LBA;
        let last_usable = total_sectors.saturating_sub(GPT_RESERVED_LBAS + 1);

        let partition_size = if partition_size_sectors == 0
            || partition_size_sectors > (last_usable - first_usable)
        {
            last_usable - first_usable
        } else {
            partition_size_sectors
        };

        let partition_end = first_usable + partition_size - 1;

        let entries = vec![GptEntry {
            type_guid: NTFS_GUID,
            unique_guid: generate_guid(),
            start_lba: first_usable,
            end_lba: partition_end,
            attributes: 0,
            name: str_to_utf16le("NTFS"),
        }];

        write_gpt(dev, &entries)?;

        Ok((first_usable, partition_size))
    }

    /// Read disk GPT information
    pub(crate) fn read_disk(dev: &mut dyn BlockDevice) -> Result<GptInfo, GptError> {
        let (header, entries) = read_gpt(dev)?;

        let partitions: Vec<PartitionInfo> = entries
            .into_iter()
            .filter(|e| !is_zero_guid(&e.type_guid))
            .map(|e| PartitionInfo {
                type_guid: format_guid(&e.type_guid),
                unique_guid: format_guid(&e.unique_guid),
                start_lba: e.start_lba,
                end_lba: e.end_lba,
                size_bytes: (e.end_lba - e.start_lba + 1) * dev.sector_size() as u64,
                attributes: e.attributes,
                name: utf16le_to_str(&e.name),
            })
            .collect();

        Ok(GptInfo {
            disk_guid: format_guid(&header.disk_guid),
            sector_size: dev.sector_size(),
            total_sectors: dev.block_count(),
            first_usable_lba: header.first_usable_lba,
            last_usable_lba: header.last_usable_lba,
            partitions,
        })
    }

    /// Find first NTFS partition offset in bytes
    pub(crate) fn find_ntfs_offset(dev: &mut impl BlockDevice) -> Result<u64, GptError> {
        let (_, entries) = read_gpt(dev)?;

        for entry in entries {
            if entry.type_guid == NTFS_GUID {
                let sector_size = dev.sector_size() as u64;
                return Ok(entry.start_lba * sector_size);
            }
        }

        Err(GptError::NotFound)
    }

    pub(crate) fn create_ntfs_boot_sector(
        total_sectors: u64,
        sector_size: usize,
    ) -> Result<Vec<u8>, GptError> {
        let mut boot = vec![0u8; sector_size];

        // Jump instruction
        boot[0..3].copy_from_slice(&[0xEB, 0x52, 0x90]);

        // OEM ID "NTFS    "
        boot[3..11].copy_from_slice(b"NTFS    ");

        // Bytes per sector (usually 512)
        boot[11..13].copy_from_slice(&(sector_size as u16).to_le_bytes());

        // Sectors per cluster (e.g., 8 = 4KB clusters)
        boot[13] = 8;

        // Total sectors (using your total_sectors)
        boot[40..48].copy_from_slice(&total_sectors.to_le_bytes());

        // MFT start cluster (usually 2)
        boot[48..56].copy_from_slice(&2u64.to_le_bytes());

        // Serial number (random)
        let serial = 0x123456789ABCDEF0u64;
        boot[72..80].copy_from_slice(&serial.to_le_bytes());

        // Boot signature
        boot[510] = 0x55;
        boot[511] = 0xAA;

        Ok(boot)
    }
}

#[derive(Debug, Clone)]
pub(crate) struct GptInfo {
    pub disk_guid: String,
    pub sector_size: usize,
    pub total_sectors: u64,
    pub first_usable_lba: u64,
    pub last_usable_lba: u64,
    pub partitions: Vec<PartitionInfo>,
}

#[derive(Debug, Clone)]
pub struct PartitionInfo {
    pub type_guid: String,
    pub unique_guid: String,
    pub start_lba: u64,
    pub end_lba: u64,
    pub size_bytes: u64,
    pub attributes: u64,
    pub name: String,
}

// Helper functions
fn str_to_utf16le(s: &str) -> [u16; 36] {
    let mut name = [0u16; 36];
    for (i, c) in s.encode_utf16().take(36).enumerate() {
        name[i] = c;
    }
    name
}

fn utf16le_to_str(name: &[u16; 36]) -> String {
    let len = name.iter().position(|&c| c == 0).unwrap_or(36);
    let chars: Vec<u8> =
        name[..len].iter().filter_map(|&c| if c < 128 { Some(c as u8) } else { None }).collect();
    String::from_utf8_lossy(&chars).into_owned()
}

fn format_guid(guid: &[u8; 16]) -> String {
    format!(
        "{:08x}-{:04x}-{:04x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        u32::from_le_bytes([guid[0], guid[1], guid[2], guid[3]]),
        u16::from_le_bytes([guid[4], guid[5]]),
        u16::from_le_bytes([guid[6], guid[7]]),
        guid[8],
        guid[9],
        guid[10],
        guid[11],
        guid[12],
        guid[13],
        guid[14],
        guid[15]
    )
}

fn is_zero_guid(guid: &[u8; 16]) -> bool { guid.iter().all(|&b| b == 0) }
