use alloc::vec;
use alloc::vec::Vec;
use core::fmt;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BlockError {
    DeviceError,
    NotInitialized,
    OutOfBounds,
    Timeout,
    NoMemory,
    IoError,
    InvalidParameter,
    InvalidOffset,
    NotSupported,
}

impl fmt::Display for BlockError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BlockError::OutOfBounds => write!(f, "access out of bounds"),
            BlockError::InvalidOffset => write!(f, "invalid offset"),
            BlockError::DeviceError => write!(f, "device error"),
            BlockError::NotInitialized => write!(f, "device not initialized"),
            BlockError::IoError => write!(f, "I/O error"),
            BlockError::NotSupported => write!(f, "operation not supported"),
            BlockError::Timeout => write!(f, "timeout"),
            BlockError::NoMemory => write!(f, "no memory"),
            BlockError::InvalidParameter => write!(f, "invalid parameter"),
        }
    }
}

// --- Minimal no_std I/O primitives ---

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum IoErrorKind {
    UnexpectedEof,
    InvalidInput,
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct IoError {
    pub kind: IoErrorKind,
}

impl IoError {
    pub fn new(kind: IoErrorKind) -> Self {
        Self { kind }
    }
}

pub type IoResult<T> = Result<T, IoError>;

pub enum SeekFrom {
    Start(u64),
    Current(i64),
    End(i64),
}

pub trait Read {
    fn read(&mut self, buf: &mut [u8]) -> IoResult<usize>;

    fn read_exact(&mut self, mut buf: &mut [u8]) -> IoResult<()> {
        while !buf.is_empty() {
            match self.read(buf) {
                Ok(0) => return Err(IoError::new(IoErrorKind::UnexpectedEof)),
                Ok(n) => buf = &mut buf[n..],
                Err(e) => return Err(e),
            }
        }
        Ok(())
    }
}

pub trait Seek {
    fn seek(&mut self, pos: SeekFrom) -> IoResult<u64>;

    fn stream_position(&mut self) -> IoResult<u64> {
        self.seek(SeekFrom::Current(0))
    }
}

// --- BlockDevice trait ---

pub trait BlockDevice {
    fn read_at(&mut self, offset: u64, buf: &mut [u8]) -> Result<(), BlockError>;
    fn write_at(&mut self, offset: u64, buf: &[u8]) -> Result<(), BlockError>;
    fn size(&self) -> u64;
    fn sector_size(&self) -> usize;
}

// --- BlockStream ---

pub struct BlockStream<D: BlockDevice> {
    device: D,
    position: u64,
    size: u64,
    sector_buffer: [u8; 4096],
    sector_size: usize,
    buffered_sector: Option<u64>,
}

impl<D: BlockDevice> BlockStream<D> {
    pub fn new(device: D) -> Self {
        let size = device.size();
        let sector_size = device.sector_size();
        assert!(sector_size <= 4096, "Sector size too large");

        Self {
            device,
            position: 0,
            size,
            sector_buffer: [0u8; 4096],
            sector_size,
            buffered_sector: None,
        }
    }

    pub fn into_device(self) -> D {
        self.device
    }

    pub fn device(&mut self) -> &mut D {
        &mut self.device
    }

    pub fn size(&self) -> u64 {
        self.size
    }

    fn read_internal(&mut self, buf: &mut [u8]) -> Result<usize, BlockError> {
        if self.position >= self.size {
            return Ok(0);
        }

        let to_read = core::cmp::min(buf.len(), (self.size - self.position) as usize);
        if to_read == 0 {
            return Ok(0);
        }

        let mut total_read = 0;

        while total_read < to_read {
            let sector = self.position / self.sector_size as u64;
            let sector_offset = (self.position % self.sector_size as u64) as usize;
            let remaining_in_sector = self.sector_size - sector_offset;
            let to_copy = core::cmp::min(remaining_in_sector, to_read - total_read);

            if self.buffered_sector != Some(sector) {
                self.device.read_at(
                    sector * self.sector_size as u64,
                    &mut self.sector_buffer[..self.sector_size],
                )?;
                self.buffered_sector = Some(sector);
            }

            buf[total_read..total_read + to_copy]
                .copy_from_slice(&self.sector_buffer[sector_offset..sector_offset + to_copy]);

            self.position += to_copy as u64;
            total_read += to_copy;
        }

        Ok(total_read)
    }
}

impl<D: BlockDevice> Read for BlockStream<D> {
    fn read(&mut self, buf: &mut [u8]) -> IoResult<usize> {
        self.read_internal(buf)
            .map_err(|_| IoError::new(IoErrorKind::Other))
    }
}

impl<D: BlockDevice> Seek for BlockStream<D> {
    fn seek(&mut self, pos: SeekFrom) -> IoResult<u64> {
        let new_pos: Option<u64> = match pos {
            SeekFrom::Start(off) => Some(off),
            SeekFrom::Current(off) => (self.position as i64).checked_add(off).map(|n| n as u64),
            SeekFrom::End(off) => (self.size as i64).checked_add(off).map(|n| n as u64),
        };

        match new_pos {
            Some(p) if p <= self.size => {
                self.position = p;
                Ok(self.position)
            }
            _ => Err(IoError::new(IoErrorKind::InvalidInput)),
        }
    }

    fn stream_position(&mut self) -> IoResult<u64> {
        Ok(self.position)
    }
}

pub fn read_clusters<D: BlockDevice>(
    device: &mut D,
    cluster_number: u64,
    length: u64,
    bytes_per_cluster: u64,
) -> Result<Vec<u8>, BlockError> {
    let mut buf = vec![0u8; (length * bytes_per_cluster) as usize];

    // Calculate offset in bytes
    let offset = cluster_number * bytes_per_cluster;

    device.read_at(offset, &mut buf)?;

    Ok(buf)
}