use alloc::{vec, vec::Vec};

use hal::block::BlockDevice;

use crate::partition::gpt::{
    crc32,
    error::GptError,
    generate_guid,
    serialize_entries,
    structs::GptEntry,
    GPT_ENTRY_SIZE,
    GPT_FIRST_USABLE_LBA,
    GPT_HEADER_SIZE,
    GPT_MAX_ENTRIES,
    GPT_RESERVED_LBAS,
    GPT_REVISION,
    GPT_SIGNATURE,
};

/// Write GPT to disk with proper ordering (entries before headers)
///
/// WARNING: This is not atomic. If power is lost during write, the disk
/// may be left with inconsistent headers. Always keep backups.
pub(crate) fn write_gpt(dev: &mut dyn BlockDevice, entries: &[GptEntry]) -> Result<(), GptError> {
    if entries.len() > GPT_MAX_ENTRIES as usize {
        return Err(GptError::NoSpace);
    }

    let sector = dev.block_size() as u64;
    let last_lba = dev.block_count().checked_sub(1).ok_or(GptError::NoSpace)?;

    // Write protective MBR first so firmware sees a valid protective entry
    write_protective_mbr(dev, last_lba)?;

    // Validate entries fit within usable space
    let entries_end_lba =
        2u64 + ((entries.len() as u64 * GPT_ENTRY_SIZE as u64 + sector - 1) / sector);
    if entries_end_lba >= last_lba.saturating_sub(GPT_RESERVED_LBAS) {
        return Err(GptError::NoSpace);
    }

    // 1. Serialize entry array and compute its CRC once
    let entry_array = serialize_entries(entries);
    let entry_array_crc = crc32(&entry_array);

    // Generate a single disk GUID shared by both headers
    let disk_guid = generate_guid();

    let backup_entries_lba = last_lba - GPT_RESERVED_LBAS + 1;

    // 2. Build primary and backup headers
    let mut primary = build_header(
        1,        // my_lba
        last_lba, // alternate_lba
        2,        // partition_entry_lba (primary entries start at LBA 2)
        entry_array_crc,
        last_lba,
        sector,
        &disk_guid,
        entries.len() as u32,
    )?;

    let mut backup = build_header(
        last_lba,           // my_lba
        1,                  // alternate_lba
        backup_entries_lba, // partition_entry_lba (backup entries before last header)
        entry_array_crc,
        last_lba,
        sector,
        &disk_guid,
        entries.len() as u32,
    )?;

    // 3. Finalize CRCs (must be done after all other fields are set)
    finalize_header_crc(&mut primary);
    finalize_header_crc(&mut backup);

    // 4. Write in safe order: entries first, then headers. This ensures headers
    //    never point to unwritten entry data if power is lost mid-write.

    // Primary partition entries (LBA 2)
    dev.write_blocks(2 * sector, &entry_array)?;

    // Backup partition entries (last_lba - GPT_RESERVED_LBAS + 1)
    dev.write_blocks(backup_entries_lba * sector, &entry_array)?;

    // Primary GPT header (LBA 1)
    dev.write_blocks(1 * sector, &primary)?;

    // Backup GPT header (last LBA)
    dev.write_blocks(last_lba * sector, &backup)?;

    Ok(())
}

fn build_header(
    my_lba: u64,
    alternate_lba: u64,
    partition_entry_lba: u64,
    entry_array_crc: u32,
    last_lba: u64,
    sector: u64,
    disk_guid: &[u8; 16],
    num_entries: u32,
) -> Result<Vec<u8>, GptError> {
    if sector < GPT_HEADER_SIZE as u64 {
        return Err(GptError::InvalidHeaderSize);
    }

    let first_usable = GPT_FIRST_USABLE_LBA;
    let last_usable = last_lba.saturating_sub(GPT_RESERVED_LBAS);

    let mut h = vec![0u8; sector as usize];

    h[0..8].copy_from_slice(GPT_SIGNATURE); // Signature
    h[8..12].copy_from_slice(&GPT_REVISION); // Revision
    h[12..16].copy_from_slice(&GPT_HEADER_SIZE.to_le_bytes()); // HeaderSize
    // [16..20] header CRC — left as zero, filled by finalize_header_crc
    // [20..24] reserved — zero
    h[24..32].copy_from_slice(&my_lba.to_le_bytes()); // MyLBA
    h[32..40].copy_from_slice(&alternate_lba.to_le_bytes()); // AlternateLBA
    h[40..48].copy_from_slice(&first_usable.to_le_bytes()); // FirstUsableLBA
    h[48..56].copy_from_slice(&last_usable.to_le_bytes()); // LastUsableLBA
    h[56..72].copy_from_slice(disk_guid); // DiskGUID
    h[72..80].copy_from_slice(&partition_entry_lba.to_le_bytes()); // PartitionEntryLBA
    h[80..84].copy_from_slice(&num_entries.to_le_bytes()); // NumberOfPartitionEntries
    h[84..88].copy_from_slice(&GPT_ENTRY_SIZE.to_le_bytes()); // SizeOfPartitionEntry
    h[88..92].copy_from_slice(&entry_array_crc.to_le_bytes()); // PartitionEntryArrayCRC32

    Ok(h)
}

fn finalize_header_crc(header: &mut Vec<u8>) {
    // The CRC field itself must be zero when computing the checksum
    header[16..20].copy_from_slice(&[0u8; 4]);
    let crc = crc32(&header[..GPT_HEADER_SIZE as usize]);
    header[16..20].copy_from_slice(&crc.to_le_bytes());
}

fn write_protective_mbr(dev: &mut dyn BlockDevice, last_lba: u64) -> Result<(), GptError> {
    let sector = dev.block_size() as u64;
    let mut mbr = vec![0u8; sector as usize];

    // Single partition entry at offset 446
    mbr[446] = 0x00; // Not bootable
    // CHS start: head=0, sector=2, cylinder=0 (conventional for GPT protective)
    mbr[447] = 0x00;
    mbr[448] = 0x02;
    mbr[449] = 0x00;
    // Partition type: 0xEE = GPT protective
    mbr[450] = 0xEE;
    // CHS end: 0xFFFFFF (ignored by GPT-aware firmware)
    mbr[451] = 0xFF;
    mbr[452] = 0xFF;
    mbr[453] = 0xFF;
    // LBA of first sector: always 1
    mbr[454..458].copy_from_slice(&1u32.to_le_bytes());
    // Number of sectors: capped at 0xFFFFFFFF for disks larger than 2TB
    let size = last_lba.min(0xFFFF_FFFF) as u32;
    mbr[458..462].copy_from_slice(&size.to_le_bytes());

    // MBR boot signature
    mbr[510] = 0x55;
    mbr[511] = 0xAA;

    dev.write_blocks(0, &mbr)?;
    Ok(())
}
