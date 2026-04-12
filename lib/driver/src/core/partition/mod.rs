pub(crate) mod gpt;

use alloc::string::ToString;

use hal::{block::BlockDevice, io::IoError};

use crate::util::debug;

pub struct Partition<B: BlockDevice> {
    pub inner: B,
    pub start_offset: u64,
    pub size: u64,
}

impl<B: BlockDevice> BlockDevice for Partition<B> {
    fn read_at(&mut self, offset: u64, buf: &mut [u8]) -> Result<(), IoError> {
        debug(
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
        debug(
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
            debug(
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
        debug(&format_args!("PARTITION_WRITE result: {:?}", result.is_ok()).to_string());
        result
    }

    fn size(&self) -> u64 {
        debug(&format_args!("PARTITION_SIZE: {}", self.size).to_string());
        self.size
    }

    fn sector_size(&self) -> usize {
        let sz = self.inner.sector_size();
        debug(&format_args!("PARTITION_SECTOR_SIZE: {}", sz).to_string());
        sz
    }
}
