use alloc::vec::Vec;
use core::sync::atomic::{AtomicU64, Ordering};

pub mod error;
pub mod read;
pub mod structs;
pub mod write;
pub mod manager;

pub const GPT_SIGNATURE: &[u8; 8] = b"EFI PART";
pub const GPT_REVISION: [u8; 4] = [0x00, 0x00, 0x01, 0x00];
pub const GPT_HEADER_SIZE: u32 = 92;
pub const GPT_ENTRY_SIZE: u32 = 128;
pub const GPT_MAX_ENTRIES: u32 = 128;
pub const GPT_FIRST_USABLE_LBA: u64 = 34;
pub const GPT_RESERVED_LBAS: u64 = 33;

static GUID_COUNTER: AtomicU64 = AtomicU64::new(1);

pub(crate) fn generate_guid() -> [u8; 16] {
    let n = GUID_COUNTER.fetch_add(1, Ordering::SeqCst);
    let mut g = [0u8; 16];
    g[0..8].copy_from_slice(&n.to_le_bytes());
    g[8..16].copy_from_slice(&n.wrapping_mul(0x9E3779B97F4A7C15).to_le_bytes());
    g[6] = (g[6] & 0x0F) | 0x40;  // Version 4
    g[8] = (g[8] & 0x3F) | 0x80;  // Variant 10
    g
}

static CRC32C_TABLE: [u32; 256] = {
    let mut table = [0u32; 256];
    let mut i = 0;
    while i < 256 {
        let mut crc = i as u32;
        let mut j = 0;
        while j < 8 {
            crc = if crc & 1 != 0 {
                (crc >> 1) ^ 0x82F63B78
            } else {
                crc >> 1
            };
            j += 1;
        }
        table[i] = crc;
        i += 1;
    }
    table
};

pub(crate) fn crc32(data: &[u8]) -> u32 {
    let mut crc: u32 = !0;
    for &byte in data {
        crc = CRC32C_TABLE[((crc as u8) ^ byte) as usize] ^ (crc >> 8);
    }
    !crc
}

pub(crate) fn serialize_entries(entries: &[structs::GptEntry]) -> Vec<u8> {
    let mut buf = Vec::with_capacity(entries.len() * 128);
    for entry in entries {
        buf.extend_from_slice(&entry.type_guid);
        buf.extend_from_slice(&entry.unique_guid);
        buf.extend_from_slice(&entry.start_lba.to_le_bytes());
        buf.extend_from_slice(&entry.end_lba.to_le_bytes());
        buf.extend_from_slice(&entry.attributes.to_le_bytes());
        for &unit in &entry.name {
            buf.extend_from_slice(&unit.to_le_bytes());
        }
    }
    buf
}