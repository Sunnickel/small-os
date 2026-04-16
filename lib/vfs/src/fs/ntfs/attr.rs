use alloc::{format, string::String, vec::Vec};

use crate::{
    fs::ntfs::{
        boot::BootSector,
        error::NtfsError,
        runs::parse_data_runs,
        types::{
            AlternateDataStream,
            AttributeType,
            DataRun,
            NtfsStat,
            ObjectId,
            ReparsePoint,
            SecurityDescriptor,
            StandardInformation,
        },
    },
};
pub(crate) fn parse_filename(attr_data: &[u8]) -> Option<(String, u64)> {
    let header_len = 24;
    parse_name_and_parent(&attr_data[header_len..], 0x42)
}

pub(crate) fn parse_filename_from_key(key: &[u8]) -> Option<(String, u64)> {
    parse_name_and_parent(key, 0x42)
}

/// Yields `(AttributeType, raw_attr_slice, is_resident)` for every
/// attribute in the record until the end marker or a malformed entry.
pub(crate) fn parse_attributes(
    record: &[u8],
) -> impl Iterator<Item = (AttributeType, &[u8], bool)> {
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
pub(crate) fn find_data_attribute_offset(record: &[u8]) -> Result<usize, NtfsError> {
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
pub(crate) fn update_resident_data(
    record: &mut [u8],
    attr_offset: usize,
    data: &[u8],
) -> Result<(), NtfsError> {
    if attr_offset + 24 > record.len() {
        return Err(NtfsError::InvalidAttribute);
    }

    let value_offset =
        u16::from_le_bytes([record[attr_offset + 20], record[attr_offset + 21]]) as usize;
    let value_length = u32::from_le_bytes([
        record[attr_offset + 16],
        record[attr_offset + 17],
        record[attr_offset + 18],
        record[attr_offset + 19],
    ]) as usize;

    let start = attr_offset + value_offset;
    let end = start + value_length;

    if data.len() != value_length || end > record.len() {
        return Err(NtfsError::InvalidAttribute);
    }

    record[start..end].copy_from_slice(data);
    Ok(())
}

/// Parse all relevant attributes from the MFT record and return them.
///
/// This is the single source of truth for every piece of file metadata.
pub(crate) fn parse_stat(record: &[u8], boot: &BootSector) -> Result<NtfsStat, NtfsError> {
    let mut stat = NtfsStat {
        is_directory: false,
        size: 0,
        name: None,
        data_runs: Vec::new(),
        index_root: None,
        standard_info: None,
        security_descriptor: None,
        object_id: None,
        reparse_point: None,
        alternate_data_streams: Vec::new(),
    };

    for (attr_type, attr_data, is_resident) in parse_attributes(record) {
        match attr_type {
            AttributeType::StandardInformation => {
                if is_resident {
                    stat.standard_info = StandardInformation::parse(attr_data);
                }
            }

            AttributeType::ObjectId => {
                if is_resident {
                    stat.object_id = ObjectId::parse(attr_data);
                }
            }

            AttributeType::SecurityDescriptor => {
                stat.security_descriptor = SecurityDescriptor::parse(attr_data, is_resident);
            }

            AttributeType::ReparsePoint => {
                if is_resident {
                    stat.reparse_point = ReparsePoint::parse(attr_data);
                }
            }

            AttributeType::FileName => {
                if is_resident && stat.name.is_none() {
                    if let Some((name, _)) = parse_filename(attr_data) {
                        stat.name = Some(name);
                    }
                }
            }

            AttributeType::IndexRoot => {
                if is_resident {
                    stat.is_directory = true;
                    let val_off =
                        u16::from_le_bytes(attr_data[20..22].try_into().unwrap()) as usize;
                    if attr_data.len() > val_off {
                        stat.index_root = Some(attr_data[val_off..].to_vec());
                    }
                }
            }

            AttributeType::Data => {
                // name_length is a u8 at offset 9 — nonzero means named/ADS stream
                let name_length = attr_data[9] as usize;

                if is_resident {
                    if attr_data.len() < 24 {
                        continue;
                    }
                    let value_length =
                        u32::from_le_bytes(attr_data[16..20].try_into().unwrap()) as usize;
                    let value_offset =
                        u16::from_le_bytes(attr_data[20..22].try_into().unwrap()) as usize;

                    if value_offset + value_length > attr_data.len() {
                        continue;
                    }

                    let data = attr_data[value_offset..value_offset + value_length].to_vec();

                    if name_length == 0 {
                        // Primary unnamed $DATA stream
                        stat.size = value_length as u64;
                        stat.data_runs.push(DataRun::Resident { data });
                    } else {
                        // Named alternate data stream
                        let name = parse_attr_name(attr_data, name_length);
                        stat.alternate_data_streams.push(AlternateDataStream {
                            name,
                            size: value_length as u64,
                            data: DataRun::Resident { data },
                        });
                    }
                } else {
                    let runs = parse_data_runs(attr_data)?;
                    let size = runs.iter().map(|(_, len)| len * boot.bytes_per_cluster()).sum();

                    if name_length == 0 {
                        stat.size = size;
                        stat.data_runs.push(DataRun::NonResident(runs));
                    } else {
                        let name = parse_attr_name(attr_data, name_length);
                        stat.alternate_data_streams.push(AlternateDataStream {
                            name,
                            size,
                            data: DataRun::NonResident(runs),
                        });
                    }
                }
            }

            _ => {}
        }
    }

    Ok(stat)
}

/// Apply fixups to an MFT record after reading from disk.
/// This restores the original bytes at sector ends using the update sequence
/// array.
pub(crate) fn apply_fixups(buf: &mut [u8], sector_size: usize) -> Result<(), NtfsError> {
    let uso = u16::from_le_bytes([buf[4], buf[5]]) as usize;
    let usc = u16::from_le_bytes([buf[6], buf[7]]) as usize;

    // Validate bounds before accessing
    if uso < 48 || uso + 2 > buf.len() {
        return Ok(());
    }
    if usc == 0 {
        return Ok(());
    }

    // Check that update sequence array fits within buffer
    if uso + 2 * usc > buf.len() {
        return Err(NtfsError::CorruptedFilesystem);
    }

    let seq_number = u16::from_le_bytes([buf[uso], buf[uso + 1]]);

    for i in 1..usc {
        let end = i * sector_size - 2;
        if end + 2 > buf.len() {
            return Err(NtfsError::CorruptedFilesystem);
        }

        // Read the stored original value from update sequence array
        let stored = u16::from_le_bytes([buf[uso + i * 2], buf[uso + i * 2 + 1]]);
        // Read the current value at sector end (should be seq_number)
        let bytes = u16::from_le_bytes([buf[end], buf[end + 1]]);

        if bytes != seq_number {
            return Err(NtfsError::CorruptedFilesystem);
        }

        // Restore original bytes
        buf[end..end + 2].copy_from_slice(&stored.to_le_bytes());
    }

    Ok(())
}

/// Re-apply fixups before writing (complement to apply_fixups).
/// This prepares the MFT record for writing to disk by:
/// 1. Storing the sequence number at the end of each sector
/// 2. Saving the original bytes that were there into the update sequence array
///
/// CRITICAL: Only call this ONCE per buffer, and only on buffers that have
/// already had fixups applied (i.e., after read + modify operations).
pub(crate) fn reapply_fixups(buf: &mut [u8], sector_size: usize) -> Result<(), NtfsError> {
    let uso = u16::from_le_bytes([buf[4], buf[5]]) as usize;
    let usc = u16::from_le_bytes([buf[6], buf[7]]) as usize;

    if uso < 48 || usc == 0 {
        return Ok(());
    }

    if uso + 2 * usc > buf.len() {
        return Err(NtfsError::CorruptedFilesystem);
    }

    let seq_number = u16::from_le_bytes([buf[uso], buf[uso + 1]]);

    for i in 1..usc {
        let end = i * sector_size - 2;
        if end + 2 > buf.len() {
            return Err(NtfsError::CorruptedFilesystem);
        }

        let original = u16::from_le_bytes([buf[end], buf[end + 1]]);
        buf[uso + i * 2..uso + i * 2 + 2].copy_from_slice(&original.to_le_bytes());
        buf[end..end + 2].copy_from_slice(&seq_number.to_le_bytes());
    }

    Ok(())
}

/// Check if fixups have been applied by looking at sector end signatures.
/// Returns true if the sector ends contain the sequence number (fixups NOT
/// applied), false if they contain original data (fixups already applied).
pub(crate) fn fixups_applied(buf: &[u8], sector_size: usize) -> Result<bool, NtfsError> {
    let uso = u16::from_le_bytes([buf[4], buf[5]]) as usize;
    let usc = u16::from_le_bytes([buf[6], buf[7]]) as usize;

    if uso < 48 || usc == 0 || uso + 2 > buf.len() {
        return Ok(false);
    }

    let seq_number = u16::from_le_bytes([buf[uso], buf[uso + 1]]);

    // Check first sector end (if there's more than one sector)
    if usc > 1 {
        let end = sector_size - 2;
        if end + 2 <= buf.len() {
            let bytes = u16::from_le_bytes([buf[end], buf[end + 1]]);
            // If sector end equals seq_number, fixups haven't been applied yet
            return Ok(bytes != seq_number);
        }
    }

    Ok(false)
}

/// Read a UTF-16LE attribute name from the attribute header.
/// name_offset is at attr_data[10..12], name_length is in UTF-16 units.
fn parse_attr_name(attr_data: &[u8], name_length: usize) -> String {
    if attr_data.len() < 12 {
        return String::new();
    }
    let name_offset = u16::from_le_bytes([attr_data[10], attr_data[11]]) as usize;
    let byte_len = name_length * 2;
    let Some(raw) = attr_data.get(name_offset..name_offset + byte_len) else {
        return String::new();
    };
    raw.chunks_exact(2)
        .map(|c| {
            let u = u16::from_le_bytes([c[0], c[1]]);
            if u < 128 { u as u8 as char } else { '?' }
        })
        .collect()
}

fn parse_name_and_parent(data: &[u8], name_offset: usize) -> Option<(String, u64)> {
    if data.len() < name_offset + 2 {
        return None;
    }
    let parent = u64::from_le_bytes(data[0..8].try_into().unwrap());
    let name_len = data[0x40] as usize;
    let raw = &data[name_offset..];
    if raw.len() < name_len * 2 {
        return None;
    }

    let mut name = String::with_capacity(name_len);
    for i in 0..name_len {
        let c = u16::from_le_bytes([raw[i * 2], raw[i * 2 + 1]]);
        if c == 0 {
            break;
        }
        name.push(if c < 128 { c as u8 as char } else { '?' });
    }

    Some((name, parent))
}
