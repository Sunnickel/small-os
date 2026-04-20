use alloc::{vec, vec::Vec};

use hal::block::BlockDevice;

use crate::partition::gpt::{
    GPT_ENTRY_SIZE,
    GPT_HEADER_SIZE,
    GPT_MAX_ENTRIES,
    GPT_REVISION,
    GPT_SIGNATURE,
    crc32,
    error::GptError,
    serialize_entries,
    structs::{GptEntry, GptHeader},
};

/// Read and validate GPT from device (tries primary, then backup)
pub(crate) fn read_gpt(dev: &mut (dyn BlockDevice)) -> Result<(GptHeader, Vec<GptEntry>), GptError> {
    let sector = dev.block_size() as u64;

    // Try primary header at LBA 1
    let result = read_header_raw(dev, 1, sector).and_then(|raw| parse_and_verify_header(&raw).ok());

    // Fall back to backup header if primary fails
    let header_raw = match result {
        Some((raw, _)) => raw,
        None => {
            let last_lba = dev.block_count().checked_sub(1).ok_or(GptError::InvalidSignature)?;
            let raw = read_header_raw(dev, last_lba, sector).ok_or(GptError::InvalidSignature)?;
            parse_and_verify_header(&raw).map(|(r, _)| r)?
        }
    };

    let header = parse_header(&header_raw)?;
    validate_header(&header)?;

    let entries = read_entries(dev, sector, &header)?;
    verify_entries_crc(&entries, header.partition_array_crc32)?;

    Ok((header, entries))
}

/// Parse header and verify CRC in one step
fn parse_and_verify_header(raw: &[u8]) -> Result<(Vec<u8>, GptHeader), GptError> {
    if raw.len() < GPT_HEADER_SIZE as usize {
        return Err(GptError::InvalidHeaderSize);
    }

    let header = parse_header(raw)?;
    validate_header(&header)?;

    // Verify CRC over header_size bytes (field at offset 16 zeroed)
    let stored_crc = header.header_crc32;
    let mut crc_buf = raw[..header.header_size as usize].to_vec();
    crc_buf[16..20].copy_from_slice(&[0u8; 4]);

    if crc32(&crc_buf) != stored_crc {
        return Err(GptError::InvalidHeaderCrc);
    }

    Ok((raw.to_vec(), header))
}

fn validate_header(header: &GptHeader) -> Result<(), GptError> {
    if header.signature != *GPT_SIGNATURE {
        return Err(GptError::InvalidSignature);
    }
    if header.revision != GPT_REVISION {
        // Allow newer revisions but warn? For now strict check
        // log::warn!("Unknown GPT revision: {:02x?}", header.revision);
    }
    if header.header_size != GPT_HEADER_SIZE {
        return Err(GptError::InvalidHeaderSize);
    }
    if header.partition_entry_size != GPT_ENTRY_SIZE {
        return Err(GptError::InvalidEntrySize);
    }
    Ok(())
}

fn read_header_raw(dev: &mut (dyn  BlockDevice), lba: u64, sector: u64) -> Option<Vec<u8>> {
    if sector as usize > 4096 {
        // Sanity check sector size
        return None;
    }

    let mut buf = vec![0u8; sector as usize];
    dev.read_blocks(lba.checked_mul(sector)?, &mut buf).ok()?;

    if buf.get(0..8)? == GPT_SIGNATURE { Some(buf) } else { None }
}

fn parse_header(raw: &[u8]) -> Result<GptHeader, GptError> {
    if raw.len() < GPT_HEADER_SIZE as usize {
        return Err(GptError::InvalidHeaderSize);
    }

    Ok(GptHeader {
        signature: raw[0..8].try_into().map_err(|_| GptError::InvalidSignature)?,
        revision: raw[8..12].try_into().map_err(|_| GptError::InvalidHeaderSize)?,
        header_size: u32::from_le_bytes(raw[12..16].try_into().unwrap()),
        header_crc32: u32::from_le_bytes(raw[16..20].try_into().unwrap()),
        reserved: u32::from_le_bytes(raw[20..24].try_into().unwrap()),
        my_lba: u64::from_le_bytes(raw[24..32].try_into().unwrap()),
        alternate_lba: u64::from_le_bytes(raw[32..40].try_into().unwrap()),
        first_usable_lba: u64::from_le_bytes(raw[40..48].try_into().unwrap()),
        last_usable_lba: u64::from_le_bytes(raw[48..56].try_into().unwrap()),
        disk_guid: raw[56..72].try_into().map_err(|_| GptError::InvalidHeaderSize)?,
        partition_entry_lba: u64::from_le_bytes(raw[72..80].try_into().unwrap()),
        num_partition_entries: u32::from_le_bytes(raw[80..84].try_into().unwrap()),
        partition_entry_size: u32::from_le_bytes(raw[84..88].try_into().unwrap()),
        partition_array_crc32: u32::from_le_bytes(raw[88..92].try_into().unwrap()),
    })
}

fn read_entries(
    dev: &mut dyn  BlockDevice,
    sector: u64,
    h: &GptHeader,
) -> Result<Vec<GptEntry>, GptError> {
    if h.partition_entry_size != GPT_ENTRY_SIZE {
        return Err(GptError::InvalidEntrySize);
    }

    // Prevent overflow and excessive allocation
    if h.num_partition_entries > GPT_MAX_ENTRIES * 4 {
        // Allow up to 512 entries max
        return Err(GptError::InvalidHeaderSize);
    }

    let total_bytes = (h.num_partition_entries as u64)
        .checked_mul(h.partition_entry_size as u64)
        .ok_or(GptError::Overflow)?;

    let total_usize = total_bytes.try_into().map_err(|_| GptError::Overflow)?;

    let mut buf = vec![0u8; total_usize];
    let offset = h.partition_entry_lba.checked_mul(sector).ok_or(GptError::Overflow)?;
    dev.read_blocks(offset, &mut buf)?;

    let entries: Result<Vec<_>, _> =
        buf.chunks_exact(GPT_ENTRY_SIZE as usize).map(|e| parse_gpt_entry(e)).collect();

    entries
}

fn parse_gpt_entry(e: &[u8]) -> Result<GptEntry, GptError> {
    if e.len() < 128 {
        return Err(GptError::InvalidEntrySize);
    }

    Ok(GptEntry {
        type_guid: e[0..16].try_into().map_err(|_| GptError::InvalidHeaderSize)?,
        unique_guid: e[16..32].try_into().map_err(|_| GptError::InvalidHeaderSize)?,
        start_lba: u64::from_le_bytes(e[32..40].try_into().unwrap()),
        end_lba: u64::from_le_bytes(e[40..48].try_into().unwrap()),
        attributes: u64::from_le_bytes(e[48..56].try_into().unwrap()),
        name: parse_utf16le(&e[56..128]),
    })
}

fn parse_utf16le(data: &[u8]) -> [u16; 36] {
    let mut name = [0u16; 36];
    let units = data.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).take(36);

    for (i, u) in units.enumerate() {
        name[i] = u;
    }
    name
}

fn verify_entries_crc(entries: &[GptEntry], stored: u32) -> Result<(), GptError> {
    let raw = serialize_entries(entries);
    if crc32(&raw) == stored { Ok(()) } else { Err(GptError::InvalidEntriesCrc) }
}
