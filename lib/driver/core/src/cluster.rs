use alloc::{string::ToString, vec, vec::Vec};

use hal::{block::BlockDevice, io::IoError};

pub fn read_clusters<D: BlockDevice>(
    device: &mut D,
    cluster_number: u64,
    length: u64,
    bytes_per_cluster: u64,
) -> Result<Vec<u8>, IoError> {
    crate::debug(
        &format_args!(
            "READ_CLUSTERS cluster={} length={} bytes_per_cluster={}",
            cluster_number, length, bytes_per_cluster
        )
        .to_string(),
    );

    let total_bytes = length.checked_mul(bytes_per_cluster).ok_or(IoError::InvalidInput)?;
    crate::debug(&format_args!("READ_CLUSTERS total_bytes={}", total_bytes).to_string());

    let mut buf = vec![0u8; total_bytes as usize];
    let offset = cluster_number * bytes_per_cluster;
    crate::debug(&format_args!("READ_CLUSTERS offset={:#x}", offset).to_string());

    device.read_at(offset, &mut buf)?;
    crate::debug(&format_args!("READ_CLUSTERS completed, read {} bytes", buf.len()).to_string());

    Ok(buf)
}
