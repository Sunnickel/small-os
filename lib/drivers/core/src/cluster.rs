use alloc::{vec, vec::Vec};

use hal::{block::BlockDevice, io::IoError};

pub fn read_clusters<D: BlockDevice>(
    device: &mut D,
    cluster_number: u64,
    length: u64,
    bytes_per_cluster: u64,
) -> Result<Vec<u8>, IoError> {
    let total_bytes = length.checked_mul(bytes_per_cluster).ok_or(IoError::InvalidInput)?;

    let mut buf = vec![0u8; total_bytes as usize];
    let offset = cluster_number * bytes_per_cluster;
    device.read_at(offset, &mut buf)?;
    Ok(buf)
}
