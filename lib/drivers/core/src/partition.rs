use hal::{block::BlockDevice, io::IoError};

pub struct Partition<B: BlockDevice> {
    pub inner: B,
    pub start_offset: u64,
    pub size: u64,
}

impl<B: BlockDevice> BlockDevice for Partition<B> {
    fn read_at(&mut self, offset: u64, buf: &mut [u8]) -> Result<(), IoError> {
        self.inner.read_at(self.start_offset + offset, buf)
    }

    fn write_at(&mut self, offset: u64, buf: &[u8]) -> Result<(), IoError> {
        if offset + buf.len() as u64 > self.size {
            return Err(IoError::Other);
        }
        self.inner.write_at(self.start_offset + offset, buf)
    }

    fn size(&self) -> u64 { self.size }

    fn sector_size(&self) -> usize { self.inner.sector_size() }
}
