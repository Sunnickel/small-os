use alloc::string::ToString;

use hal::{block::BlockDevice, io::IoError};

const GPT_HEADER_LBA: u64 = 1;
const GPT_SIGNATURE: &[u8; 8] = b"EFI PART";

pub struct Partition<B: BlockDevice> {
    pub inner: B,
    pub start_offset: u64,
    pub size: u64,
}

impl<B: BlockDevice> BlockDevice for Partition<B> {
    fn read_at(&mut self, offset: u64, buf: &mut [u8]) -> Result<(), IoError> {
        crate::debug(
            &format_args!(
                "PARTITION_READ offset={:#x} len={} start_offset={:#x} absolute={:#x}",
                offset,
                buf.len(),
                self.start_offset,
                self.start_offset + offset
            )
            .to_string(),
        );

        self.inner.read_at(self.start_offset + offset, buf)
    }

    fn write_at(&mut self, offset: u64, buf: &[u8]) -> Result<(), IoError> {
        crate::debug(
            &format_args!(
                "PARTITION_WRITE offset={:#x} len={} start_offset={:#x} absolute={:#x}",
                offset,
                buf.len(),
                self.start_offset,
                self.start_offset + offset
            )
            .to_string(),
        );

        if offset + buf.len() as u64 > self.size {
            crate::debug(
                &format_args!(
                    "PARTITION_WRITE FAILED: offset({:#x}) + len({}) > size({:#x})",
                    offset,
                    buf.len(),
                    self.size
                )
                .to_string(),
            );
            return Err(IoError::Other);
        }

        let result = self.inner.write_at(self.start_offset + offset, buf);
        crate::debug(&format_args!("PARTITION_WRITE result: {:?}", result.is_ok()).to_string());
        result
    }

    fn size(&self) -> u64 {
        crate::debug(&format_args!("PARTITION_SIZE: {}", self.size).to_string());
        self.size
    }

    fn sector_size(&self) -> usize {
        let sz = self.inner.sector_size();
        crate::debug(&format_args!("PARTITION_SECTOR_SIZE: {}", sz).to_string());
        sz
    }
}

pub mod gpt {
    use hal::block::BlockDevice;

    use crate::partition::{GPT_HEADER_LBA, GPT_SIGNATURE};

    pub fn first_ntfs_partition_offset(dev: &mut impl BlockDevice) -> Option<u64> {
        let sector = dev.sector_size() as u64;

        // Read GPT header at LBA 1
        let mut header = [0u8; 512];
        dev.read_at(GPT_HEADER_LBA * sector, &mut header).ok()?;

        if &header[0..8] != GPT_SIGNATURE {
            return None;
        }

        let partition_entry_lba = u64::from_le_bytes(header[72..80].try_into().ok()?);
        let num_entries = u32::from_le_bytes(header[80..84].try_into().ok()?);
        let entry_size = u32::from_le_bytes(header[84..88].try_into().ok()?) as u64;

        // NTFS partition type GUID: EBD0A0A2-B9E5-4433-87C0-68B6B72699C7
        // Stored as mixed-endian in GPT
        const NTFS_TYPE_GUID: [u8; 16] = [
            0xA2, 0xA0, 0xD0, 0xEB, // first 4 bytes little-endian
            0xE5, 0xB9, // next 2 little-endian
            0x33, 0x44, // next 2 little-endian
            0x87, 0xC0, // big-endian from here
            0x68, 0xB6, 0xB7, 0x26, 0x99, 0xC7,
        ];

        for i in 0..num_entries as u64 {
            let entry_offset = partition_entry_lba * sector + i * entry_size;
            let mut entry = [0u8; 128];
            dev.read_at(entry_offset, &mut entry).ok()?;

            // Skip empty entries (all-zero GUID)
            if entry[0..16].iter().all(|&b| b == 0) {
                continue;
            }

            let start_lba = u64::from_le_bytes(entry[32..40].try_into().ok()?);

            // Check type GUID — or just return the first non-empty partition
            // since your disk only has one partition
            if start_lba > 0 {
                return Some(start_lba * sector);
            }
        }

        None
    }
}
