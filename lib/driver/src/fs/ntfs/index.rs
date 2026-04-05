use alloc::{format, string::String, vec, vec::Vec};

use hal::block::BlockDevice;

use crate::{
    fs::ntfs::{
        NtfsDriver,
        attr::parse_filename_from_key,
        error::NtfsError,
        runs::parse_data_runs,
        types::NtfsFile,
    },
    util::debug,
};

/// Add a directory entry to parent's index (resident or external)
pub(crate) fn add_directory_entry<D: BlockDevice>(
    driver: &mut NtfsDriver<D>,
    parent: &NtfsFile,
    child_record: u64,
    child_name: &str,
) -> Result<(), NtfsError> {
    debug(&format!(
        "add_directory_entry: parent={}, child={}, name={}",
        parent.record_number(),
        child_record,
        child_name
    ));

    let mut record = driver.read_mft_record(parent.record_number())?;

    // Check if has external index ($INDEX_ALLOCATION)
    let has_external = find_attribute_offset(&record, 0xA0).is_ok();
    debug(&format!("has_external_index: {}", has_external));

    if has_external {
        // Try resident first, fall back to external on NoSpace
        match add_to_resident_index(driver, &mut record, parent, child_record, child_name) {
            Ok(()) => Ok(()),
            Err(NtfsError::NoSpace) => {
                debug("Resident full, trying external");
                add_to_external_index(driver, &record, parent, child_record, child_name)
            }
            Err(e) => Err(e),
        }
    } else {
        add_to_resident_index(driver, &mut record, parent, child_record, child_name)
    }
}

/// Add entry to resident $INDEX_ROOT
fn add_to_resident_index<D: BlockDevice>(
    driver: &mut NtfsDriver<D>,
    record: &mut [u8],
    parent: &NtfsFile,
    child_record: u64,
    child_name: &str,
) -> Result<(), NtfsError> {
    let index_attr_offset =
        find_attribute_offset(record, 0x90).map_err(|_| NtfsError::InvalidAttribute)?;

    // Verify resident flag
    if index_attr_offset + 8 >= record.len() || record[index_attr_offset + 8] != 0 {
        return Err(NtfsError::NotSupported);
    }

    // Parse attribute header
    if index_attr_offset + 22 >= record.len() {
        return Err(NtfsError::CorruptedFilesystem);
    }

    let val_off =
        u16::from_le_bytes([record[index_attr_offset + 20], record[index_attr_offset + 21]])
            as usize;
    let index_root_value_offset = index_attr_offset + val_off;
    let index_header_offset = index_root_value_offset + 16;

    if index_header_offset + 12 > record.len() {
        return Err(NtfsError::CorruptedFilesystem);
    }

    let first_entry_rel = u32::from_le_bytes(
        record[index_header_offset..index_header_offset + 4].try_into().unwrap(),
    ) as usize;
    let total_size = u32::from_le_bytes(
        record[index_header_offset + 4..index_header_offset + 8].try_into().unwrap(),
    ) as usize;
    let allocated_size = u32::from_le_bytes(
        record[index_header_offset + 8..index_header_offset + 12].try_into().unwrap(),
    ) as usize;

    debug(&format!(
        "resident_index: first={}, total={}, allocated={}",
        first_entry_rel, total_size, allocated_size
    ));

    let entries_end_offset = index_header_offset + total_size;
    if entries_end_offset < 16 || entries_end_offset > record.len() {
        return Err(NtfsError::CorruptedFilesystem);
    }

    let end_marker_offset = entries_end_offset - 16;

    // Calculate entry size
    let name_utf16: Vec<u16> = child_name.encode_utf16().collect();
    let name_len = name_utf16.len();
    let key_len = 66 + name_len * 2;
    let entry_len = 16 + key_len;
    let entry_len_aligned = (entry_len + 7) & !7;

    let new_total_size = total_size + entry_len_aligned;
    if new_total_size > allocated_size {
        debug("No space in resident index");
        return Err(NtfsError::NoSpace);
    }

    let new_end_marker_offset = end_marker_offset + entry_len_aligned;
    if new_end_marker_offset + 16 > record.len() {
        return Err(NtfsError::NoSpace);
    }

    // Shift end marker
    record.copy_within(end_marker_offset..entries_end_offset, new_end_marker_offset);

    // Write new entry
    let entry_start = end_marker_offset;
    let entry = &mut record[entry_start..entry_start + entry_len_aligned];
    entry.fill(0);

    // Entry header
    entry[0..8].copy_from_slice(&child_record.to_le_bytes());
    entry[8..10].copy_from_slice(&(entry_len_aligned as u16).to_le_bytes());
    entry[10..12].copy_from_slice(&(key_len as u16).to_le_bytes());
    entry[12..14].copy_from_slice(&0u16.to_le_bytes()); // Flags
    entry[14..16].copy_from_slice(&0u16.to_le_bytes()); // Reserved

    // Key data ($FILE_NAME format)
    let key = &mut entry[16..16 + key_len];
    key[0..8].copy_from_slice(&parent.record_number().to_le_bytes());
    key[8..56].fill(0); // Timestamps, sizes, flags (zeroed - should be fixed!)
    key[56..60].copy_from_slice(&0x10u32.to_le_bytes()); // Flags (directory?)
    key[60..64].fill(0);
    key[64] = name_len as u8;
    key[65] = 1; // Win32 & DOS namespace

    for (i, &c) in name_utf16.iter().enumerate() {
        let pos = 66 + i * 2;
        key[pos..pos + 2].copy_from_slice(&c.to_le_bytes());
    }

    // Update index header
    record[index_header_offset + 4..index_header_offset + 8]
        .copy_from_slice(&(new_total_size as u32).to_le_bytes());

    // Update MFT record used size
    let current_used = u32::from_le_bytes(record[24..28].try_into().unwrap());
    let new_used = current_used + entry_len_aligned as u32;
    record[24..28].copy_from_slice(&new_used.to_le_bytes());

    // Write back
    driver.reapply_fixups(record)?;
    let mft_offset =
        driver.boot.mft_byte_offset() + parent.record_number() * driver.mft_record_size as u64;
    driver.device.write_at(mft_offset, record).map_err(|_| NtfsError::IoError)?;

    Ok(())
}

/// Add entry to external $INDEX_ALLOCATION (INDX records)
fn add_to_external_index<D: BlockDevice>(
    driver: &mut NtfsDriver<D>,
    parent_record: &[u8],
    parent: &NtfsFile,
    child_record: u64,
    child_name: &str,
) -> Result<(), NtfsError> {
    debug("Adding to external index");

    // Get index record size from $INDEX_ROOT
    let idx_root_offset = find_attribute_offset(parent_record, 0x90)?;
    if idx_root_offset + 22 > parent_record.len() {
        return Err(NtfsError::CorruptedFilesystem);
    }

    let val_off = u16::from_le_bytes([
        parent_record[idx_root_offset + 20],
        parent_record[idx_root_offset + 21],
    ]) as usize;
    let idx_root_val = idx_root_offset + val_off;

    if idx_root_val + 12 > parent_record.len() {
        return Err(NtfsError::CorruptedFilesystem);
    }

    let idx_record_size =
        u32::from_le_bytes(parent_record[idx_root_val + 8..idx_root_val + 12].try_into().unwrap())
            as usize;

    debug(&format!("index_record_size: {}", idx_record_size));

    // Find $INDEX_ALLOCATION
    let idx_alloc_offset = find_attribute_offset(parent_record, 0xA0)?;
    debug(&format!("$INDEX_ALLOCATION at offset {}", idx_alloc_offset));

    if idx_alloc_offset + 48 > parent_record.len() {
        return Err(NtfsError::CorruptedFilesystem);
    }

    if parent_record[idx_alloc_offset + 8] != 1 {
        debug("$INDEX_ALLOCATION is not non-resident");
        return Err(NtfsError::NotSupported);
    }

    let allocated_size = u64::from_le_bytes(
        parent_record[idx_alloc_offset + 40..idx_alloc_offset + 48].try_into().unwrap(),
    );

    if allocated_size == 0 {
        return Err(NtfsError::NoSpace);
    }

    // Parse data runs
    let attr_slice = &parent_record[idx_alloc_offset..];
    let runs = parse_data_runs(attr_slice)?;

    if runs.is_empty() {
        return Err(NtfsError::NoSpace);
    }

    // Try each run for space (currently only implements first run - FIXME)
    let (start_cluster, _run_len) = &runs[0];
    let bytes_per_cluster = driver.boot.bytes_per_cluster();

    // Validate cluster number
    let partition_size = (driver.boot.total_sectors as u64) * (driver.boot.bytes_per_sector as u64);
    let max_cluster = partition_size / bytes_per_cluster;

    if *start_cluster > max_cluster {
        debug(&format!("Invalid cluster {} (max {})", start_cluster, max_cluster));
        return Err(NtfsError::CorruptedFilesystem);
    }

    let idx_offset = *start_cluster * bytes_per_cluster;
    let mut idx_buf = vec![0u8; idx_record_size];

    driver.device.read_at(idx_offset, &mut idx_buf).map_err(|_| NtfsError::IoError)?;

    // Verify INDX signature
    if &idx_buf[0..4] != b"INDX" {
        debug(&format!("Bad INDX signature: {:02x?}", &idx_buf[0..4]));
        return Err(NtfsError::CorruptedFilesystem);
    }

    // Apply fixups
    let uso = u16::from_le_bytes([idx_buf[4], idx_buf[5]]) as usize;
    let usc = u16::from_le_bytes([idx_buf[6], idx_buf[7]]) as usize;

    if uso < 24 || usc == 0 || uso + 2 * usc > idx_buf.len() {
        return Err(NtfsError::CorruptedFilesystem);
    }

    let seq = u16::from_le_bytes([idx_buf[uso], idx_buf[uso + 1]]);

    for i in 1..usc {
        let end = i * 512 - 2;
        if end + 2 > idx_buf.len() {
            return Err(NtfsError::CorruptedFilesystem);
        }
        let stored = u16::from_le_bytes([idx_buf[uso + i * 2], idx_buf[uso + i * 2 + 1]]);
        let bytes = u16::from_le_bytes([idx_buf[end], idx_buf[end + 1]]);
        if bytes != seq {
            return Err(NtfsError::CorruptedFilesystem);
        }
        idx_buf[end..end + 2].copy_from_slice(&stored.to_le_bytes());
    }

    // Parse index header within record
    if 24 + 6 > idx_buf.len() {
        return Err(NtfsError::CorruptedFilesystem);
    }

    let entries_offset = u16::from_le_bytes([idx_buf[24], idx_buf[25]]) as usize;
    let total_size = u16::from_le_bytes([idx_buf[26], idx_buf[27]]) as usize;
    let allocated_size = u16::from_le_bytes([idx_buf[28], idx_buf[29]]) as usize;

    let entries_end = 24 + entries_offset + total_size;
    if entries_end < 16 || entries_end > idx_buf.len() {
        return Err(NtfsError::CorruptedFilesystem);
    }

    let end_marker = entries_end - 16;

    // Prepare entry
    let name_utf16: Vec<u16> = child_name.encode_utf16().collect();
    let name_len = name_utf16.len();
    let key_len = 66 + name_len * 2;
    let entry_len = 16 + key_len;
    let entry_len_aligned = (entry_len + 7) & !7;

    let new_total = total_size + entry_len_aligned;
    if new_total > allocated_size {
        return Err(NtfsError::NoSpace);
    }

    // Move end marker and write entry
    idx_buf.copy_within(end_marker..entries_end, end_marker + entry_len_aligned);

    let entry = &mut idx_buf[end_marker..end_marker + entry_len_aligned];
    entry.fill(0);
    entry[0..8].copy_from_slice(&child_record.to_le_bytes());
    entry[8..10].copy_from_slice(&(entry_len_aligned as u16).to_le_bytes());
    entry[10..12].copy_from_slice(&(key_len as u16).to_le_bytes());

    let key = &mut entry[16..16 + key_len];
    key[0..8].copy_from_slice(&parent.record_number().to_le_bytes());
    key[8..56].fill(0); // FIXME: Invalid timestamps/flags!
    key[56..60].copy_from_slice(&0x10u32.to_le_bytes());
    key[60..64].fill(0);
    key[64] = name_len as u8;
    key[65] = 1;

    for (i, &c) in name_utf16.iter().enumerate() {
        key[66 + i * 2..66 + i * 2 + 2].copy_from_slice(&c.to_le_bytes());
    }

    // Update header
    idx_buf[26..28].copy_from_slice(&(new_total as u16).to_le_bytes());

    // Reapply fixups
    for i in 1..usc {
        let end = i * 512 - 2;
        let original = u16::from_le_bytes([idx_buf[end], idx_buf[end + 1]]);
        idx_buf[uso + i * 2..uso + i * 2 + 2].copy_from_slice(&original.to_le_bytes());
        idx_buf[end..end + 2].copy_from_slice(&seq.to_le_bytes());
    }

    driver.device.write_at(idx_offset, &idx_buf).map_err(|_| NtfsError::IoError)?;
    Ok(())
}

/// Find attribute offset in MFT record by type code
fn find_attribute_offset(record: &[u8], attr_type: u32) -> Result<usize, NtfsError> {
    if record.len() < 22 {
        return Err(NtfsError::InvalidMftRecord);
    }

    let first = u16::from_le_bytes([record[20], record[21]]) as usize;
    let mut offset = first;

    while offset + 8 <= record.len() {
        let type_code = u32::from_le_bytes([
            record[offset],
            record[offset + 1],
            record[offset + 2],
            record[offset + 3],
        ]);

        if type_code == 0xFFFFFFFF {
            break;
        }

        if type_code == attr_type {
            return Ok(offset);
        }

        let attr_len = u32::from_le_bytes([
            record[offset + 4],
            record[offset + 5],
            record[offset + 6],
            record[offset + 7],
        ]) as usize;

        if attr_len == 0 || offset + attr_len > record.len() {
            break;
        }
        offset += attr_len;
    }

    Err(NtfsError::InvalidAttribute)
}

/// List filenames in directory index
pub(crate) fn list_directory(index_data: &[u8]) -> Result<Vec<String>, NtfsError> {
    let mut result = Vec::new();

    if index_data.len() < 32 {
        return Ok(result);
    }

    let index_header_offset = 16;
    if index_data.len() < index_header_offset + 8 {
        return Ok(result);
    }

    let first_entry_rel = u32::from_le_bytes(
        index_data[index_header_offset..index_header_offset + 4].try_into().unwrap(),
    ) as usize;
    let total_size = u32::from_le_bytes(
        index_data[index_header_offset + 4..index_header_offset + 8].try_into().unwrap(),
    ) as usize;

    let mut offset = index_header_offset + first_entry_rel;
    let end = index_header_offset + total_size;

    while offset + 16 <= end.min(index_data.len()) {
        let entry = &index_data[offset..];
        let entry_len = u16::from_le_bytes([entry[8], entry[9]]) as usize;
        let key_len = u16::from_le_bytes([entry[10], entry[11]]) as usize;
        let flags = u16::from_le_bytes([entry[12], entry[13]]);

        if flags & 0x0002 != 0 {
            break; // End marker
        }

        if key_len > 0 && offset + 16 + key_len <= index_data.len() {
            let key = &entry[16..16 + key_len];
            if let Some((name, _)) = parse_filename_from_key(key) {
                result.push(name);
            }
        }

        if entry_len == 0 {
            break;
        }
        offset += entry_len;
    }

    Ok(result)
}

/// Find record number by filename in directory index
pub(crate) fn find_in_directory(index_data: &[u8], name: &str) -> Result<u64, NtfsError> {
    if index_data.len() < 32 {
        return Err(NtfsError::FileNotFound);
    }

    let index_header_offset = 16;
    let first_entry_rel = u32::from_le_bytes(
        index_data[index_header_offset..index_header_offset + 4].try_into().unwrap(),
    ) as usize;
    let total_size = u32::from_le_bytes(
        index_data[index_header_offset + 4..index_header_offset + 8].try_into().unwrap(),
    ) as usize;

    let mut offset = index_header_offset + first_entry_rel;
    let end = index_header_offset + total_size;

    while offset + 16 <= end.min(index_data.len()) {
        let entry = &index_data[offset..];
        let file_ref = u64::from_le_bytes(entry[0..8].try_into().unwrap());
        let entry_len = u16::from_le_bytes([entry[8], entry[9]]) as usize;
        let key_len = u16::from_le_bytes([entry[10], entry[11]]) as usize;
        let flags = u16::from_le_bytes([entry[12], entry[13]]);

        if flags & 0x0002 != 0 {
            break;
        }

        if key_len > 0 && offset + 16 + key_len <= index_data.len() {
            let key = &entry[16..16 + key_len];
            if let Some((entry_name, _)) = parse_filename_from_key(key) {
                if entry_name.eq_ignore_ascii_case(name) {
                    return Ok(file_ref & 0x0000FFFFFFFFFFFF);
                }
            }
        }

        if entry_len == 0 {
            break;
        }
        offset += entry_len;
    }

    Err(NtfsError::FileNotFound)
}

/// Find $INDEX_ROOT attribute offset
pub(crate) fn find_index_root_offset(record: &[u8]) -> Result<usize, NtfsError> {
    find_attribute_offset(record, 0x90)
}
