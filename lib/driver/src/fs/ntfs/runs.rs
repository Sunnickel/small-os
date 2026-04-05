use alloc::vec::Vec;

use hal::block::BlockDevice;

use crate::{
    core::cluster::read_clusters,
    fs::ntfs::{boot::BootSector, error::NtfsError, types::DataRun},
};

/// Parse data runs from non-resident attribute header
/// Returns vector of (start_cluster, length_in_clusters) pairs
///
/// # Arguments
/// * `attr_data` - The non-resident attribute header (minimum 64 bytes)
pub(crate) fn parse_data_runs(attr_data: &[u8]) -> Result<Vec<(u64, u64)>, NtfsError> {
    if attr_data.len() < 64 {
        return Err(NtfsError::InvalidAttribute);
    }

    // Verify non-resident flag (offset 8)
    if attr_data[8] != 1 {
        return Err(NtfsError::InvalidAttribute);
    }

    // Data run offset is at offset 32-33 in non-resident header
    let run_offset = u16::from_le_bytes([attr_data[32], attr_data[33]]) as usize;

    if run_offset == 0 {
        return Ok(Vec::new()); // Empty attribute
    }

    if run_offset < 64 || run_offset >= attr_data.len() {
        return Err(NtfsError::InvalidAttribute);
    }

    // Validate allocated size exists
    let allocated_size = u64::from_le_bytes(attr_data[40..48].try_into().unwrap());
    if allocated_size == 0 {
        return Ok(Vec::new());
    }

    let mut runs = Vec::new();
    let mut current_offset = run_offset;
    let mut prev_cluster: i64 = 0;

    while current_offset < attr_data.len() {
        let header = attr_data[current_offset];

        // Check for terminator
        if header == 0 {
            break;
        }

        let len_size = (header & 0x0F) as usize;
        let offset_size = ((header >> 4) & 0x0F) as usize;

        // Validate sizes
        if len_size == 0 || len_size > 8 || offset_size > 8 {
            break;
        }

        if current_offset + 1 + len_size + offset_size > attr_data.len() {
            break;
        }

        // Parse run length (unsigned)
        let mut length = 0u64;
        for i in 0..len_size {
            length |= (attr_data[current_offset + 1 + i] as u64) << (i * 8);
        }

        // Parse run offset (signed)
        let mut run_offset_signed = 0i64;
        for i in 0..offset_size {
            run_offset_signed |= (attr_data[current_offset + 1 + len_size + i] as i64) << (i * 8);
        }

        // Sign extend if negative
        if offset_size > 0
            && (attr_data[current_offset + 1 + len_size + offset_size - 1] & 0x80) != 0
        {
            run_offset_signed |= !((1i64 << (offset_size * 8)) - 1);
        }

        // Calculate absolute cluster
        let absolute_cluster =
            prev_cluster.checked_add(run_offset_signed).ok_or(NtfsError::CorruptedFilesystem)?;

        if absolute_cluster < 0 {
            return Err(NtfsError::CorruptedFilesystem);
        }

        runs.push((absolute_cluster as u64, length));
        prev_cluster = absolute_cluster;

        current_offset += 1 + len_size + offset_size;
    }

    Ok(runs)
}

/// Read all data from data runs into a contiguous buffer
pub(crate) fn read_data_runs<D: BlockDevice>(
    device: &mut D,
    data_runs: &[DataRun],
    boot: &BootSector,
) -> Result<Vec<u8>, NtfsError> {
    let mut result = Vec::new();

    for run in data_runs {
        match run {
            DataRun::Resident { data } => {
                result.extend_from_slice(data);
            }
            DataRun::NonResident(runs) => {
                let bytes_per_cluster = boot.bytes_per_cluster();
                for (cluster, length) in runs {
                    let data = read_clusters(device, *cluster, *length, bytes_per_cluster)
                        .map_err(|_| NtfsError::IoError)?;
                    result.extend_from_slice(&data);
                }
            }
        }
    }

    Ok(result)
}
