use alloc::vec::Vec;

use driver_core::cluster::read_clusters;
use hal::block::BlockDevice;

use crate::{boot::BootSector, DataRun, NtfsError};

/// Read all data from data runs into a single buffer.
pub fn read_all<D: BlockDevice>(
    device: &mut D,
    data_runs: &[DataRun],
    boot: &BootSector,
) -> Result<Vec<u8>, NtfsError> {
    let mut content = Vec::new();

    for run in data_runs {
        match run {
            DataRun::Resident { data } => {
                content.extend_from_slice(data);
            }
            DataRun::NonResident(runs) => {
                for (cluster, length) in runs {
                    let bytes = read_clusters(device, *cluster, *length, boot.bytes_per_cluster())
                        .map_err(|_| NtfsError::IoError)?;
                    content.extend_from_slice(&bytes);
                }
            }
        }
    }

    Ok(content)
}

/// Parse data runs from a non-resident attribute header.
pub fn parse_data_runs(attr_data: &[u8]) -> Result<Vec<(u64, u64)>, NtfsError> {
    if attr_data.len() < 34 {
        return Err(NtfsError::InvalidAttribute);
    }
    let pairs_offset = u16::from_le_bytes([attr_data[32], attr_data[33]]) as usize;
    let mut runs = Vec::new();
    let mut offset = pairs_offset;
    let mut prev_cluster = 0u64;

    while offset < attr_data.len() && attr_data[offset] != 0 {
        let header = attr_data[offset];
        let len_bytes = (header & 0x0F) as usize;
        let off_bytes = ((header >> 4) & 0x0F) as usize;

        if offset + 1 + len_bytes + off_bytes > attr_data.len() {
            break;
        }

        let mut run_len: u64 = 0;
        for i in 0..len_bytes {
            run_len |= (attr_data[offset + 1 + i] as u64) << (i * 8);
        }

        let mut run_off: i64 = 0;
        for i in 0..off_bytes {
            run_off |= (attr_data[offset + 1 + len_bytes + i] as i64) << (i * 8);
        }
        if off_bytes > 0 && (attr_data[offset + 1 + len_bytes + off_bytes - 1] & 0x80) != 0 {
            run_off |= !((1i64 << (off_bytes * 8)) - 1);
        }

        let cluster = if run_off < 0 {
            (prev_cluster as i64 + run_off) as u64
        } else {
            prev_cluster + run_off as u64
        };

        runs.push((cluster, run_len));
        prev_cluster = cluster;
        offset += 1 + len_bytes + off_bytes;
    }
    Ok(runs)
}
