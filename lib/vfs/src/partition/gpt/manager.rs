use alloc::{format, string::String, vec, vec::Vec};

use hal::block::BlockDevice;
use crate::partition::gpt::error::GptError;
use crate::partition::gpt::{generate_guid, GPT_FIRST_USABLE_LBA, GPT_RESERVED_LBAS};
use crate::partition::gpt::read::read_gpt;
use crate::partition::gpt::structs::GptEntry;
use crate::partition::gpt::write::write_gpt;

/// EFI System Partition GUID
const ESP_GUID: [u8; 16] = [
    0x28, 0x73, 0x2A, 0xC1, 0x1F, 0xF8, 0xD2, 0x11,
    0xBA, 0x4B, 0x00, 0xA0, 0xC9, 0x3E, 0xC9, 0x3B,
];

/// NTFS / Basic Data partition GUID
const NTFS_GUID: [u8; 16] = [
    0xA2, 0xA0, 0xD0, 0xEB, 0xE5, 0xB9, 0x33, 0x44,
    0x87, 0xC0, 0x68, 0xB6, 0xB7, 0x26, 0x99, 0xC7,
];

/// Size of the ESP in sectors (512 MB at 512 bytes/sector)
const ESP_SIZE_SECTORS: u64 = 1_048_576;

pub struct GptManager;

impl GptManager {
    /// Format disk with a fresh GPT containing:
    ///   - Partition 1: FAT32 EFI System Partition (512 MB)
    ///   - Partition 2: NTFS userspace (remainder)
    ///
    /// Returns (esp_start, esp_size, ntfs_start, ntfs_size) in sectors.
    pub fn format_disk(
        dev: &mut dyn BlockDevice,
        ntfs_size_sectors: u64,
    ) -> Result<(u64, u64, u64, u64), GptError> {
        let total_sectors = dev.block_count();

        // Need at least ESP + some NTFS space + GPT overhead
        if total_sectors < ESP_SIZE_SECTORS + 128 {
            return Err(GptError::NoSpace);
        }

        let first_usable = GPT_FIRST_USABLE_LBA;
        let last_usable  = total_sectors.saturating_sub(GPT_RESERVED_LBAS + 1);
        let usable_total = last_usable - first_usable;

        if usable_total < ESP_SIZE_SECTORS + 1 {
            return Err(GptError::NoSpace);
        }

        // ESP occupies the first ESP_SIZE_SECTORS of usable space
        let esp_start = first_usable;
        let esp_end   = esp_start + ESP_SIZE_SECTORS - 1;

        // NTFS gets either the requested size or the remainder
        let ntfs_start    = esp_end + 1;
        let available     = last_usable - ntfs_start;
        let ntfs_size     = if ntfs_size_sectors == 0 || ntfs_size_sectors > available {
            available
        } else {
            ntfs_size_sectors
        };
        let ntfs_end = ntfs_start + ntfs_size - 1;

        let entries = vec![
            GptEntry {
                type_guid:   ESP_GUID,
                unique_guid: generate_guid(),
                start_lba:   esp_start,
                end_lba:     esp_end,
                attributes:  0,
                name:        str_to_utf16le("EFI System"),
            },
            GptEntry {
                type_guid:   NTFS_GUID,
                unique_guid: generate_guid(),
                start_lba:   ntfs_start,
                end_lba:     ntfs_end,
                attributes:  0,
                name:        str_to_utf16le("Userspace"),
            },
        ];

        write_gpt(dev, &entries)?;

        Ok((esp_start, ESP_SIZE_SECTORS, ntfs_start, ntfs_size))
    }

    /// Read disk GPT information
    pub fn read_disk(dev: &mut dyn BlockDevice) -> Result<GptInfo, GptError> {
        let (header, entries) = read_gpt(dev)?;

        let sector_size = dev.block_size();

        let partitions: Vec<PartitionInfo> = entries
            .into_iter()
            .filter(|e| !is_zero_guid(&e.type_guid))
            .map(|e| {
                let size_bytes = (e.end_lba - e.start_lba + 1) * sector_size as u64;
                let kind = if e.type_guid == ESP_GUID {
                    PartitionKind::Esp
                } else if e.type_guid == NTFS_GUID {
                    PartitionKind::Ntfs
                } else {
                    PartitionKind::Unknown
                };
                PartitionInfo {
                    kind,
                    type_guid:   format_guid(&e.type_guid),
                    unique_guid: format_guid(&e.unique_guid),
                    start_lba:   e.start_lba,
                    end_lba:     e.end_lba,
                    size_bytes,
                    attributes:  e.attributes,
                    name:        utf16le_to_str(&e.name),
                }
            })
            .collect();

        Ok(GptInfo {
            disk_guid:       format_guid(&header.disk_guid),
            sector_size,
            total_sectors:   dev.block_count(),
            first_usable_lba: header.first_usable_lba,
            last_usable_lba:  header.last_usable_lba,
            partitions,
        })
    }

    /// Find the byte offset of the ESP partition
    pub fn find_esp_offset(dev: &mut impl BlockDevice) -> Result<u64, GptError> {
        let (_, entries) = read_gpt(dev)?;
        for entry in entries {
            if entry.type_guid == ESP_GUID {
                return Ok(entry.start_lba * dev.block_size() as u64);
            }
        }
        Err(GptError::NotFound)
    }

    /// Find the byte offset of the first NTFS partition
    pub fn find_ntfs_offset(dev: &mut impl BlockDevice) -> Result<u64, GptError> {
        let (_, entries) = read_gpt(dev)?;
        for entry in entries {
            if entry.type_guid == NTFS_GUID {
                return Ok(entry.start_lba * dev.block_size() as u64);
            }
        }
        Err(GptError::NotFound)
    }

    /// Build a minimal NTFS VBR for the NTFS partition
    pub fn create_ntfs_boot_sector(
        total_sectors: u64,
        sector_size: usize,
    ) -> Result<Vec<u8>, GptError> {
        if sector_size < 512 {
            return Err(GptError::InvalidHeaderSize);
        }

        let mut boot = vec![0u8; sector_size];

        boot[0..3].copy_from_slice(&[0xEB, 0x52, 0x90]);   // jump
        boot[3..11].copy_from_slice(b"NTFS    ");            // OEM ID
        boot[11..13].copy_from_slice(&(sector_size as u16).to_le_bytes()); // bytes/sector
        boot[13] = 8;                                        // sectors/cluster (4KB)
        boot[40..48].copy_from_slice(&total_sectors.to_le_bytes());
        boot[48..56].copy_from_slice(&2u64.to_le_bytes());  // $MFT cluster
        boot[72..80].copy_from_slice(&0x123456789ABCDEF0u64.to_le_bytes()); // serial
        boot[510] = 0x55;
        boot[511] = 0xAA;

        Ok(boot)
    }
}

// ─────────────────────────────────────────────
// Public types
// ─────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PartitionKind {
    Esp,
    Ntfs,
    Unknown,
}

#[derive(Debug, Clone)]
pub struct GptInfo {
    pub disk_guid:        String,
    pub sector_size:      usize,
    pub total_sectors:    u64,
    pub first_usable_lba: u64,
    pub last_usable_lba:  u64,
    pub partitions:       Vec<PartitionInfo>,
}

#[derive(Debug, Clone)]
pub struct PartitionInfo {
    pub kind:        PartitionKind,
    pub type_guid:   String,
    pub unique_guid: String,
    pub start_lba:   u64,
    pub end_lba:     u64,
    pub size_bytes:  u64,
    pub attributes:  u64,
    pub name:        String,
}

// ─────────────────────────────────────────────
// Helpers
// ─────────────────────────────────────────────

fn str_to_utf16le(s: &str) -> [u16; 36] {
    let mut name = [0u16; 36];
    for (i, c) in s.encode_utf16().take(36).enumerate() {
        name[i] = c;
    }
    name
}

fn utf16le_to_str(name: &[u16; 36]) -> String {
    let len = name.iter().position(|&c| c == 0).unwrap_or(36);
    let chars: Vec<u8> = name[..len]
        .iter()
        .filter_map(|&c| if c < 128 { Some(c as u8) } else { None })
        .collect();
    String::from_utf8_lossy(&chars).into_owned()
}

fn format_guid(guid: &[u8; 16]) -> String {
    format!(
        "{:08x}-{:04x}-{:04x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        u32::from_le_bytes([guid[0], guid[1], guid[2], guid[3]]),
        u16::from_le_bytes([guid[4], guid[5]]),
        u16::from_le_bytes([guid[6], guid[7]]),
        guid[8], guid[9],
        guid[10], guid[11], guid[12], guid[13], guid[14], guid[15],
    )
}

fn is_zero_guid(guid: &[u8; 16]) -> bool {
    guid.iter().all(|&b| b == 0)
}