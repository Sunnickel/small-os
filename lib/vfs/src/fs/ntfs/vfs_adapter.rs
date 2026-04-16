use alloc::{
    boxed::Box,
    string::{String, ToString},
    sync::Arc,
    vec::Vec,
};

use driver::block::BlockDeviceEnum;
use hal::{fs::File, io::IoError};
use spin::Mutex;

use crate::{
    FileSystem,
    FsError,
    Inode,
    InodeType,
    Metadata,
    fs::ntfs::{NtfsDriver, NtfsFile, hal_impl::NtfsFileImpl},
    inode::DirectoryEntry,
};
// Import this

pub struct NtfsFsAdapter {
    inner: Arc<Mutex<NtfsDriver<BlockDeviceEnum>>>, // Add type parameter
    root_ino: u64,
}

impl NtfsFsAdapter {
    pub fn new(driver: NtfsDriver<BlockDeviceEnum>) -> Self {
        let inner = Arc::new(Mutex::new(driver));
        Self { inner, root_ino: 5 }
    }
}

impl FileSystem for NtfsFsAdapter {
    fn open(&self, path: &str) -> Result<Box<dyn File>, hal::fs::FsError> {
        let mut fs = self.inner.lock();

        // Use NtfsDriver::open to navigate the path
        let ntfs_file = fs.open(path).map_err(|_| hal::fs::FsError::NotFound)?;

        // Get file stats to determine type and size
        let stat = fs.stat(&ntfs_file).map_err(|_| hal::fs::FsError::Io(IoError::Other))?;

        if stat.is_directory {
            return Err(hal::fs::FsError::NotAFile);
        }

        Ok(Box::new(NtfsFileImpl {
            driver: self.inner.clone(),
            file: ntfs_file,
            offset: 0,
            path: path.to_string(),
            size: stat.size,
        }))
    }

    fn exists(&self, path: &str) -> bool {
        let mut fs = self.inner.lock();
        fs.open(path).is_ok()
    }
}

pub struct NtfsInode {
    fs: Arc<Mutex<NtfsDriver<BlockDeviceEnum>>>,
    ino: u64,
    typ: InodeType,
}

impl NtfsInode {
    fn to_ntfs_file(&self) -> NtfsFile { NtfsFile { record_number: self.ino } }
}

impl Inode for NtfsInode {
    fn metadata(&self) -> Metadata {
        let mut fs = self.fs.lock(); // Make mutable
        let file = self.to_ntfs_file();
        let stat = fs.stat(&file).unwrap_or_default();

        Metadata {
            inode_num: self.ino,
            size: stat.size,
            typ: if stat.is_directory { InodeType::Directory } else { InodeType::File },
            permissions: 0o755,
            created: 0,
            modified: 0,
            accessed: 0,
            links: 1,
        }
    }

    fn lookup(&self, name: &str) -> Result<Arc<dyn Inode>, FsError> {
        let mut fs = self.fs.lock(); // Make mutable
        let dir = self.to_ntfs_file();

        // find_in_directory returns u64 (record number)
        let child_ino = fs.find_in_directory(&dir, name).map_err(|_| FsError::NotFound)?;

        let child_stat =
            fs.stat(&NtfsFile { record_number: child_ino }).map_err(|_| FsError::IoError)?;

        Ok(Arc::new(NtfsInode {
            fs: self.fs.clone(),
            ino: child_ino,
            typ: if child_stat.is_directory { InodeType::Directory } else { InodeType::File },
        }))
    }

    fn readlink(&self) -> Result<String, FsError> { Err(FsError::NotImplemented) }

    fn read_at(&self, offset: u64, buf: &mut [u8]) -> Result<usize, FsError> {
        let mut fs = self.fs.lock(); // Make mutable
        let file = self.to_ntfs_file();

        // read_file_all returns Vec<u8>, not reading to buffer directly
        let data = fs.read_file_all(&file).map_err(|_| FsError::IoError)?;

        // Handle offset manually
        let start = offset as usize;
        let available = data.len().saturating_sub(start);
        let to_read = buf.len().min(available);

        if to_read > 0 {
            buf[..to_read].copy_from_slice(&data[start..start + to_read]);
        }

        Ok(to_read)
    }

    fn write_at(&self, offset: u64, buf: &[u8]) -> Result<usize, FsError> {
        let mut fs = self.fs.lock();
        let file = self.to_ntfs_file();

        // write_file takes &NtfsFile, data: &[u8]
        fs.write_file(&file, buf).map_err(|_| FsError::IoError)?;

        Ok(buf.len())
    }

    fn readdir(&self) -> Result<Vec<DirectoryEntry>, FsError> {
        let mut fs = self.fs.lock(); // Make mutable
        let dir = self.to_ntfs_file();

        // list_directory returns Vec<String>
        let names = fs.list_directory(&dir).map_err(|_| FsError::IoError)?;

        let mut entries = Vec::new();
        for name in names {
            if let Ok(ino) = fs.find_in_directory(&dir, &name) {
                let stat = fs.stat(&NtfsFile { record_number: ino }).unwrap_or_default();
                entries.push(DirectoryEntry {
                    name,
                    inode_num: ino,
                    typ: if stat.is_directory { InodeType::Directory } else { InodeType::File },
                });
            }
        }

        Ok(entries)
    }

    fn inode_type(&self) -> InodeType { self.typ }
}
