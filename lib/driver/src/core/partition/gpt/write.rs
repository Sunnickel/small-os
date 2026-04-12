use alloc::{vec, vec::Vec};

use hal::block::BlockDevice;

use crate::core::partition::gpt::{
    GPT_ENTRY_SIZE,
    GPT_FIRST_USABLE_LBA,
    GPT_HEADER_SIZE,
    GPT_MAX_ENTRIES,
    GPT_RESERVED_LBAS,
    GPT_REVISION,
    GPT_SIGNATURE,
    crc32,
    error::GptError,
    generate_guid,
    serialize_entries,
    structs::GptEntry,
};

/// Write GPT to disk with proper ordering (entries before headers)
///
/// WARNING: This is not atomic. If power is lost during write, the disk
/// may be left with inconsistent headers. Always keep backups.
pub(crate) fn write_gpt(dev: &mut impl BlockDevice, entries: &[GptEntry]) -> Result<(), GptError> {
    if entries.len() > GPT_MAX_ENTRIES as usize {
        return Err(GptError::NoSpace);
    }

    let sector = dev.sector_size() as u64;
    let last_lba = dev.block_count().checked_sub(1).ok_or(GptError::NoSpace)?;

    // Validate entries fit within usable space
    let entries_end_lba = 2u64 + ((entries.len() as u64 * 128 + sector - 1) / sector);
    if entries_end_lba >= last_lba.saturating_sub(32) {
        return Err(GptError::NoSpace);
    }

    // 1. Serialize entry array
    let entry_array = serialize_entries(entries);
    let entry_array_crc = crc32(&entry_array);

    // 2. Build headers
    let mut primary = build_header(
        1,        // my_lba
        last_lba, // alternate_lba
        2,        // partition_entry_lba
        &entry_array,
        entry_array_crc,
        last_lba,
        sector,
    )?;

    let backup_entries_lba = last_lba - 32;
    let mut backup = build_header(
        last_lba,           // my_lba
        1,                  // alternate_lba
        backup_entries_lba, // partition_entry_lba
        &entry_array,
        entry_array_crc,
        last_lba,
        sector,
    )?;

    // 3. Calculate CRCs
    finalize_header_crc(&mut primary);
    finalize_header_crc(&mut backup);

    // 4. Write in safe order: entries first, then headers This ensures headers
    //    point to valid data if interrupted

    // Primary entries (LBA 2)
    dev.write_at(2 * sector, &entry_array)?;

    // Backup entries (last_lba - 32)
    dev.write_at(backup_entries_lba * sector, &entry_array)?;

    // Primary header (LBA 1)
    dev.write_at(1 * sector, &primary)?;

    // Backup header (last_lba)
    dev.write_at(last_lba * sector, &backup)?;

    Ok(())
}

fn build_header(
    my_lba: u64,
    alternate_lba: u64,
    partition_entry_lba: u64,
    entry_array: &[u8],
    entry_array_crc: u32,
    last_lba: u64,
    sector: u64,
) -> Result<Vec<u8>, GptError> {
    if sector < GPT_HEADER_SIZE as u64 {
        return Err(GptError::InvalidHeaderSize);
    }

    let num_entries = (entry_array.len() / 128) as u32;

    let mut h = vec![0u8; sector as usize];
    h[0..8].copy_from_slice(GPT_SIGNATURE);
    h[8..12].copy_from_slice(&GPT_REVISION);
    h[12..16].copy_from_slice(&GPT_HEADER_SIZE.to_le_bytes());
    h[24..32].copy_from_slice(&my_lba.to_le_bytes());
    h[32..40].copy_from_slice(&alternate_lba.to_le_bytes());
    h[40..48].copy_from_slice(&GPT_FIRST_USABLE_LBA.to_le_bytes());
    h[48..56].copy_from_slice(&last_lba.saturating_sub(GPT_RESERVED_LBAS).to_le_bytes());
    h[56..72].copy_from_slice(&generate_guid());
    h[72..80].copy_from_slice(&partition_entry_lba.to_le_bytes());
    h[80..84].copy_from_slice(&num_entries.to_le_bytes());
    h[84..88].copy_from_slice(&GPT_ENTRY_SIZE.to_le_bytes());
    h[88..92].copy_from_slice(&entry_array_crc.to_le_bytes());

    Ok(h)
}

fn finalize_header_crc(header: &mut Vec<u8>) {
    // Zero CRC field
    header[16..20].copy_from_slice(&[0u8; 4]);
    // Calculate over header_size bytes
    let crc = crc32(&header[0..GPT_HEADER_SIZE as usize]);
    header[16..20].copy_from_slice(&crc.to_le_bytes());
}
