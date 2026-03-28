use crate::error::NtfsError;

#[repr(C, packed)]
#[derive(Clone, Copy)]
pub struct BootSector {
    pub jump: [u8; 3],
    pub oem_id: [u8; 8],
    pub bytes_per_sector: u16,
    pub sectors_per_cluster: u8,
    pub reserved: [u8; 7],
    pub media_type: u8,
    pub total_sectors: u64,
    pub mft_start_cluster: u64,
    pub mft_mirror_start_cluster: u64,
    pub clusters_per_mft_record: i8,
    pub clusters_per_index_buffer: i8,
    pub serial_number: u64,
    pub checksum: u32,
    pub boot_code: [u8; 426],
    pub boot_signature: u16,
}

impl BootSector {
    pub fn from_bytes(buf: &[u8; 512]) -> Result<Self, NtfsError> {
        if &buf[0x03..0x0B] != b"NTFS    " {
            return Err(NtfsError::InvalidBootSector);
        }
        if buf[0x1FE] != 0x55 || buf[0x1FF] != 0xAA {
            return Err(NtfsError::InvalidBootSector);
        }
        // ... same parsing as before
        Ok(Self {
            jump: [buf[0], buf[1], buf[2]],
            oem_id: buf[0x03..0x0B].try_into().unwrap(),
            bytes_per_sector: u16::from_le_bytes([buf[0x0B], buf[0x0C]]),
            sectors_per_cluster: buf[0x0D],
            reserved: [0; 7],
            media_type: buf[0x15],
            total_sectors: u64_le(buf, 0x28),
            mft_start_cluster: u64_le(buf, 0x30),
            mft_mirror_start_cluster: u64_le(buf, 0x38),
            clusters_per_mft_record: buf[0x40] as i8,
            clusters_per_index_buffer: buf[0x44] as i8,
            serial_number: u64_le(buf, 0x48),
            checksum: u32::from_le_bytes([buf[0x50], buf[0x51], buf[0x52], buf[0x53]]),
            boot_code: [0; 426],
            boot_signature: 0xAA55,
        })
    }

    pub fn bytes_per_cluster(&self) -> u64 {
        self.bytes_per_sector as u64 * self.sectors_per_cluster as u64
    }

    pub fn mft_record_size(&self) -> usize {
        if self.clusters_per_mft_record > 0 {
            self.clusters_per_mft_record as usize * self.bytes_per_cluster() as usize
        } else {
            1usize << (-(self.clusters_per_mft_record as i32)) as usize
        }
    }

    pub fn mft_byte_offset(&self) -> u64 { self.mft_start_cluster * self.bytes_per_cluster() }
}

fn u64_le(buf: &[u8], off: usize) -> u64 {
    u64::from_le_bytes(buf[off..off + 8].try_into().unwrap())
}
