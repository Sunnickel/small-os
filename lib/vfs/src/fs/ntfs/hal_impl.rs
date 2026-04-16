use alloc::{
    boxed::Box,
    string::{String, ToString},
    sync::Arc,
};

use driver::block::BlockDeviceEnum;
use hal::{
    fs::{File, FileSystem, FsError},
    io::{IoError, Read, Seek, SeekFrom},
};
use spin::Mutex;

use crate::fs::ntfs::{NtfsDriver, NtfsFile};

pub struct NtfsFsImpl {
    driver: Arc<Mutex<NtfsDriver<BlockDeviceEnum>>>,
}

impl NtfsFsImpl {
    pub fn new(block_dev: BlockDeviceEnum) -> Result<Self, FsError> {
        let driver = NtfsDriver::mount(block_dev, 0).map_err(|_| FsError::Io(IoError::Other))?;
        Ok(Self { driver: Arc::new(Mutex::new(driver)) })
    }
}

impl FileSystem for NtfsFsImpl {
    fn open(&self, path: &str) -> Result<Box<dyn File>, FsError> {
        let mut driver = self.driver.lock();
        // Use driver.open() not find_path()
        let ntfs_file = driver.open(path).map_err(|_| FsError::NotFound)?;

        // Get stat to check if it's a file
        let stat = driver.stat(&ntfs_file).map_err(|_| FsError::Io(IoError::Other))?;
        if stat.is_directory {
            return Err(FsError::NotAFile);
        }

        Ok(Box::new(NtfsFileImpl {
            driver: self.driver.clone(),
            file: ntfs_file,
            offset: 0,
            path: path.to_string(),
            size: stat.size,
        }))
    }

    fn exists(&self, path: &str) -> bool { self.open(path).is_ok() }
}

pub struct NtfsFileImpl {
    pub(crate) driver: Arc<Mutex<NtfsDriver<BlockDeviceEnum>>>,
    pub(crate) file: NtfsFile, // Use NtfsFile, not FileRecord
    pub(crate) offset: u64,
    pub(crate) path: String,
    pub(crate) size: u64,
}

impl File for NtfsFileImpl {
    fn size(&self) -> u64 { self.size }

    fn path(&self) -> &str { &self.path }
}

impl Read for NtfsFileImpl {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, IoError> {
        let mut driver = self.driver.lock();

        // Read full file (simplified - you may want partial reads)
        let data = driver.read_file_all(&self.file).map_err(|_| IoError::Other)?;

        // Copy from offset
        let start = self.offset as usize;
        let available = data.len().saturating_sub(start);
        let to_read = buf.len().min(available);

        if to_read > 0 {
            buf[..to_read].copy_from_slice(&data[start..start + to_read]);
            self.offset += to_read as u64;
        }

        Ok(to_read)
    }
}

impl Seek for NtfsFileImpl {
    fn seek(&mut self, pos: SeekFrom) -> Result<u64, IoError> {
        match pos {
            SeekFrom::Start(n) => self.offset = n,
            SeekFrom::Current(n) => self.offset += n as u64,
            SeekFrom::End(n) => self.offset = self.size.saturating_add(n as u64),
        }
        Ok(self.offset)
    }
}
