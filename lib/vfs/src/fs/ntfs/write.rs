use alloc::vec::Vec;

use hal::block::BlockDevice;

use crate::{
    Inode,
    fs::ntfs::{
        NtfsDriver,
        attr::{find_data_attribute_offset, reapply_fixups, update_resident_data},
        error::NtfsError,
        index::add_directory_entry,
        types::{CreateOptions, DataRun, NtfsFile},
    },
};

/// Overwrite content of a resident file in-place (size must match)
pub(crate) fn write_resident_file<D: BlockDevice>(
    driver: &mut NtfsDriver<D>,
    file: &NtfsFile,
    data: &[u8],
) -> Result<(), NtfsError> {
    let stat = driver.stat(file)?;
    let run = stat.data_runs.first().ok_or(NtfsError::InvalidAttribute)?;

    match run {
        DataRun::Resident { data: existing } => {
            if data.len() != existing.len() {
                return Err(NtfsError::InvalidAttribute); // Cannot resize resident files
            }
        }
        DataRun::NonResident(_) => {
            return Err(NtfsError::InvalidAttribute); // Only resident supported
        }
    }

    let mut record = driver.read_mft_record(file.record_number())?;
    let data_attr_offset = find_data_attribute_offset(&record)?;
    update_resident_data(&mut record, data_attr_offset, data)?;

    reapply_fixups(&mut record, driver.boot.bytes_per_sector as usize)?;

    let mft_offset =
        driver.boot.mft_byte_offset() + file.record_number() * driver.mft_record_size as u64;
    driver.device.write_blocks(mft_offset, &record).map_err(|_| NtfsError::IoError)?;

    Ok(())
}

/// Create new file or directory in parent
pub(crate) fn create_file<D: BlockDevice>(
    driver: &mut NtfsDriver<D>,
    parent: &NtfsFile,
    name: &str,
    options: CreateOptions,
) -> Result<NtfsFile, NtfsError> {
    // Validate parent is directory
    if !driver.is_directory(parent)? {
        return Err(NtfsError::NotADirectory);
    }

    // Check for existing name
    match driver.find_in_directory(parent, name) {
        Ok(_) => return Err(NtfsError::AlreadyExists),
        Err(NtfsError::FileNotFound) => {}
        Err(e) => return Err(e),
    }

    // Validate name length
    if name.len() > 255 {
        return Err(NtfsError::NameTooLong);
    }

    // Allocate record
    let new_record_number = allocate_mft_record(driver)?;

    // Create record
    let mut record = build_mft_record(
        driver,
        new_record_number,
        name,
        parent.record_number(),
        options.is_directory,
        &options.data,
    )?;

    reapply_fixups(&mut record, driver.boot.bytes_per_sector as usize)?;

    // Write MFT record
    let mft_offset =
        driver.boot.mft_byte_offset() + new_record_number * driver.mft_record_size as u64;
    driver.device.write_blocks(mft_offset, &record).map_err(|_| NtfsError::IoError)?;

    // Update bitmap (FIXME: Currently not implemented - critical bug!)
    // update_mft_bitmap(driver, new_record_number, true)?;

    // Add to parent directory
    add_directory_entry(driver, parent, new_record_number, name)?;

    driver.open_file(new_record_number)
}

/// Scan for free MFT record (simplified, does not check $MFT Bitmap)
fn allocate_mft_record<D: BlockDevice>(driver: &mut NtfsDriver<D>) -> Result<u64, NtfsError> {
    const START_SCAN: u64 = 16; // After system files
    const MAX_SCAN: u64 = 100000;

    for num in START_SCAN..MAX_SCAN {
        let offset = driver.boot.mft_byte_offset() + num * driver.mft_record_size as u64;
        let mut buf = alloc::vec![0u8; driver.mft_record_size];

        if driver.device.read_blocks(offset, &mut buf).is_ok() {
            let sig = &buf[0..4];
            // Free if not "FILE" or "BAAD"
            if sig != b"FILE" && sig != b"BAAD" {
                return Ok(num);
            }
        }
    }

    Err(NtfsError::NoSpace)
}

/// Build MFT record for new file/directory
fn build_mft_record<D: BlockDevice>(
    driver: &NtfsDriver<D>,
    record_number: u64,
    name: &str,
    parent_record: u64,
    is_directory: bool,
    data: &[u8],
) -> Result<Vec<u8>, NtfsError> {
    let mut record = alloc::vec![0u8; driver.mft_record_size];
    let seq = (record_number & 0xFFFF) as u16;

    // MFT Record Header (48 bytes)
    record[0x00..0x04].copy_from_slice(b"FILE");
    record[0x04..0x06].copy_from_slice(&48u16.to_le_bytes()); // Update Sequence Offset
    record[0x06..0x08].copy_from_slice(&3u16.to_le_bytes()); // Update Sequence Count (1 + 2 fixups)
    record[0x08..0x10].copy_from_slice(&0u64.to_le_bytes()); // LSN
    record[0x10..0x12].copy_from_slice(&seq.to_le_bytes()); // Sequence Number
    record[0x12..0x14].copy_from_slice(&1u16.to_le_bytes()); // Hard Link Count
    record[0x14..0x16].copy_from_slice(&56u16.to_le_bytes()); // First Attribute Offset
    record[0x16..0x18].copy_from_slice(&1u16.to_le_bytes()); // Flags (in-use)
    record[0x18..0x1C].copy_from_slice(&0u32.to_le_bytes()); // Used size (updated later)
    record[0x1C..0x20].copy_from_slice(&(driver.mft_record_size as u32).to_le_bytes()); // Allocated size
    record[0x20..0x28].copy_from_slice(&0u64.to_le_bytes()); // Base file record
    record[0x28..0x2A].copy_from_slice(&0u16.to_le_bytes()); // Next Attribute ID
    record[0x2A..0x30].fill(0);

    // Update Sequence Array at offset 48
    record[0x30..0x32].copy_from_slice(&seq.to_le_bytes());
    record[0x32..0x36].fill(0); // Fixup entries (sector size / 512 - 1)

    // Build attributes at offset 56
    let mut off = 56usize;
    off = write_standard_info_attr(&mut record, off, is_directory)?;
    off = write_filename_attr(&mut record, off, name, parent_record)?;

    if is_directory {
        off = write_index_root_attr(driver, &mut record, off)?;
    } else {
        off = write_data_attr(&mut record, off, data)?;
    }

    // End marker (0xFFFFFFFF)
    record[off..off + 4].copy_from_slice(&0xFFFFFFFFu32.to_le_bytes());
    off += 4;

    // Update used size (align to 8)
    let used_size = ((off + 7) & !7) as u32;
    record[0x18..0x1C].copy_from_slice(&used_size.to_le_bytes());

    Ok(record)
}

/// Write $STANDARD_INFORMATION attribute
fn write_standard_info_attr(
    record: &mut [u8],
    offset: usize,
    is_directory: bool,
) -> Result<usize, NtfsError> {
    const DATA_LEN: u32 = 72;
    const ATTR_LEN: u32 = 24 + DATA_LEN;

    if offset + ATTR_LEN as usize > record.len() {
        return Err(NtfsError::NoSpace);
    }

    // Header
    record[offset..offset + 4].copy_from_slice(&0x10u32.to_le_bytes());
    record[offset + 4..offset + 8].copy_from_slice(&ATTR_LEN.to_le_bytes());
    record[offset + 8] = 0; // Resident
    record[offset + 9] = 0; // No name
    record[offset + 10..offset + 12].copy_from_slice(&0u16.to_le_bytes());
    record[offset + 12..offset + 14].copy_from_slice(&0u16.to_le_bytes());
    record[offset + 14..offset + 16].copy_from_slice(&0u16.to_le_bytes());
    record[offset + 16..offset + 20].copy_from_slice(&DATA_LEN.to_le_bytes());
    record[offset + 20..offset + 22].copy_from_slice(&24u16.to_le_bytes());
    record[offset + 22..offset + 24].fill(0);

    // Data (72 bytes) - FIXME: All zeros = invalid timestamps!
    let d = offset + 24;
    let now = 0u64; // TODO: Get current NTFS time
    let flags: u32 = if is_directory { 0x10 } else { 0x20 };

    record[d..d + 8].copy_from_slice(&now.to_le_bytes());
    record[d + 8..d + 16].copy_from_slice(&now.to_le_bytes());
    record[d + 16..d + 24].copy_from_slice(&now.to_le_bytes());
    record[d + 24..d + 32].copy_from_slice(&now.to_le_bytes());
    record[d + 32..d + 36].copy_from_slice(&flags.to_le_bytes());
    record[d + 36..d + 72].fill(0);

    Ok(offset + ATTR_LEN as usize)
}

/// Write $FILE_NAME attribute
fn write_filename_attr(
    record: &mut [u8],
    offset: usize,
    name: &str,
    parent_record: u64,
) -> Result<usize, NtfsError> {
    let utf16: Vec<u16> = name.encode_utf16().collect();
    let name_len = utf16.len();

    if name_len > 255 {
        return Err(NtfsError::NameTooLong);
    }

    let data_len = 66 + name_len * 2;
    let attr_len = (24 + data_len + 7) & !7;

    if offset + attr_len > record.len() {
        return Err(NtfsError::NoSpace);
    }

    // Header
    record[offset..offset + 4].copy_from_slice(&0x30u32.to_le_bytes());
    record[offset + 4..offset + 8].copy_from_slice(&(attr_len as u32).to_le_bytes());
    record[offset + 8] = 0;
    record[offset + 9] = 0;
    record[offset + 10..offset + 12].copy_from_slice(&0u16.to_le_bytes());
    record[offset + 12..offset + 16].copy_from_slice(&0u32.to_le_bytes());
    record[offset + 16..offset + 20].copy_from_slice(&(data_len as u32).to_le_bytes());
    record[offset + 20..offset + 22].copy_from_slice(&24u16.to_le_bytes());
    record[offset + 22] = 1; // Indexed flag
    record[offset + 23] = 0;

    // Data
    let d = offset + 24;
    record[d..d + 8].copy_from_slice(&parent_record.to_le_bytes());

    // FIXME: Bytes 8-64 should contain timestamps and file size, not zeros!
    record[d + 8..d + 64].fill(0);
    record[d + 64] = name_len as u8;
    record[d + 65] = 1; // Win32 & DOS namespace

    for (i, &c) in utf16.iter().enumerate() {
        let pos = d + 66 + i * 2;
        record[pos..pos + 2].copy_from_slice(&c.to_le_bytes());
    }

    Ok(offset + attr_len)
}

/// Write resident $DATA attribute
fn write_data_attr(record: &mut [u8], offset: usize, data: &[u8]) -> Result<usize, NtfsError> {
    // Limit for resident data to avoid overflowing MFT record
    const MAX_RESIDENT_DATA: usize = 700;

    if data.len() > MAX_RESIDENT_DATA {
        return Err(NtfsError::InvalidAttribute); // Should implement non-resident
    }

    let data_len = data.len();
    let attr_len = (24 + data_len + 7) & !7;

    if offset + attr_len > record.len() {
        return Err(NtfsError::NoSpace);
    }

    record[offset..offset + 4].copy_from_slice(&0x80u32.to_le_bytes());
    record[offset + 4..offset + 8].copy_from_slice(&(attr_len as u32).to_le_bytes());
    record[offset + 8] = 0; // Resident
    record[offset + 9] = 0; // No name
    record[offset + 10..offset + 12].copy_from_slice(&0u16.to_le_bytes());
    record[offset + 12..offset + 16].copy_from_slice(&0u32.to_le_bytes());
    record[offset + 16..offset + 20].copy_from_slice(&(data_len as u32).to_le_bytes());
    record[offset + 20..offset + 22].copy_from_slice(&24u16.to_le_bytes());
    record[offset + 22..offset + 24].fill(0);

    record[offset + 24..offset + 24 + data_len].copy_from_slice(data);
    record[offset + 24 + data_len..offset + attr_len].fill(0);

    Ok(offset + attr_len)
}

/// Write $INDEX_ROOT attribute for directories
fn write_index_root_attr<D: BlockDevice>(
    driver: &NtfsDriver<D>,
    record: &mut [u8],
    offset: usize,
) -> Result<usize, NtfsError> {
    const INDEX_ROOT_HEADER_LEN: usize = 16;
    const INDEX_HEADER_LEN: usize = 16;
    const END_ENTRY_LEN: usize = 16;

    let data_len = INDEX_ROOT_HEADER_LEN + INDEX_HEADER_LEN + END_ENTRY_LEN;
    let attr_len = 24 + data_len;

    if offset + attr_len > record.len() {
        return Err(NtfsError::NoSpace);
    }

    // Attribute header
    record[offset..offset + 4].copy_from_slice(&0x90u32.to_le_bytes());
    record[offset + 4..offset + 8].copy_from_slice(&(attr_len as u32).to_le_bytes());
    record[offset + 8] = 0;
    record[offset + 9] = 0;
    record[offset + 10..offset + 12].copy_from_slice(&0u16.to_le_bytes());
    record[offset + 12..offset + 16].copy_from_slice(&0u32.to_le_bytes());
    record[offset + 16..offset + 20].copy_from_slice(&(data_len as u32).to_le_bytes());
    record[offset + 20..offset + 22].copy_from_slice(&24u16.to_le_bytes());
    record[offset + 22..offset + 24].fill(0);

    // Index Root Header (16 bytes)
    let d = offset + 24;
    record[d..d + 4].copy_from_slice(&0x30u32.to_le_bytes()); // $FILE_NAME type
    record[d + 4..d + 8].copy_from_slice(&0u32.to_le_bytes()); // Collation rule
    record[d + 8..d + 12].copy_from_slice(&(driver.mft_record_size as u32).to_le_bytes());
    // FIXME: clusters_per_index_buffer may not exist on BootSector
    record[d + 12] = 1; // Default to 1 cluster if field missing
    record[d + 13..d + 16].fill(0);

    // Index Header (16 bytes)
    let ih = d + 16;
    let first_entry_offset = (INDEX_ROOT_HEADER_LEN + INDEX_HEADER_LEN) as u32;
    let total_size = (INDEX_ROOT_HEADER_LEN + INDEX_HEADER_LEN + END_ENTRY_LEN) as u32;

    record[ih..ih + 4].copy_from_slice(&first_entry_offset.to_le_bytes());
    record[ih + 4..ih + 8].copy_from_slice(&total_size.to_le_bytes());
    record[ih + 8..ih + 12].copy_from_slice(&total_size.to_le_bytes());
    record[ih + 12..ih + 16].fill(0);

    // End entry (16 bytes)
    let le = d + 32;
    record[le..le + 8].copy_from_slice(&0u64.to_le_bytes());
    record[le + 8..le + 10].copy_from_slice(&16u16.to_le_bytes());
    record[le + 10..le + 12].copy_from_slice(&0u16.to_le_bytes());
    record[le + 12..le + 14].copy_from_slice(&0x0002u16.to_le_bytes()); // END_ENTRY
    record[le + 14..le + 16].fill(0);

    Ok(offset + attr_len)
}
