use alloc::{string::String, vec, vec::Vec};

use hal::block::BlockDevice;

use crate::{
    attr,
    boot::BootSector,
    error::NtfsError,
    index,
    index::find_index_root_offset,
    runs,
    types::{CreateOptions, NtfsFile, NtfsStat, VolumeInfo},
    write,
};

pub struct NtfsDriver<D: BlockDevice> {
    pub(crate) device: D,
    pub(crate) boot: BootSector,
    pub(crate) mft_record_size: usize,
}

impl<D: BlockDevice> NtfsDriver<D> {
    pub fn mount(mut device: D, partition_byte_offset: u64) -> Result<Self, NtfsError> {
        let mut boot_buf = [0u8; 512];
        device.read_at(partition_byte_offset, &mut boot_buf).map_err(|_| NtfsError::IoError)?;
        let boot = BootSector::from_bytes(&boot_buf, partition_byte_offset)?;
        let mft_record_size = boot.mft_record_size();
        Ok(Self { device, boot, mft_record_size })
    }

    pub fn unmount(self) -> D { self.device }

    pub fn root_directory(&mut self) -> Result<NtfsFile, NtfsError> {
        Ok(NtfsFile { record_number: 5 })
    }

    pub fn open(&mut self, path: &str) -> Result<NtfsFile, NtfsError> {
        if path == "/" || path.is_empty() {
            return self.root_directory();
        }
        let mut current = self.root_directory()?;
        for component in path.split('/').filter(|s| !s.is_empty()) {
            let stat = self.stat(&current)?;
            if !stat.is_directory {
                return Err(NtfsError::NotADirectory);
            }
            let child = index::find(
                stat.index_root.as_deref().ok_or(NtfsError::NotADirectory)?,
                component,
            )?;
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
        runs::read_all(&mut self.device, &stat.data_runs, &self.boot)
    }

    pub fn read_file(&mut self, file: &NtfsFile, buf: &mut [u8]) -> Result<usize, NtfsError> {
        let data = self.read_file_all(file)?;
        let n = buf.len().min(data.len());
        buf[..n].copy_from_slice(&data[..n]);
        Ok(n)
    }

    pub fn list_directory(&mut self, dir: &NtfsFile) -> Result<Vec<String>, NtfsError> {
        let mut stat = self.stat(dir)?;

        // Special-case root record 5
        if dir.record_number == 5 && stat.index_root.is_none() {
            let record = self.read_mft_record(5)?;
            let offset = find_index_root_offset(&record)?;
            let index_len = record.len() - offset;
            stat.index_root = Some(record[offset..offset + index_len].to_vec());
        }

        let index_data = stat.index_root.ok_or(NtfsError::NotADirectory)?;
        index::list(&index_data)
    }

    pub fn find_in_directory(&mut self, dir: &NtfsFile, name: &str) -> Result<u64, NtfsError> {
        let stat = self.stat(dir)?;
        let index_data = stat.index_root.ok_or(NtfsError::NotADirectory)?;
        index::find(&index_data, name)
    }

    pub fn write_file(&mut self, file: &NtfsFile, data: &[u8]) -> Result<(), NtfsError> {
        write::write_file(self, file, data)
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
        self.device.read_at(offset, &mut buf).map_err(|_| NtfsError::IoError)?;
        if &buf[0..4] != b"FILE" {
            return Err(NtfsError::InvalidMftRecord);
        }
        attr::apply_fixups(&mut buf, self.boot.bytes_per_sector as usize)?;
        Ok(buf)
    }
}
