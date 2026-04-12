mod error;

pub use error::BlockError;

use crate::io::IoError;

pub trait BlockDevice {
    fn read_at(&mut self, offset: u64, buf: &mut [u8]) -> Result<(), IoError>;
    fn write_at(&mut self, offset: u64, buf: &[u8]) -> Result<(), IoError>;
    fn size(&self) -> u64;
    fn sector_size(&self) -> usize;

    /// Calculate number of blocks/sectors from size and sector_size
    fn block_count(&self) -> u64 {
        let sector_size = self.sector_size() as u64;
        if sector_size == 0 {
            return 0;
        }
        self.size() / sector_size
    }
}
