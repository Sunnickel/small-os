use crate::error::NtfsError;

/// Raw NTFS BPB — parsed manually, repr(C) dropped since we aren't
/// casting raw pointers into this.
#[derive(Clone, Copy, Debug)]
pub struct BootSector {
    pub bytes_per_sector: u16,
    pub sectors_per_cluster: u8,
    pub media_type: u8,
    pub total_sectors: u64,
    pub mft_start_cluster: u64,
    pub mft_mirror_start_cluster: u64,
    pub clusters_per_mft_record: i8,
    pub clusters_per_index_buffer: i8,
    pub serial_number: u64,

    /// Byte offset of the partition from the start of the block device.
    /// For a raw-NTFS device this is 0.
    /// For a GPT disk it is partition_start_lba * bytes_per_sector.
    pub partition_byte_offset: u64,
}

impl BootSector {
    /// Parse from a 512-byte buffer that was read from the first sector of
    /// the NTFS partition.  `partition_byte_offset` is the byte offset of
    /// that sector within the block device (0 for raw NTFS, non-zero for
    /// partitioned disks).
    pub fn from_bytes(buf: &[u8; 512], partition_byte_offset: u64) -> Result<Self, NtfsError> {
        if &buf[0x03..0x0B] != b"NTFS    " {
            return Err(NtfsError::InvalidBootSector);
        }
        if buf[0x1FE] != 0x55 || buf[0x1FF] != 0xAA {
            return Err(NtfsError::InvalidBootSector);
        }

        Ok(Self {
            bytes_per_sector: u16::from_le_bytes([buf[0x0B], buf[0x0C]]),
            sectors_per_cluster: buf[0x0D],
            media_type: buf[0x15],
            total_sectors: u64_le(buf, 0x28),
            mft_start_cluster: u64_le(buf, 0x30),
            mft_mirror_start_cluster: u64_le(buf, 0x38),
            clusters_per_mft_record: buf[0x40] as i8,
            clusters_per_index_buffer: buf[0x44] as i8,
            serial_number: u64_le(buf, 0x48),
            partition_byte_offset,
        })
    }

    pub fn bytes_per_cluster(&self) -> u64 {
        self.bytes_per_sector as u64 * self.sectors_per_cluster as u64
    }

    /// Size of one MFT file record in bytes.
    ///
    /// If `clusters_per_mft_record` is positive it is a cluster count.
    /// If it is negative the record size is 2^|value| bytes (common: -10 = 1024
    /// B).
    pub fn mft_record_size(&self) -> usize {
        if self.clusters_per_mft_record > 0 {
            self.clusters_per_mft_record as usize * self.bytes_per_cluster() as usize
        } else {
            1usize << (-(self.clusters_per_mft_record as i32))
            //        ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
            // Cast to i32 BEFORE negating so -128i8 doesn't overflow,
            // then shift — result is usize implicitly.
        }
    }

    /// Byte offset of the MFT from the start of the **block device**.
    /// This is what you pass to `device.read_at()`.
    pub fn mft_byte_offset(&self) -> u64 {
        self.partition_byte_offset + self.mft_start_cluster * self.bytes_per_cluster()
    }

    /// Byte offset of the MFT mirror from the start of the **block device**.
    pub fn mft_mirror_byte_offset(&self) -> u64 {
        self.partition_byte_offset + self.mft_mirror_start_cluster * self.bytes_per_cluster()
    }

    /// Byte offset of any cluster from the start of the **block device**.
    pub fn cluster_to_disk_offset(&self, lcn: u64) -> u64 {
        self.partition_byte_offset + lcn * self.bytes_per_cluster()
    }
}

fn u64_le(buf: &[u8], off: usize) -> u64 {
    u64::from_le_bytes(buf[off..off + 8].try_into().unwrap())
}
