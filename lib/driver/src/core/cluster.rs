use alloc::{vec, vec::Vec};

use hal::{block::BlockDevice, io::IoError};

/// Read contiguous clusters from a block device.
///
/// # Arguments
/// * `cluster_number` - Starting Logical Cluster Number (LCN)
/// * `length` - Number of clusters to read
/// * `bytes_per_cluster` - Cluster size in bytes (from BootSector)
///
/// # Errors
/// Returns `IoError::InvalidInput` if the total size overflows usize or u64.
pub fn read_clusters<D: BlockDevice>(
    device: &mut D,
    cluster_number: u64,
    length: u64,
    bytes_per_cluster: u64,
) -> Result<Vec<u8>, IoError> {
    if length == 0 {
        return Ok(Vec::new());
    }

    let total_bytes = length.checked_mul(bytes_per_cluster).ok_or(IoError::InvalidInput)?;

    let total_bytes_usize = if total_bytes > usize::MAX as u64 {
        return Err(IoError::InvalidInput);
    } else {
        total_bytes as usize
    };

    let mut buf = vec![0u8; total_bytes_usize];
    let offset = cluster_number
        .checked_mul(bytes_per_cluster)
        .ok_or(IoError::InvalidInput)?;
    device.read_at(offset, &mut buf)?;
    Ok(buf)
}
