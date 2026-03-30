use alloc::vec::Vec;

use hal::block::BlockDevice;

use crate::{
    attr::{find_data_attribute_offset, reapply_fixups, update_resident_data},
    CreateOptions,
    DataRun,
    NtfsDriver,
    NtfsError,
    NtfsFile,
};

/// Overwrite the content of a resident file in-place.
///
/// Restrictions:
/// - File must be resident (≲ ~700 bytes).
/// - `data.len()` must equal the current on-disk value length (no resize).
pub fn write_file<D: BlockDevice>(
    driver: &mut NtfsDriver<D>,
    file: &NtfsFile,
    data: &[u8],
) -> Result<(), NtfsError> {
    let stat = driver.stat(file)?;
    let run = stat.data_runs.first().ok_or(NtfsError::InvalidAttribute)?;
    match run {
        DataRun::Resident { data: existing } => {
            if data.len() != existing.len() {
                return Err(NtfsError::InvalidAttribute);
            }
        }
        DataRun::NonResident(_) => {
            return Err(NtfsError::InvalidAttribute);
        }
    }

    let mut record = driver.read_mft_record(file.record_number)?;
    let data_attr_offset = find_data_attribute_offset(&record)?;
    update_resident_data(&mut record, data_attr_offset, data)?;
    reapply_fixups(&mut record, driver.boot.bytes_per_sector as usize)?;

    let mft_offset =
        driver.boot.mft_byte_offset() + file.record_number * driver.mft_record_size as u64;
    driver.device.write_at(mft_offset, &record).map_err(|_| NtfsError::IoError)?;
    Ok(())
}

/// Create a new file or directory inside `parent`.
///
/// Returns a live handle to the newly created entry.
pub fn create_file<D: BlockDevice>(
    driver: &mut NtfsDriver<D>,
    parent: &NtfsFile,
    name: &str,
    options: CreateOptions,
) -> Result<NtfsFile, NtfsError> {
    if !driver.is_directory(parent)? {
        return Err(NtfsError::NotADirectory);
    }
    if driver.find_in_directory(parent, name).is_ok() {
        return Err(NtfsError::FileNotFound);
    }

    let new_record_number = allocate_mft_record(driver)?;
    let mut record = create_mft_record(
        driver,
        new_record_number,
        name,
        parent.record_number,
        options.is_directory,
        &options.data,
    )?;

    reapply_fixups(&mut record, driver.boot.bytes_per_sector as usize)?;

    let mft_offset =
        driver.boot.mft_byte_offset() + new_record_number * driver.mft_record_size as u64;
    driver.device.write_at(mft_offset, &record).map_err(|_| NtfsError::IoError)?;

    let _ = crate::index::add_directory_entry(driver, parent, new_record_number, name);

    driver.open_file(new_record_number)
}

// -----------------------------------------------------------------------
// Private helpers
// -----------------------------------------------------------------------

fn allocate_mft_record<D: BlockDevice>(driver: &mut NtfsDriver<D>) -> Result<u64, NtfsError> {
    for num in 16u64..1000 {
        let offset = driver.boot.mft_byte_offset() + num * driver.mft_record_size as u64;
        let mut buf = alloc::vec![0u8; driver.mft_record_size];
        if driver.device.read_at(offset, &mut buf).is_ok() {
            if &buf[0..4] != b"FILE" && &buf[0..4] != b"BAAD" {
                return Ok(num);
            }
        }
    }
    Err(NtfsError::IoError)
}

fn create_mft_record<D: BlockDevice>(
    driver: &NtfsDriver<D>,
    record_number: u64,
    name: &str,
    parent_record: u64,
    is_directory: bool,
    data: &[u8],
) -> Result<Vec<u8>, NtfsError> {
    let mut record = alloc::vec![0u8; driver.mft_record_size];

    record[0..4].copy_from_slice(b"FILE");
    record[4..6].copy_from_slice(&48u16.to_le_bytes());
    record[6..8].copy_from_slice(&3u16.to_le_bytes());
    record[8..16].copy_from_slice(&0u64.to_le_bytes());
    record[16..18].copy_from_slice(&(record_number as u16).to_le_bytes());
    record[18..20].copy_from_slice(&1u16.to_le_bytes());
    record[20..22].copy_from_slice(&56u16.to_le_bytes());
    record[22..24].copy_from_slice(&1u16.to_le_bytes());
    record[24..28].copy_from_slice(&(driver.mft_record_size as u32).to_le_bytes());
    record[28..32].copy_from_slice(&(driver.mft_record_size as u32).to_le_bytes());

    let seq = (record_number & 0xFFFF) as u16;
    record[48..50].copy_from_slice(&seq.to_le_bytes());

    let mut off = 56usize;
    off = write_standard_info_attr(&mut record, off, is_directory)?;
    off = write_filename_attr(&mut record, off, name, parent_record)?;
    if is_directory {
        off = write_index_root_attr(driver, &mut record, off)?;
    } else {
        off = write_data_attr(&mut record, off, data)?;
    }

    record[off..off + 4].copy_from_slice(&0xFFFF_FFFFu32.to_le_bytes());

    Ok(record)
}

fn write_standard_info_attr(
    record: &mut [u8],
    offset: usize,
    is_directory: bool,
) -> Result<usize, NtfsError> {
    let data_len: u32 = 56;
    let attr_len: u32 = 24 + data_len;

    record[offset..offset + 4].copy_from_slice(&0x10u32.to_le_bytes());
    record[offset + 4..offset + 8].copy_from_slice(&attr_len.to_le_bytes());
    record[offset + 8] = 0;
    record[offset + 9] = 0;
    record[offset + 10..offset + 12].copy_from_slice(&0u16.to_le_bytes());
    record[offset + 12..offset + 14].copy_from_slice(&0u16.to_le_bytes());
    record[offset + 14..offset + 16].copy_from_slice(&0u16.to_le_bytes());
    record[offset + 16..offset + 20].copy_from_slice(&data_len.to_le_bytes());
    record[offset + 20..offset + 22].copy_from_slice(&24u16.to_le_bytes());
    record[offset + 22] = 0;
    record[offset + 23] = 0;

    let d = offset + 24;
    let now = 0u64;
    let flags: u32 = if is_directory { 0x10 } else { 0x20 };
    record[d..d + 8].copy_from_slice(&now.to_le_bytes());
    record[d + 8..d + 16].copy_from_slice(&now.to_le_bytes());
    record[d + 16..d + 24].copy_from_slice(&now.to_le_bytes());
    record[d + 24..d + 32].copy_from_slice(&now.to_le_bytes());
    record[d + 32..d + 36].copy_from_slice(&flags.to_le_bytes());

    Ok(offset + attr_len as usize)
}

fn write_filename_attr(
    record: &mut [u8],
    offset: usize,
    name: &str,
    parent_record: u64,
) -> Result<usize, NtfsError> {
    let utf16: Vec<u16> = name.encode_utf16().collect();
    let name_len = utf16.len();
    let data_len = 66 + name_len * 2;
    let attr_len = (24 + data_len + 7) & !7;

    record[offset..offset + 4].copy_from_slice(&0x30u32.to_le_bytes());
    record[offset + 4..offset + 8].copy_from_slice(&(attr_len as u32).to_le_bytes());
    record[offset + 8] = 0;
    record[offset + 9] = 0;
    record[offset + 10..offset + 12].copy_from_slice(&0u16.to_le_bytes());
    record[offset + 12..offset + 14].copy_from_slice(&0u16.to_le_bytes());
    record[offset + 14..offset + 16].copy_from_slice(&0u16.to_le_bytes());
    record[offset + 16..offset + 20].copy_from_slice(&(data_len as u32).to_le_bytes());
    record[offset + 20..offset + 22].copy_from_slice(&24u16.to_le_bytes());
    record[offset + 22] = 0;
    record[offset + 23] = 0;

    let d = offset + 24;
    record[d..d + 8].copy_from_slice(&parent_record.to_le_bytes());
    record[d + 64] = name_len as u8;
    record[d + 65] = 0;

    for (i, &c) in utf16.iter().enumerate() {
        let pos = d + 66 + i * 2;
        record[pos..pos + 2].copy_from_slice(&c.to_le_bytes());
    }

    Ok(offset + attr_len)
}

fn write_data_attr(record: &mut [u8], offset: usize, data: &[u8]) -> Result<usize, NtfsError> {
    if data.len() > 700 {
        return Err(NtfsError::InvalidAttribute);
    }

    let data_len = data.len();
    let attr_len = (24 + data_len + 7) & !7;

    record[offset..offset + 4].copy_from_slice(&0x80u32.to_le_bytes());
    record[offset + 4..offset + 8].copy_from_slice(&(attr_len as u32).to_le_bytes());
    record[offset + 8] = 0;
    record[offset + 9] = 0;
    record[offset + 10..offset + 12].copy_from_slice(&0u16.to_le_bytes());
    record[offset + 12..offset + 14].copy_from_slice(&0u16.to_le_bytes());
    record[offset + 14..offset + 16].copy_from_slice(&0u16.to_le_bytes());
    record[offset + 16..offset + 20].copy_from_slice(&(data_len as u32).to_le_bytes());
    record[offset + 20..offset + 22].copy_from_slice(&24u16.to_le_bytes());
    record[offset + 22] = 0;
    record[offset + 23] = 0;

    record[offset + 24..offset + 24 + data_len].copy_from_slice(data);
    Ok(offset + attr_len)
}

fn write_index_root_attr<D: BlockDevice>(
    driver: &NtfsDriver<D>,
    record: &mut [u8],
    offset: usize,
) -> Result<usize, NtfsError> {
    let index_root_header_len = 16usize;
    let index_header_len = 16usize;
    let end_entry_len = 16usize;

    let data_len = index_root_header_len + index_header_len + end_entry_len;
    let attr_len = 24 + data_len;

    record[offset..offset + 4].copy_from_slice(&0x90u32.to_le_bytes());
    record[offset + 4..offset + 8].copy_from_slice(&(attr_len as u32).to_le_bytes());
    record[offset + 8] = 0;
    record[offset + 9] = 0;
    record[offset + 10..offset + 12].copy_from_slice(&0u16.to_le_bytes());
    record[offset + 12..offset + 14].copy_from_slice(&0u16.to_le_bytes());
    record[offset + 14..offset + 16].copy_from_slice(&0u16.to_le_bytes());
    record[offset + 16..offset + 20].copy_from_slice(&(data_len as u32).to_le_bytes());
    record[offset + 20..offset + 22].copy_from_slice(&24u16.to_le_bytes());
    record[offset + 22] = 0;
    record[offset + 23] = 0;

    let d = offset + 24;
    record[d..d + 4].copy_from_slice(&0x30u32.to_le_bytes());
    record[d + 4..d + 8].copy_from_slice(&0x01u32.to_le_bytes());
    record[d + 8..d + 12].copy_from_slice(&(driver.mft_record_size as u32).to_le_bytes());
    record[d + 12] = driver.boot.clusters_per_index_buffer as u8;
    record[d + 13..d + 16].fill(0);

    let ih = d + 16;
    let first_entry_offset = (index_root_header_len + index_header_len) as u32;
    let total_size = (first_entry_offset as usize + end_entry_len) as u32;

    record[ih..ih + 4].copy_from_slice(&first_entry_offset.to_le_bytes());
    record[ih + 4..ih + 8].copy_from_slice(&total_size.to_le_bytes());
    record[ih + 8..ih + 12].copy_from_slice(&total_size.to_le_bytes());
    record[ih + 12..ih + 16].fill(0);

    let le = d + 32;
    record[le..le + 8].copy_from_slice(&0u64.to_le_bytes());
    record[le + 8..le + 10].copy_from_slice(&16u16.to_le_bytes());
    record[le + 10..le + 12].copy_from_slice(&0u16.to_le_bytes());
    record[le + 12..le + 14].copy_from_slice(&0x0002u16.to_le_bytes());

    Ok(offset + attr_len)
}
