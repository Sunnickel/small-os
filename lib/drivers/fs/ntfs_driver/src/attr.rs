use alloc::{string::String, vec::Vec};

use crate::{boot::BootSector, AttributeType, DataRun, NtfsError, NtfsStat};

/// Parse a `$FILE_NAME` attribute (resident form) and return `(name,
/// parent_record_number)`.
pub fn parse_filename(attr_data: &[u8]) -> Option<(String, u64)> {
    // Resident attribute header is 24 bytes; $FILE_NAME data starts there.
    let header_len = 24;
    if attr_data.len() < header_len + 0x42 {
        return None;
    }
    let data = &attr_data[header_len..];
    let parent = u64::from_le_bytes([
        data[0], data[1], data[2], data[3], data[4], data[5], data[6], data[7],
    ]);
    let name_len = data[0x40] as usize;
    let name_off = 0x42;
    if name_off + name_len * 2 > data.len() {
        return None;
    }
    let raw = &data[name_off..name_off + name_len * 2];
    let mut name = String::with_capacity(name_len);
    for i in 0..name_len {
        let c = u16::from_le_bytes([raw[i * 2], raw[i * 2 + 1]]);
        if c == 0 {
            break;
        }
        if c < 128 {
            name.push(c as u8 as char);
        } else {
            name.push('?');
        }
    }
    Some((name, parent))
}

/// Parse the filename embedded in an index entry key.
pub fn parse_filename_from_key(key: &[u8]) -> Option<(String, u64)> {
    if key.len() < 0x42 {
        return None;
    }
    let parent =
        u64::from_le_bytes([key[0], key[1], key[2], key[3], key[4], key[5], key[6], key[7]]);
    let name_len = key[0x40] as usize;
    let name_off = 0x42;
    if name_off + name_len * 2 > key.len() {
        return None;
    }
    let raw = &key[name_off..name_off + name_len * 2];
    let mut name = String::with_capacity(name_len);
    for i in 0..name_len {
        let c = u16::from_le_bytes([raw[i * 2], raw[i * 2 + 1]]);
        if c == 0 {
            break;
        }
        if c < 128 {
            name.push(c as u8 as char);
        } else {
            name.push('?');
        }
    }
    Some((name, parent))
}

/// Yields `(AttributeType, raw_attr_slice, is_resident)` for every
/// attribute in the record until the end marker or a malformed entry.
pub fn parse_attributes(record: &[u8]) -> impl Iterator<Item = (AttributeType, &[u8], bool)> {
    let first = u16::from_le_bytes([record[20], record[21]]) as usize;
    let mut offset = first;

    core::iter::from_fn(move || {
        if offset + 8 > record.len() {
            return None;
        }
        let type_code = u32::from_le_bytes([
            record[offset],
            record[offset + 1],
            record[offset + 2],
            record[offset + 3],
        ]);
        if type_code == 0xFFFFFFFF {
            return None;
        }
        let record_length = u32::from_le_bytes([
            record[offset + 4],
            record[offset + 5],
            record[offset + 6],
            record[offset + 7],
        ]) as usize;
        if record_length == 0 || offset + record_length > record.len() {
            return None;
        }
        let is_resident = record[offset + 8] == 0;
        let attr_type = match type_code {
            0x10 => AttributeType::StandardInformation,
            0x20 => AttributeType::AttributeList,
            0x30 => AttributeType::FileName,
            0x40 => AttributeType::ObjectId,
            0x50 => AttributeType::SecurityDescriptor,
            0x60 => AttributeType::VolumeName,
            0x70 => AttributeType::VolumeInformation,
            0x80 => AttributeType::Data,
            0x90 => AttributeType::IndexRoot,
            0xA0 => AttributeType::IndexAllocation,
            0xB0 => AttributeType::Bitmap,
            0xC0 => AttributeType::ReparsePoint,
            0xD0 => AttributeType::EaInformation,
            0xE0 => AttributeType::Ea,
            0x100 => AttributeType::LoggedUtilityStream,
            _ => return None,
        };
        let slice = &record[offset..offset + record_length];
        offset += record_length;
        Some((attr_type, slice, is_resident))
    })
}

/// Find offset of $DATA attribute in MFT record.
pub fn find_data_attribute_offset(record: &[u8]) -> Result<usize, NtfsError> {
    let first = u16::from_le_bytes([record[20], record[21]]) as usize;
    let mut offset = first;
    while offset + 8 <= record.len() {
        let attr_type = u32::from_le_bytes([
            record[offset],
            record[offset + 1],
            record[offset + 2],
            record[offset + 3],
        ]);
        if attr_type == 0xFFFFFFFF {
            return Err(NtfsError::InvalidAttribute);
        }
        if attr_type == 0x80 {
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

/// Update resident data in place (used by `write_file`).
pub fn update_resident_data(
    record: &mut [u8],
    attr_offset: usize,
    data: &[u8],
) -> Result<(), NtfsError> {
    let value_offset =
        u16::from_le_bytes([record[attr_offset + 20], record[attr_offset + 21]]) as usize;
    let value_length = u32::from_le_bytes([
        record[attr_offset + 16],
        record[attr_offset + 17],
        record[attr_offset + 18],
        record[attr_offset + 19],
    ]) as usize;

    if data.len() != value_length {
        return Err(NtfsError::InvalidAttribute);
    }
    let start = attr_offset + value_offset;
    record[start..start + data.len()].copy_from_slice(data);
    Ok(())
}

/// Parse all relevant attributes from the MFT record and return them.
///
/// This is the single source of truth for every piece of file metadata.
pub fn parse_stat(record: &[u8], boot: &BootSector) -> Result<NtfsStat, NtfsError> {
    let mut stat = NtfsStat {
        is_directory: false,
        size: 0,
        name: None,
        data_runs: Vec::new(),
        index_root: None,
    };

    for (attr_type, attr_data, is_resident) in parse_attributes(record) {
        match attr_type {
            AttributeType::FileName => {
                if is_resident && stat.name.is_none() {
                    if let Some((name, _)) = parse_filename(attr_data) {
                        stat.name = Some(name);
                    }
                }
            }
            AttributeType::Data => {
                if is_resident {
                    let value_offset = u16::from_le_bytes([attr_data[20], attr_data[21]]) as usize;
                    let value_length = u32::from_le_bytes([
                        attr_data[16],
                        attr_data[17],
                        attr_data[18],
                        attr_data[19],
                    ]) as usize;
                    if value_offset + value_length <= attr_data.len() {
                        stat.size = value_length as u64;
                        stat.data_runs.push(DataRun::Resident {
                            data: attr_data[value_offset..value_offset + value_length].to_vec(),
                        });
                    }
                } else {
                    let runs = crate::runs::parse_data_runs(attr_data)?;
                    stat.size = runs.iter().map(|(_, len)| len * boot.bytes_per_cluster()).sum();
                    stat.data_runs.push(DataRun::NonResident(runs));
                }
            }
            AttributeType::IndexRoot => {
                if is_resident {
                    stat.is_directory = true;
                    // Skip the 24-byte resident attribute header.
                    if attr_data.len() > 24 {
                        stat.index_root = Some(attr_data[24..].to_vec());
                    }
                }
            }
            _ => {}
        }
    }
    Ok(stat)
}

/// Apply fixups to an MFT record after reading from disk.
pub fn apply_fixups(buf: &mut [u8]) -> Result<(), NtfsError> {
    let uso = u16::from_le_bytes([buf[4], buf[5]]) as usize;
    let usc = u16::from_le_bytes([buf[6], buf[7]]) as usize;

    if uso < 48 || usc == 0 {
        return Ok(());
    }

    let seq = u16::from_le_bytes([buf[uso], buf[uso + 1]]);
    let seq_lo = (seq & 0xFF) as u8;
    let seq_hi = (seq >> 8) as u8;

    for i in 1..usc {
        let end = i * 512 - 2;
        if end + 2 > buf.len() {
            break;
        }

        // Verify the fixup matches the sequence number
        if buf[end] != seq_lo || buf[end + 1] != seq_hi {
            return Err(NtfsError::CorruptedFilesystem);
        }

        // Restore original bytes from the update sequence array
        let original = u16::from_le_bytes([buf[uso + i * 2], buf[uso + i * 2 + 1]]);
        buf[end] = (original & 0xFF) as u8;
        buf[end + 1] = (original >> 8) as u8;
    }

    Ok(())
}

/// Re-apply fixups before writing (complement to apply_fixups).
/// This prepares the MFT record for writing to disk by:
/// 1. Storing the sequence number at the end of each 512-byte sector
/// 2. Saving the original bytes that were there into the update sequence array
pub fn reapply_fixups(buf: &mut [u8]) -> Result<(), NtfsError> {
    let uso = u16::from_le_bytes([buf[4], buf[5]]) as usize; // Update Sequence Offset
    let usc = u16::from_le_bytes([buf[6], buf[7]]) as usize; // Update Sequence Count (size in words)

    if uso < 48 || usc == 0 {
        return Ok(());
    }

    // Get the sequence number from the update sequence array
    let seq = u16::from_le_bytes([buf[uso], buf[uso + 1]]);

    for i in 1..usc {
        let end = i * 512 - 2;
        if end + 2 > buf.len() {
            break;
        }

        // Save the current bytes at the end of sector into update sequence array
        let original = u16::from_le_bytes([buf[end], buf[end + 1]]);
        buf[uso + i * 2] = (original & 0xFF) as u8;
        buf[uso + i * 2 + 1] = (original >> 8) as u8;

        // Write the sequence number at the end of the sector
        buf[end] = (seq & 0xFF) as u8;
        buf[end + 1] = (seq >> 8) as u8;
    }

    Ok(())
}
