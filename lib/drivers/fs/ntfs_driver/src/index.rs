use alloc::{string::String, vec::Vec};

use hal::block::BlockDevice;

use crate::{attr::parse_filename_from_key, NtfsDriver, NtfsError, NtfsFile};

/// Walk a raw index block and return all filenames.
pub fn list_from_index(index_data: &[u8]) -> Result<Vec<String>, NtfsError> {
    let mut result = Vec::new();
    if index_data.len() < 16 {
        return Ok(result);
    }
    let first_entry =
        u32::from_le_bytes([index_data[0], index_data[1], index_data[2], index_data[3]]) as usize;
    let total_size =
        u32::from_le_bytes([index_data[4], index_data[5], index_data[6], index_data[7]]) as usize;

    let mut offset = first_entry;
    while offset + 0x12 <= total_size && offset + 0x12 <= index_data.len() {
        let entry = &index_data[offset..];
        let entry_len = u16::from_le_bytes([entry[8], entry[9]]) as usize;
        let key_len = u16::from_le_bytes([entry[10], entry[11]]) as usize;
        let flags = u16::from_le_bytes([entry[12], entry[13]]);

        if flags & 0x0002 != 0 {
            break;
        } // LAST_ENTRY

        if key_len > 0 && offset + 0x10 + key_len <= index_data.len() {
            let key = &entry[0x10..0x10 + key_len];
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

/// Search a raw index block for `name`, returning the MFT record number.
pub fn find_in_directory_with_index(index_data: &[u8], name: &str) -> Result<u64, NtfsError> {
    if index_data.len() < 16 {
        return Err(NtfsError::FileNotFound);
    }
    let first_entry =
        u32::from_le_bytes([index_data[0], index_data[1], index_data[2], index_data[3]]) as usize;
    let total_size =
        u32::from_le_bytes([index_data[4], index_data[5], index_data[6], index_data[7]]) as usize;

    let mut offset = first_entry;
    while offset + 0x12 <= total_size && offset + 0x12 <= index_data.len() {
        let entry = &index_data[offset..];
        let file_ref = u64::from_le_bytes([
            entry[0], entry[1], entry[2], entry[3], entry[4], entry[5], entry[6], entry[7],
        ]);
        let entry_len = u16::from_le_bytes([entry[8], entry[9]]) as usize;
        let key_len = u16::from_le_bytes([entry[10], entry[11]]) as usize;
        let flags = u16::from_le_bytes([entry[12], entry[13]]);

        if flags & 0x0002 != 0 {
            break;
        }

        if key_len > 0 && offset + 0x10 + key_len <= index_data.len() {
            let key = &entry[0x10..0x10 + key_len];
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

/// Placeholder — insert the new entry into the parent's $INDEX_ROOT.
///
/// A full B-tree insertion is non-trivial; this stub exists so the rest of
/// the create path compiles. Until implemented, `list_directory` on the
/// parent will not show newly created children.
/// Insert a new entry into parent's $INDEX_ROOT (simple version for small
/// directories)
pub fn add_directory_entry<D: BlockDevice>(
    driver: &mut NtfsDriver<D>,
    parent: &NtfsFile,
    child_record: u64,
    name: &str,
) -> Result<(), NtfsError> {
    // Read parent's MFT record
    let mut record = driver.read_mft_record(parent.record_number)?;

    // Find $INDEX_ROOT attribute
    let index_offset = find_attribute_offset(&record, 0x90)?;
    let attr_header_len = 24;
    let index_root_start = index_offset + attr_header_len;

    // Parse index root header (at offset 24 within attribute)
    // Index root structure: header(16 bytes) + index header(16 bytes) + entries...
    let ih_offset = index_root_start + 16; // Index header starts after root header

    // Read current index header
    let first_entry_off = u32::from_le_bytes([
        record[ih_offset],
        record[ih_offset + 1],
        record[ih_offset + 2],
        record[ih_offset + 3],
    ]) as usize;
    let total_size = u32::from_le_bytes([
        record[ih_offset + 4],
        record[ih_offset + 5],
        record[ih_offset + 6],
        record[ih_offset + 7],
    ]) as usize;
    let alloc_size = u32::from_le_bytes([
        record[ih_offset + 8],
        record[ih_offset + 9],
        record[ih_offset + 10],
        record[ih_offset + 11],
    ]) as usize;

    // Build new index entry
    let name_utf16: Vec<u16> = name.encode_utf16().collect();
    let name_len = name_utf16.len();
    let key_len = 66 + name_len * 2; // $FILE_NAME header + name
    let entry_len = 16 + key_len; // Header(16) + key

    // Check if we have space (simplified - assumes resident index)
    if total_size + entry_len > alloc_size {
        return Err(NtfsError::InvalidAttribute); // Would need $INDEX_ALLOCATION
    }

    // Find where to insert (before last entry marker)
    let entries_start = ih_offset + first_entry_off;
    let last_entry_pos = ih_offset + total_size - 16; // Last entry is 16 bytes

    // Shift existing entries down to make room (if any)
    let shift_start = last_entry_pos;
    let shift_end = ih_offset + total_size;
    let shift_size = shift_end - shift_start;

    if shift_size > 0 {
        record.copy_within(shift_start..shift_end, shift_start + entry_len);
    }

    // Write new entry at last_entry_pos
    let entry = &mut record[last_entry_pos..last_entry_pos + entry_len];

    // File reference (MFT record number + sequence number)
    entry[0..8].copy_from_slice(&child_record.to_le_bytes());
    // Entry length
    entry[8..10].copy_from_slice(&(entry_len as u16).to_le_bytes());
    // Key length
    entry[10..12].copy_from_slice(&(key_len as u16).to_le_bytes());
    // Flags: 0 (not last, not subnode)
    entry[12..14].copy_from_slice(&0u16.to_le_bytes());
    // Padding
    entry[14..16].copy_from_slice(&0u16.to_le_bytes());

    // Write $FILE_NAME key
    let key = &mut entry[16..16 + key_len];
    // Parent directory reference (points to parent itself)
    key[0..8].copy_from_slice(&parent.record_number.to_le_bytes());
    // Timestamps (zeros)
    key[8..40].fill(0);
    // Allocated size
    key[40..48].copy_from_slice(&0u64.to_le_bytes());
    // Real size
    key[48..56].copy_from_slice(&0u64.to_le_bytes());
    // Flags
    key[56..60].copy_from_slice(&0u32.to_le_bytes());
    // Reparse
    key[60..64].copy_from_slice(&0u32.to_le_bytes());
    // Name length
    key[64] = name_len as u8;
    // Name type (0 = POSIX)
    key[65] = 0;
    // UTF-16 name
    for (i, &c) in name_utf16.iter().enumerate() {
        key[66 + i * 2..66 + i * 2 + 2].copy_from_slice(&c.to_le_bytes());
    }

    // Update index header: increase total size
    let new_total = (total_size + entry_len) as u32;
    record[ih_offset + 4..ih_offset + 8].copy_from_slice(&new_total.to_le_bytes());

    // Rewrite fixups and write record back
    driver.reapply_fixups(&mut record)?;
    let mft_offset =
        driver.boot.mft_byte_offset() + parent.record_number * driver.mft_record_size as u64;
    driver.device.write_at(mft_offset, &record).map_err(|_| NtfsError::IoError)?;

    Ok(())
}

fn find_attribute_offset(record: &[u8], attr_type: u32) -> Result<usize, NtfsError> {
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
            return Err(NtfsError::InvalidAttribute);
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

        if attr_len == 0 {
            break;
        }
        offset += attr_len;
    }

    Err(NtfsError::InvalidAttribute)
}

/// List all filenames in a directory index.
pub fn list(index_data: &[u8]) -> Result<Vec<String>, NtfsError> {
    let mut result = Vec::new();
    if index_data.len() < 16 {
        return Ok(result);
    }
    let first_entry =
        u32::from_le_bytes([index_data[0], index_data[1], index_data[2], index_data[3]]) as usize;
    let total_size =
        u32::from_le_bytes([index_data[4], index_data[5], index_data[6], index_data[7]]) as usize;

    let mut offset = first_entry;
    while offset + 0x12 <= total_size && offset + 0x12 <= index_data.len() {
        let entry = &index_data[offset..];
        let entry_len = u16::from_le_bytes([entry[8], entry[9]]) as usize;
        let key_len = u16::from_le_bytes([entry[10], entry[11]]) as usize;
        let flags = u16::from_le_bytes([entry[12], entry[13]]);

        if flags & 0x0002 != 0 {
            break;
        } // LAST_ENTRY

        if key_len > 0 && offset + 0x10 + key_len <= index_data.len() {
            let key = &entry[0x10..0x10 + key_len];
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

/// Find an entry in a directory index by name, returning its MFT record number.
pub fn find(index_data: &[u8], name: &str) -> Result<u64, NtfsError> {
    if index_data.len() < 16 {
        return Err(NtfsError::FileNotFound);
    }
    let first_entry =
        u32::from_le_bytes([index_data[0], index_data[1], index_data[2], index_data[3]]) as usize;
    let total_size =
        u32::from_le_bytes([index_data[4], index_data[5], index_data[6], index_data[7]]) as usize;

    let mut offset = first_entry;
    while offset + 0x12 <= total_size && offset + 0x12 <= index_data.len() {
        let entry = &index_data[offset..];
        let file_ref = u64::from_le_bytes([
            entry[0], entry[1], entry[2], entry[3], entry[4], entry[5], entry[6], entry[7],
        ]);
        let entry_len = u16::from_le_bytes([entry[8], entry[9]]) as usize;
        let key_len = u16::from_le_bytes([entry[10], entry[11]]) as usize;
        let flags = u16::from_le_bytes([entry[12], entry[13]]);

        if flags & 0x0002 != 0 {
            break;
        }

        if key_len > 0 && offset + 0x10 + key_len <= index_data.len() {
            let key = &entry[0x10..0x10 + key_len];
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
