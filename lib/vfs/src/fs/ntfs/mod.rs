pub(crate) mod attr;
pub(crate) mod boot;
pub(crate) mod error;
pub mod hal_impl;
pub(crate) mod index;
pub(crate) mod runs;
pub(crate) mod types;
pub(crate) mod vfs_adapter;
pub(crate) mod write;

use alloc::{
    string::{String, ToString},
    vec,
    vec::Vec,
};

use hal::block::BlockDevice;

pub use crate::fs::ntfs::types::{CreateOptions, NtfsFile, NtfsStat, VolumeInfo};
use crate::{
    Inode,
    fs::ntfs::{boot::BootSector, error::NtfsError, index::find_index_root_offset},
};

pub struct NtfsDriver<D: BlockDevice> {
    pub device: D,
    pub(crate) boot: BootSector,
    pub(crate) mft_record_size: usize,
}

impl<D: BlockDevice> NtfsDriver<D> {
    pub fn mount(mut device: D, partition_byte_offset: u64) -> Result<Self, NtfsError> {
        let mut boot_buf = [0u8; 512];
        device.read_blocks(partition_byte_offset, &mut boot_buf).map_err(|_| NtfsError::IoError)?;
        let boot = BootSector::from_bytes(&boot_buf, partition_byte_offset)?;
        let mft_record_size = boot.mft_record_size();
        Ok(Self { device, boot, mft_record_size })
    }

    pub fn unmount(self) -> D { self.device }

    pub fn root_directory(&mut self) -> Result<NtfsFile, NtfsError> {
        Ok(NtfsFile { record_number: 5 })
    }

    pub fn open(&mut self, path: &str) -> Result<NtfsFile, NtfsError> {
        let normalized = NtfsDriver::<D>::normalize_path(path);

        if normalized == "/" || normalized.is_empty() {
            return self.root_directory();
        }

        let mut current = self.root_directory()?;
        for component in normalized.split('/').filter(|s| !s.is_empty()) {
            if component == ".." {
                return Err(NtfsError::InvalidInput);
            }

            let stat = self.stat(&current)?;
            if !stat.is_directory {
                return Err(NtfsError::NotADirectory);
            }

            let index_data = self.get_index_data(&current, &stat)?;
            let child = index::find_in_directory(&index_data, component)?;

            current = self.open_file(child)?;
        }
        Ok(current)
    }

    pub fn open_file(&mut self, record_number: u64) -> Result<NtfsFile, NtfsError> {
        let _ = self.read_mft_record(record_number)?;
        Ok(NtfsFile { record_number })
    }

    pub fn stat(&mut self, file: &NtfsFile) -> Result<NtfsStat, NtfsError> {
        let record = self.read_mft_record(file.record_number)?;
        attr::parse_stat(&record, &self.boot)
    }

    pub fn is_directory(&mut self, file: &NtfsFile) -> Result<bool, NtfsError> {
        if file.record_number == 5 {
            return Ok(true);
        }
        Ok(self.stat(file)?.is_directory)
    }

    pub fn file_size(&mut self, file: &NtfsFile) -> Result<u64, NtfsError> {
        Ok(self.stat(file)?.size)
    }

    pub fn file_name(&mut self, file: &NtfsFile) -> Result<Option<String>, NtfsError> {
        Ok(self.stat(file)?.name)
    }

    pub fn read_file_all(&mut self, file: &NtfsFile) -> Result<Vec<u8>, NtfsError> {
        let stat = self.stat(file)?;
        if stat.is_directory {
            return Err(NtfsError::NotAFile);
        }
        runs::read_data_runs(&mut self.device, &stat.data_runs, &self.boot)
    }

    pub fn read_file(&mut self, file: &NtfsFile, buf: &mut [u8]) -> Result<usize, NtfsError> {
        let data = self.read_file_all(file)?;
        let n = buf.len().min(data.len());
        buf[..n].copy_from_slice(&data[..n]);
        Ok(n)
    }

    pub fn list_directory(&mut self, dir: &NtfsFile) -> Result<Vec<String>, NtfsError> {
        let stat = self.stat(dir)?;
        let index_data = self.get_index_data(dir, &stat)?;
        index::list_directory(&index_data)
    }

    pub fn find_in_directory(&mut self, dir: &NtfsFile, name: &str) -> Result<u64, NtfsError> {
        let stat = self.stat(dir)?;
        let index_data = self.get_index_data(dir, &stat)?;
        index::find_in_directory(&index_data, name)
    }

    pub fn write_file(&mut self, file: &NtfsFile, data: &[u8]) -> Result<(), NtfsError> {
        write::write_resident_file(self, file, data)
    }

    pub fn create_file(
        &mut self,
        parent: &NtfsFile,
        name: &str,
        options: CreateOptions,
    ) -> Result<NtfsFile, NtfsError> {
        write::create_file(self, parent, name, options)
    }

    pub fn volume_info(&self) -> VolumeInfo {
        VolumeInfo {
            sector_size: self.boot.bytes_per_sector,
            cluster_size: self.boot.bytes_per_cluster() as u32,
            file_record_size: self.mft_record_size as u32,
            mft_position: self.boot.mft_byte_offset(),
            serial_number: self.boot.serial_number,
        }
    }

    pub(crate) fn apply_fixups(&mut self, buf: &mut [u8]) -> Result<(), NtfsError> {
        Ok(attr::apply_fixups(buf, self.boot.bytes_per_sector as usize)?)
    }

    pub(crate) fn reapply_fixups(&mut self, buf: &mut [u8]) -> Result<(), NtfsError> {
        Ok(attr::reapply_fixups(buf, self.boot.bytes_per_sector as usize)?)
    }

    pub(crate) fn read_mft_record(&mut self, record_number: u64) -> Result<Vec<u8>, NtfsError> {
        let offset = self.boot.mft_byte_offset() + record_number * self.mft_record_size as u64;
        let mut buf = vec![0u8; self.mft_record_size];

        self.device.read_blocks(offset, &mut buf).map_err(|_| NtfsError::IoError)?;

        if &buf[0..4] != b"FILE" {
            return Err(NtfsError::InvalidMftRecord);
        }

        attr::apply_fixups(&mut buf, self.boot.bytes_per_sector as usize)?;
        Ok(buf)
    }

    fn get_index_data(&mut self, file: &NtfsFile, stat: &NtfsStat) -> Result<Vec<u8>, NtfsError> {
        if let Some(data) = &stat.index_root {
            return Ok(data.clone());
        }

        // Need to extract from record (for root with non-resident index allocation)
        let record = self.read_mft_record(file.record_number)?;
        let offset = find_index_root_offset(&record)?;

        if record[offset + 8] != 0 {
            return Err(NtfsError::NotSupported);
        }

        let val_off =
            u16::from_le_bytes(record[offset + 20..offset + 22].try_into().unwrap()) as usize;
        Ok(record[offset + val_off..].to_vec())
    }

    fn normalize_path(path: &str) -> String {
        if path.is_empty() {
            return "/".to_string();
        }

        let mut result = String::with_capacity(path.len());
        let mut prev_was_slash = false;

        for c in path.chars() {
            if c == '/' {
                if !prev_was_slash {
                    result.push(c);
                    prev_was_slash = true;
                }
            } else {
                result.push(c);
                prev_was_slash = false;
            }
        }

        if result.len() > 1 && result.ends_with('/') {
            result.pop();
        }

        result
    }
}
