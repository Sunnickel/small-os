use alloc::{string::String, vec::Vec};

use hal::block::BlockDevice;

use crate::{attr::parse_filename_from_key, NtfsDriver, NtfsError, NtfsFile};

pub fn find_in_directory_with_index(index_data: &[u8], name: &str) -> Result<u64, NtfsError> {
    find(index_data, name)
}

/// Insert a new entry into parent's $INDEX_ROOT (simple version for small
/// directories)
pub fn add_directory_entry<D: BlockDevice>(
    driver: &mut NtfsDriver<D>,
    parent: &NtfsFile,
    child_record: u64,
    child_name: &str,
) -> Result<(), NtfsError> {
    let mut record = driver.read_mft_record(parent.record_number)?;
    let mft_record_size = driver.mft_record_size as usize;

    let index_attr_offset = find_attribute_offset(&record, 0x90)?;
    let is_resident = record[index_attr_offset + 8] == 0;
    if !is_resident {
        return Err(NtfsError::InvalidAttribute);
    }

    let data_offset =
        u16::from_le_bytes([record[index_attr_offset + 20], record[index_attr_offset + 21]])
            as usize;
    let ih_offset = index_attr_offset + data_offset;

    let first_entry_rel =
        u32::from_le_bytes(record[ih_offset..ih_offset + 4].try_into().unwrap()) as usize;
    let total_size =
        u32::from_le_bytes(record[ih_offset + 4..ih_offset + 8].try_into().unwrap()) as usize;

    let entries_end = ih_offset + total_size;
    let end_marker_pos = entries_end.checked_sub(16).ok_or(NtfsError::InvalidAttribute)?;

    let name_utf16: Vec<u16> = child_name.encode_utf16().collect();
    let name_len = name_utf16.len();
    let key_len = 66 + name_len * 2;
    let entry_len = 16 + key_len;
    let entry_len_aligned = (entry_len + 7) & !7;

    if entries_end + entry_len_aligned > mft_record_size {
        return Err(NtfsError::NoSpace);
    }

    record.copy_within(end_marker_pos..entries_end, end_marker_pos + entry_len_aligned);

    let entry_pos = end_marker_pos;
    let entry = &mut record[entry_pos..entry_pos + entry_len_aligned];
    entry.fill(0);

    entry[0..8].copy_from_slice(&child_record.to_le_bytes());
    entry[8..10].copy_from_slice(&(entry_len as u16).to_le_bytes());
    entry[10..12].copy_from_slice(&(key_len as u16).to_le_bytes());
    entry[12..14].copy_from_slice(&0u16.to_le_bytes());
    entry[14..16].copy_from_slice(&0u16.to_le_bytes());

    let key = &mut entry[16..16 + key_len];
    key[0..8].copy_from_slice(&child_record.to_le_bytes());
    key[8..40].fill(0);
    key[40..48].copy_from_slice(&0u64.to_le_bytes());
    key[48..56].copy_from_slice(&0u64.to_le_bytes());
    key[56..60].copy_from_slice(&0u32.to_le_bytes());
    key[60..64].copy_from_slice(&0u32.to_le_bytes());
    key[64] = name_len as u8;
    key[65] = 1;

    for (i, &c) in name_utf16.iter().enumerate() {
        key[66 + i * 2..66 + i * 2 + 2].copy_from_slice(&c.to_le_bytes());
    }

    let new_total = (total_size + entry_len_aligned) as u32;
    record[ih_offset + 4..ih_offset + 8].copy_from_slice(&new_total.to_le_bytes());

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

pub fn list(index_data: &[u8]) -> Result<Vec<String>, NtfsError> {
    let mut result = Vec::new();
    if index_data.len() < 32 {
        return Ok(result);
    }

    let index_header_offset = 16;
    let first_entry_rel = u32::from_le_bytes([
        index_data[index_header_offset],
        index_data[index_header_offset + 1],
        index_data[index_header_offset + 2],
        index_data[index_header_offset + 3],
    ]) as usize;

    let total_size = u32::from_le_bytes([
        index_data[index_header_offset + 4],
        index_data[index_header_offset + 5],
        index_data[index_header_offset + 6],
        index_data[index_header_offset + 7],
    ]) as usize;

    let mut offset = index_header_offset + first_entry_rel;
    let end = index_header_offset + total_size;

    while offset + 0x12 <= end && offset + 0x12 <= index_data.len() {
        let entry = &index_data[offset..];
        let entry_len = u16::from_le_bytes([entry[8], entry[9]]) as usize;
        let key_len = u16::from_le_bytes([entry[10], entry[11]]) as usize;
        let flags = u16::from_le_bytes([entry[12], entry[13]]);

        if flags & 0x0002 != 0 {
            break;
        }

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

pub fn find_index_root_offset(record: &[u8]) -> Result<usize, NtfsError> {
    let mut offset = 0usize;
    while offset + 4 <= record.len() {
        let attr_type = u32::from_le_bytes([
            record[offset],
            record[offset + 1],
            record[offset + 2],
            record[offset + 3],
        ]);
        if attr_type == 0x90 {
            return Ok(offset);
        }
        if offset + 8 > record.len() {
            break;
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
    Err(NtfsError::InvalidMftRecord)
}
