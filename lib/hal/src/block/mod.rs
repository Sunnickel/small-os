mod error;

pub use error::BlockError;

use crate::io::IoError;

pub trait BlockDevice: Send + Sync {
    fn read_blocks(&mut self, lba: u64, buf: &mut [u8]) -> Result<(), BlockError>;
    fn write_blocks(&mut self, lba: u64, buf: &[u8]) -> Result<(), BlockError>;
    fn block_size(&self) -> usize;
    fn block_count(&self) -> u64;
}
