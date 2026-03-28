mod error;

pub use error::BlockError;

use crate::io::IoError;

pub trait BlockDevice {
    fn read_at(&mut self, offset: u64, buf: &mut [u8]) -> Result<(), IoError>;
    fn write_at(&mut self, offset: u64, buf: &[u8]) -> Result<(), IoError>;
    fn size(&self) -> u64;
    fn sector_size(&self) -> usize;
}
