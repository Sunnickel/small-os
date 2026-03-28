#![no_std]

extern crate alloc;
mod error;

use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use core::fmt;

pub use driver_core::block_device::{BlockDevice, BlockError};
use driver_core::block_device::read_clusters;
pub use error::FsError;

pub struct CreateOptions {
    pub is_directory: bool,
    pub data: Vec<u8>,
}

/// NTFS error types
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum NtfsError {
    InvalidBootSector,
    InvalidMftRecord,
    InvalidAttribute,
    FileNotFound,
    InvalidPath,
    IoError,
    NotADirectory,
    NotAFile,
    CorruptedFilesystem,
}

impl fmt::Display for NtfsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            NtfsError::InvalidBootSector    => write!(f, "invalid boot sector"),
            NtfsError::InvalidMftRecord     => write!(f, "invalid MFT record"),
            NtfsError::InvalidAttribute     => write!(f, "invalid attribute"),
            NtfsError::FileNotFound         => write!(f, "file not found"),
            NtfsError::InvalidPath          => write!(f, "invalid path"),
            NtfsError::IoError              => write!(f, "I/O error"),
            NtfsError::NotADirectory        => write!(f, "not a directory"),
            NtfsError::NotAFile             => write!(f, "not a file"),
            NtfsError::CorruptedFilesystem  => write!(f, "corrupted filesystem"),
        }
    }
}

// ---------------------------------------------------------------------------
// Boot sector
// ---------------------------------------------------------------------------

/// NTFS boot sector (BPB)
#[repr(C, packed)]
#[derive(Clone, Copy)]
struct BootSector {
    jump:                        [u8; 3],
    oem_id:                      [u8; 8],
    bytes_per_sector:            u16,
    sectors_per_cluster:         u8,
    reserved:                    [u8; 7],
    media_type:                  u8,
    total_sectors:               u64,
    mft_start_cluster:           u64,
    mft_mirror_start_cluster:    u64,
    clusters_per_mft_record:     i8,
    clusters_per_index_buffer:   i8,
    serial_number:               u64,
    checksum:                    u32,
    boot_code:                   [u8; 426],
    boot_signature:              u16,
}

impl BootSector {
    fn from_bytes(buf: &[u8; 512]) -> Result<Self, NtfsError> {
        if &buf[0x03..0x0B] != b"NTFS    " {
            return Err(NtfsError::InvalidBootSector);
        }
        if buf[0x1FE] != 0x55 || buf[0x1FF] != 0xAA {
            return Err(NtfsError::InvalidBootSector);
        }

        let u16_le = |off: usize| u16::from_le_bytes([buf[off], buf[off + 1]]);
        let u64_le = |off: usize| u64::from_le_bytes([
            buf[off], buf[off+1], buf[off+2], buf[off+3],
            buf[off+4], buf[off+5], buf[off+6], buf[off+7],
        ]);

        Ok(Self {
            jump: [buf[0], buf[1], buf[2]],
            oem_id: {
                let mut arr = [0u8; 8];
                arr.copy_from_slice(&buf[0x03..0x0B]);
                arr
            },
            bytes_per_sector:          u16_le(0x0B),
            sectors_per_cluster:       buf[0x0D],
            reserved:                  [0; 7],
            media_type:                buf[0x15],
            total_sectors:             u64_le(0x28),
            mft_start_cluster:         u64_le(0x30),
            mft_mirror_start_cluster:  u64_le(0x38),
            clusters_per_mft_record:   buf[0x40] as i8,
            clusters_per_index_buffer: buf[0x44] as i8,
            serial_number:             u64_le(0x48),
            checksum: u32::from_le_bytes([buf[0x50], buf[0x51], buf[0x52], buf[0x53]]),
            boot_code:       [0; 426],
            boot_signature:  0xAA55,
        })
    }

    fn bytes_per_cluster(&self) -> u64 {
        self.bytes_per_sector as u64 * self.sectors_per_cluster as u64
    }

    fn mft_record_size(&self) -> usize {
        if self.clusters_per_mft_record > 0 {
            self.clusters_per_mft_record as usize * self.bytes_per_cluster() as usize
        } else {
            1usize << (-(self.clusters_per_mft_record as i32)) as usize
        }
    }

    fn mft_byte_offset(&self) -> u64 {
        self.mft_start_cluster * self.bytes_per_cluster()
    }
}

// ---------------------------------------------------------------------------
// Attribute types
// ---------------------------------------------------------------------------

#[repr(u32)]
#[derive(Clone, Copy, PartialEq)]
pub enum AttributeType {
    StandardInformation  = 0x10,
    AttributeList        = 0x20,
    FileName             = 0x30,
    ObjectId             = 0x40,
    SecurityDescriptor   = 0x50,
    VolumeName           = 0x60,
    VolumeInformation    = 0x70,
    Data                 = 0x80,
    IndexRoot            = 0x90,
    IndexAllocation      = 0xA0,
    Bitmap               = 0xB0,
    ReparsePoint         = 0xC0,
    EaInformation        = 0xD0,
    Ea                   = 0xE0,
    LoggedUtilityStream  = 0x100,
    End                  = 0xFFFFFFFF,
}

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// Lightweight file handle — just an MFT record number.
///
/// This type intentionally owns *no* parsed data. Every driver method that
/// needs metadata (name, size, directory flag, data runs …) performs a fresh
/// read from the block device. That way the handle never goes stale after a
/// write or create operation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NtfsFile {
    record_number: u64,
}

impl NtfsFile {
    pub fn record_number(&self) -> u64 {
        self.record_number
    }
}

/// Ephemeral metadata snapshot returned by [`NtfsDriver::stat`].
///
/// Callers that need multiple fields from the same record should call `stat()`
/// once and destructure, rather than calling individual helpers repeatedly.
pub struct NtfsStat {
    pub is_directory: bool,
    pub size:         u64,
    pub name:         Option<String>,
    pub data_runs:    Vec<DataRun>,
    pub index_root:   Option<Vec<u8>>,
}

/// Data run (resident or non-resident)
#[derive(Clone)]
pub enum DataRun {
    Resident { data: Vec<u8> },
    /// (start_cluster, cluster_count)
    NonResident(Vec<(u64, u64)>),
}

/// Volume information
#[derive(Debug, Clone)]
pub struct VolumeInfo {
    pub sector_size:      u16,
    pub cluster_size:     u32,
    pub file_record_size: u32,
    pub mft_position:     u64,
    pub serial_number:    u64,
}

// ---------------------------------------------------------------------------
// Driver
// ---------------------------------------------------------------------------

pub struct NtfsDriver<D: BlockDevice> {
    device:          D,
    boot:            BootSector,
    mft_record_size: usize,
}

impl<D: BlockDevice> NtfsDriver<D> {
    // -----------------------------------------------------------------------
    // Mount / unmount
    // -----------------------------------------------------------------------

    pub fn mount(mut device: D) -> Result<Self, NtfsError> {
        let mut boot_buf = [0u8; 512];
        device.read_at(0, &mut boot_buf).map_err(|_| NtfsError::IoError)?;
        let boot = BootSector::from_bytes(&boot_buf)?;
        let mft_record_size = boot.mft_record_size();
        Ok(Self { device, boot, mft_record_size })
    }

    pub fn unmount(self) -> D {
        self.device
    }

    // -----------------------------------------------------------------------
    // Public handle-returning operations
    // -----------------------------------------------------------------------

    /// Return a handle to the root directory (MFT record 5).
    pub fn root_directory(&mut self) -> Result<NtfsFile, NtfsError> {
        self.open_file(5)
    }

    /// Return a handle to the file/directory at `path`.
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
            let child_record = self.find_in_directory_with_index(
                stat.index_root.as_deref().ok_or(NtfsError::NotADirectory)?,
                component,
            )?;
            current = self.open_file(child_record)?;
        }
        Ok(current)
    }

    /// Open an MFT record by number; verify it is a valid FILE record.
    pub fn open_file(&mut self, record_number: u64) -> Result<NtfsFile, NtfsError> {
        // Validate the record exists and has a proper FILE signature.
        let _ = self.read_mft_record(record_number)?;
        Ok(NtfsFile { record_number })
    }

    // -----------------------------------------------------------------------
    // Metadata — always read fresh from disk
    // -----------------------------------------------------------------------

    /// Parse all relevant attributes from the MFT record and return them.
    ///
    /// This is the single source of truth for every piece of file metadata.
    /// Every other public method that needs metadata calls this internally.
    pub fn stat(&mut self, file: &NtfsFile) -> Result<NtfsStat, NtfsError> {
        let record = self.read_mft_record(file.record_number)?;
        let mut stat = NtfsStat {
            is_directory: false,
            size:         0,
            name:         None,
            data_runs:    Vec::new(),
            index_root:   None,
        };

        for (attr_type, attr_data, is_resident) in self.parse_attributes(&record) {
            match attr_type {
                AttributeType::FileName => {
                    if is_resident && stat.name.is_none() {
                        if let Some((name, _)) = self.parse_filename(attr_data) {
                            stat.name = Some(name);
                        }
                    }
                }
                AttributeType::Data => {
                    if is_resident {
                        let value_offset =
                            u16::from_le_bytes([attr_data[20], attr_data[21]]) as usize;
                        let value_length = u32::from_le_bytes([
                            attr_data[16], attr_data[17], attr_data[18], attr_data[19],
                        ]) as usize;
                        if value_offset + value_length <= attr_data.len() {
                            stat.size = value_length as u64;
                            stat.data_runs.push(DataRun::Resident {
                                data: attr_data[value_offset..value_offset + value_length].to_vec(),
                            });
                        }
                    } else {
                        let runs = self.parse_data_runs(attr_data)?;
                        stat.size = runs.iter()
                            .map(|(_, len)| len * self.boot.bytes_per_cluster())
                            .sum();
                        stat.data_runs.push(DataRun::NonResident(runs));
                    }
                }
                AttributeType::IndexRoot => {
                    if is_resident {
                        stat.is_directory = true;
                        // Skip the 24-byte resident attribute header.
                        if attr_data.len() > 24 {
                            stat.index_root = Some(attr_data[24..].to_vec());
                        }
                    }
                }
                _ => {}
            }
        }
        Ok(stat)
    }

    /// Convenience: is this handle a directory?
    pub fn is_directory(&mut self, file: &NtfsFile) -> Result<bool, NtfsError> {
        Ok(self.stat(file)?.is_directory)
    }

    /// Convenience: byte size of the file's data stream.
    pub fn file_size(&mut self, file: &NtfsFile) -> Result<u64, NtfsError> {
        Ok(self.stat(file)?.size)
    }

    /// Convenience: primary filename (if present in MFT record).
    pub fn file_name(&mut self, file: &NtfsFile) -> Result<Option<String>, NtfsError> {
        Ok(self.stat(file)?.name)
    }

    // -----------------------------------------------------------------------
    // Read
    // -----------------------------------------------------------------------

    /// Read the entire content of a file into a `Vec<u8>`.
    ///
    /// Always reads from disk — never from a stale cache.
    pub fn read_file_all(&mut self, file: &NtfsFile) -> Result<Vec<u8>, NtfsError> {
        let stat = self.stat(file)?;
        if stat.is_directory {
            return Err(NtfsError::NotAFile);
        }
        let mut content = Vec::with_capacity(stat.size as usize);
        for run in &stat.data_runs {
            match run {
                DataRun::Resident { data } => {
                    content.extend_from_slice(data);
                }
                DataRun::NonResident(runs) => {
                    for (cluster, length) in runs {
                        let bytes = read_clusters(
                            &mut self.device,
                            cluster,
                            length,
                            self.boot.bytes_per_cluster(),
                        ).map_err(|_| NtfsError::IoError)?;
                        content.extend_from_slice(&bytes);
                    }
                }
            }
        }
        Ok(content)
    }

    /// Read up to `buf.len()` bytes from a file into `buf`.
    ///
    /// Returns the number of bytes actually read.
    pub fn read_file(&mut self, file: &NtfsFile, buf: &mut [u8]) -> Result<usize, NtfsError> {
        let data = self.read_file_all(file)?;
        let n = buf.len().min(data.len());
        buf[..n].copy_from_slice(&data[..n]);
        Ok(n)
    }

    // -----------------------------------------------------------------------
    // Directory listing
    // -----------------------------------------------------------------------

    /// Return names of all entries in a directory.
    ///
    /// Always reads the directory index fresh from disk.
    pub fn list_directory(&mut self, dir: &NtfsFile) -> Result<Vec<String>, NtfsError> {
        let stat = self.stat(dir)?;
        let index_data = stat.index_root.ok_or(NtfsError::NotADirectory)?;
        self.list_from_index(&index_data)
    }

    /// Find an entry in a directory by name and return its MFT record number.
    ///
    /// Always reads the directory index fresh from disk.
    pub fn find_in_directory(
        &mut self,
        dir: &NtfsFile,
        name: &str,
    ) -> Result<u64, NtfsError> {
        let stat = self.stat(dir)?;
        let index_data = stat.index_root.ok_or(NtfsError::NotADirectory)?;
        self.find_in_directory_with_index(&index_data, name)
    }

    // -----------------------------------------------------------------------
    // Write
    // -----------------------------------------------------------------------

    /// Overwrite the content of a resident file in-place.
    ///
    /// Restrictions (same as before):
    /// - File must be resident (≲ ~700 bytes).
    /// - `data.len()` must equal the current on-disk value length (no resize).
    ///
    /// Because `NtfsFile` owns no cached data, no in-memory update is needed;
    /// the next call to `read_file_all` / `stat` will see the new bytes.
    pub fn write_file(&mut self, file: &NtfsFile, data: &[u8]) -> Result<(), NtfsError> {
        // Fresh read to validate size / residency.
        let stat = self.stat(file)?;
        let run = stat.data_runs.first().ok_or(NtfsError::InvalidAttribute)?;
        match run {
            DataRun::Resident { data: existing } => {
                if data.len() != existing.len() {
                    return Err(NtfsError::InvalidAttribute); // no resize
                }
            }
            DataRun::NonResident(_) => {
                return Err(NtfsError::InvalidAttribute); // not implemented
            }
        }

        let mut record = self.read_mft_record(file.record_number)?;
        let data_attr_offset = self.find_data_attribute_offset(&record)?;
        self.update_resident_data(&mut record, data_attr_offset, data)?;
        self.reapply_fixups(&mut record)?;

        let mft_offset =
            self.boot.mft_byte_offset() + file.record_number * self.mft_record_size as u64;
        self.device.write_at(mft_offset, &record).map_err(|_| NtfsError::IoError)?;
        Ok(())
        // No in-memory state to update — next stat() reads fresh from disk.
    }

    // -----------------------------------------------------------------------
    // Create
    // -----------------------------------------------------------------------

    /// Create a new file or directory inside `parent`.
    ///
    /// Returns a live handle to the newly created entry.
    pub fn create_file(
        &mut self,
        parent: &NtfsFile,
        name: &str,
        options: CreateOptions,
    ) -> Result<NtfsFile, NtfsError> {
        // Check parent is a directory (fresh read).
        if !self.is_directory(parent)? {
            return Err(NtfsError::NotADirectory);
        }
        // Reject if the name already exists.
        if self.find_in_directory(parent, name).is_ok() {
            return Err(NtfsError::FileNotFound); // already exists
        }

        let new_record_number = self.allocate_mft_record()?;
        let record = self.create_mft_record(
            new_record_number,
            name,
            parent.record_number,
            options.is_directory,
            &options.data,
        )?;

        let mft_offset =
            self.boot.mft_byte_offset() + new_record_number * self.mft_record_size as u64;
        self.device.write_at(mft_offset, &record).map_err(|_| NtfsError::IoError)?;

        // TODO: insert index entry into parent's $INDEX_ROOT / $INDEX_ALLOCATION.
        let _ = self.add_directory_entry(parent, new_record_number, name);

        // Return a live handle — open_file re-reads and validates the record.
        self.open_file(new_record_number)
    }

    // -----------------------------------------------------------------------
    // Volume info
    // -----------------------------------------------------------------------

    pub fn volume_info(&self) -> VolumeInfo {
        VolumeInfo {
            sector_size:      self.boot.bytes_per_sector,
            cluster_size:     self.boot.bytes_per_cluster() as u32,
            file_record_size: self.mft_record_size as u32,
            mft_position:     self.boot.mft_byte_offset(),
            serial_number:    self.boot.serial_number,
        }
    }

    // -----------------------------------------------------------------------
    // Low-level MFT helpers (private)
    // -----------------------------------------------------------------------

    fn read_mft_record(&mut self, record_number: u64) -> Result<Vec<u8>, NtfsError> {
        let offset =
            self.boot.mft_byte_offset() + record_number * self.mft_record_size as u64;
        let mut buf = vec![0u8; self.mft_record_size];
        self.device.read_at(offset, &mut buf).map_err(|_| NtfsError::IoError)?;

        if &buf[0..4] != b"FILE" {
            return Err(NtfsError::InvalidMftRecord);
        }
        self.apply_fixups(&mut buf)?;
        Ok(buf)
    }

    fn apply_fixups(&self, buf: &mut [u8]) -> Result<(), NtfsError> {
        let uso = u16::from_le_bytes([buf[4], buf[5]]) as usize;
        let usc = u16::from_le_bytes([buf[6], buf[7]]) as usize;

        if uso < 48 || usc == 0 {
            return Ok(());
        }
        if uso + usc * 2 > buf.len() {
            return Err(NtfsError::CorruptedFilesystem);
        }

        let seq = u16::from_le_bytes([buf[uso], buf[uso + 1]]);

        for i in 1..usc {
            let end = i * 512 - 2;
            if end + 2 > buf.len() {
                return Err(NtfsError::CorruptedFilesystem);
            }
            let found = u16::from_le_bytes([buf[end], buf[end + 1]]);
            if found != seq {
                return Err(NtfsError::CorruptedFilesystem);
            }
            let rep = uso + i * 2;
            if rep + 2 > buf.len() {
                return Err(NtfsError::CorruptedFilesystem);
            }
            buf[end]     = buf[rep];
            buf[end + 1] = buf[rep + 1];
        }
        Ok(())
    }

    fn reapply_fixups(&self, buf: &mut [u8]) -> Result<(), NtfsError> {
        let uso = u16::from_le_bytes([buf[4], buf[5]]) as usize;
        let usc = u16::from_le_bytes([buf[6], buf[7]]) as usize;

        if uso < 48 || usc == 0 {
            return Ok(());
        }
        let seq = u16::from_le_bytes([buf[uso], buf[uso + 1]]);
        for i in 1..usc {
            let end = i * 512 - 2;
            buf[end]     = (seq & 0xFF) as u8;
            buf[end + 1] = (seq >> 8) as u8;
        }
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Attribute iterator (private)
    // -----------------------------------------------------------------------

    /// Yields `(AttributeType, raw_attr_slice, is_resident)` for every
    /// attribute in the record until the end marker or a malformed entry.
    fn parse_attributes<'a>(
        &self,
        record: &'a [u8],
    ) -> impl Iterator<Item = (AttributeType, &'a [u8], bool)> {
        let first = u16::from_le_bytes([record[20], record[21]]) as usize;
        let mut offset = first;

        core::iter::from_fn(move || {
            if offset + 8 > record.len() {
                return None;
            }
            let type_code = u32::from_le_bytes([
                record[offset], record[offset+1], record[offset+2], record[offset+3],
            ]);
            if type_code == 0xFFFFFFFF {
                return None;
            }
            let record_length = u32::from_le_bytes([
                record[offset+4], record[offset+5], record[offset+6], record[offset+7],
            ]) as usize;
            if record_length == 0 || offset + record_length > record.len() {
                return None;
            }
            let is_resident = record[offset + 8] == 0;
            let attr_type = match type_code {
                0x10  => AttributeType::StandardInformation,
                0x20  => AttributeType::AttributeList,
                0x30  => AttributeType::FileName,
                0x40  => AttributeType::ObjectId,
                0x50  => AttributeType::SecurityDescriptor,
                0x60  => AttributeType::VolumeName,
                0x70  => AttributeType::VolumeInformation,
                0x80  => AttributeType::Data,
                0x90  => AttributeType::IndexRoot,
                0xA0  => AttributeType::IndexAllocation,
                0xB0  => AttributeType::Bitmap,
                0xC0  => AttributeType::ReparsePoint,
                0xD0  => AttributeType::EaInformation,
                0xE0  => AttributeType::Ea,
                0x100 => AttributeType::LoggedUtilityStream,
                _     => return None,
            };
            let slice = &record[offset..offset + record_length];
            offset += record_length;
            Some((attr_type, slice, is_resident))
        })
    }

    // -----------------------------------------------------------------------
    // Filename / index helpers (private)
    // -----------------------------------------------------------------------

    /// Parse a `$FILE_NAME` attribute (resident form) and return `(name, parent_record_number)`.
    fn parse_filename(&self, attr_data: &[u8]) -> Option<(String, u64)> {
        // Resident attribute header is 24 bytes; $FILE_NAME data starts there.
        let header_len = 24;
        if attr_data.len() < header_len + 0x42 {
            return None;
        }
        let data = &attr_data[header_len..];
        let parent = u64::from_le_bytes([
            data[0], data[1], data[2], data[3], data[4], data[5], data[6], data[7],
        ]);
        let name_len = data[0x40] as usize;
        let name_off = 0x42;
        if name_off + name_len * 2 > data.len() {
            return None;
        }
        let raw = &data[name_off..name_off + name_len * 2];
        let mut name = String::with_capacity(name_len);
        for i in 0..name_len {
            let c = u16::from_le_bytes([raw[i * 2], raw[i * 2 + 1]]);
            if c == 0 { break; }
            if c < 128 { name.push(c as u8 as char); } else { name.push('?'); }
        }
        Some((name, parent))
    }

    /// Parse the filename embedded in an index entry key.
    fn parse_filename_from_key(&self, key: &[u8]) -> Option<(String, u64)> {
        if key.len() < 0x42 {
            return None;
        }
        let parent = u64::from_le_bytes([
            key[0], key[1], key[2], key[3], key[4], key[5], key[6], key[7],
        ]);
        let name_len = key[0x40] as usize;
        let name_off = 0x42;
        if name_off + name_len * 2 > key.len() {
            return None;
        }
        let raw = &key[name_off..name_off + name_len * 2];
        let mut name = String::with_capacity(name_len);
        for i in 0..name_len {
            let c = u16::from_le_bytes([raw[i * 2], raw[i * 2 + 1]]);
            if c == 0 { break; }
            if c < 128 { name.push(c as u8 as char); } else { name.push('?'); }
        }
        Some((name, parent))
    }

    /// Walk a raw index block and return all filenames.
    fn list_from_index(&self, index_data: &[u8]) -> Result<Vec<String>, NtfsError> {
        let mut result = Vec::new();
        if index_data.len() < 16 {
            return Ok(result);
        }
        let first_entry = u32::from_le_bytes([
            index_data[0], index_data[1], index_data[2], index_data[3],
        ]) as usize;
        let total_size = u32::from_le_bytes([
            index_data[4], index_data[5], index_data[6], index_data[7],
        ]) as usize;

        let mut offset = first_entry;
        while offset + 0x12 <= total_size && offset + 0x12 <= index_data.len() {
            let entry = &index_data[offset..];
            let entry_len = u16::from_le_bytes([entry[8], entry[9]]) as usize;
            let key_len   = u16::from_le_bytes([entry[10], entry[11]]) as usize;
            let flags     = u16::from_le_bytes([entry[12], entry[13]]);

            if flags & 0x0002 != 0 { break; } // LAST_ENTRY

            if key_len > 0 && offset + 0x10 + key_len <= index_data.len() {
                let key = &entry[0x10..0x10 + key_len];
                if let Some((name, _)) = self.parse_filename_from_key(key) {
                    result.push(name);
                }
            }

            if entry_len == 0 { break; }
            offset += entry_len;
        }
        Ok(result)
    }

    /// Search a raw index block for `name`, returning the MFT record number.
    fn find_in_directory_with_index(
        &self,
        index_data: &[u8],
        name: &str,
    ) -> Result<u64, NtfsError> {
        if index_data.len() < 16 {
            return Err(NtfsError::FileNotFound);
        }
        let first_entry = u32::from_le_bytes([
            index_data[0], index_data[1], index_data[2], index_data[3],
        ]) as usize;
        let total_size = u32::from_le_bytes([
            index_data[4], index_data[5], index_data[6], index_data[7],
        ]) as usize;

        let mut offset = first_entry;
        while offset + 0x12 <= total_size && offset + 0x12 <= index_data.len() {
            let entry = &index_data[offset..];
            let file_ref  = u64::from_le_bytes([
                entry[0], entry[1], entry[2], entry[3],
                entry[4], entry[5], entry[6], entry[7],
            ]);
            let entry_len = u16::from_le_bytes([entry[8], entry[9]]) as usize;
            let key_len   = u16::from_le_bytes([entry[10], entry[11]]) as usize;
            let flags     = u16::from_le_bytes([entry[12], entry[13]]);

            if flags & 0x0002 != 0 { break; }

            if key_len > 0 && offset + 0x10 + key_len <= index_data.len() {
                let key = &entry[0x10..0x10 + key_len];
                if let Some((entry_name, _)) = self.parse_filename_from_key(key) {
                    if entry_name.eq_ignore_ascii_case(name) {
                        return Ok(file_ref & 0x0000FFFFFFFFFFFF);
                    }
                }
            }

            if entry_len == 0 { break; }
            offset += entry_len;
        }
        Err(NtfsError::FileNotFound)
    }

    // -----------------------------------------------------------------------
    // Data-run parser (private)
    // -----------------------------------------------------------------------

    fn parse_data_runs(&self, attr_data: &[u8]) -> Result<Vec<(u64, u64)>, NtfsError> {
        if attr_data.len() < 34 {
            return Err(NtfsError::InvalidAttribute);
        }
        let pairs_offset = u16::from_le_bytes([attr_data[32], attr_data[33]]) as usize;
        let mut runs = Vec::new();
        let mut offset = pairs_offset;
        let mut prev_cluster = 0u64;

        while offset < attr_data.len() && attr_data[offset] != 0 {
            let header     = attr_data[offset];
            let len_bytes  = (header & 0x0F) as usize;
            let off_bytes  = ((header >> 4) & 0x0F) as usize;

            if offset + 1 + len_bytes + off_bytes > attr_data.len() {
                break;
            }

            let mut run_len: u64 = 0;
            for i in 0..len_bytes {
                run_len |= (attr_data[offset + 1 + i] as u64) << (i * 8);
            }

            let mut run_off: i64 = 0;
            for i in 0..off_bytes {
                run_off |= (attr_data[offset + 1 + len_bytes + i] as i64) << (i * 8);
            }
            if off_bytes > 0
                && (attr_data[offset + 1 + len_bytes + off_bytes - 1] & 0x80) != 0
            {
                run_off |= !((1i64 << (off_bytes * 8)) - 1);
            }

            let cluster = if run_off < 0 {
                (prev_cluster as i64 + run_off) as u64
            } else {
                prev_cluster + run_off as u64
            };

            runs.push((cluster, run_len));
            prev_cluster = cluster;
            offset += 1 + len_bytes + off_bytes;
        }
        Ok(runs)
    }

    // -----------------------------------------------------------------------
    // Write helpers (private)
    // -----------------------------------------------------------------------

    fn find_data_attribute_offset(&self, record: &[u8]) -> Result<usize, NtfsError> {
        let first = u16::from_le_bytes([record[20], record[21]]) as usize;
        let mut offset = first;
        while offset + 8 <= record.len() {
            let attr_type = u32::from_le_bytes([
                record[offset], record[offset+1], record[offset+2], record[offset+3],
            ]);
            if attr_type == 0xFFFFFFFF {
                return Err(NtfsError::InvalidAttribute);
            }
            if attr_type == 0x80 {
                return Ok(offset);
            }
            let attr_len = u32::from_le_bytes([
                record[offset+4], record[offset+5], record[offset+6], record[offset+7],
            ]) as usize;
            if attr_len == 0 { break; }
            offset += attr_len;
        }
        Err(NtfsError::InvalidAttribute)
    }

    fn update_resident_data(
        &self,
        record: &mut [u8],
        attr_offset: usize,
        data: &[u8],
    ) -> Result<(), NtfsError> {
        let value_offset =
            u16::from_le_bytes([record[attr_offset + 20], record[attr_offset + 21]]) as usize;
        let value_length = u32::from_le_bytes([
            record[attr_offset + 16], record[attr_offset + 17],
            record[attr_offset + 18], record[attr_offset + 19],
        ]) as usize;

        if data.len() != value_length {
            return Err(NtfsError::InvalidAttribute);
        }
        let start = attr_offset + value_offset;
        record[start..start + data.len()].copy_from_slice(data);
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Create helpers (private)
    // -----------------------------------------------------------------------

    fn allocate_mft_record(&mut self) -> Result<u64, NtfsError> {
        // Scan records 16+ for a free slot (first 16 are reserved system files).
        // A production implementation should read $Bitmap instead.
        for num in 16u64..1000 {
            let offset = self.boot.mft_byte_offset() + num * self.mft_record_size as u64;
            let mut buf = vec![0u8; self.mft_record_size];
            if self.device.read_at(offset, &mut buf).is_ok() {
                if &buf[0..4] != b"FILE" && &buf[0..4] != b"BAAD" {
                    return Ok(num);
                }
            }
        }
        Err(NtfsError::IoError)
    }

    fn create_mft_record(
        &mut self,
        record_number: u64,
        name: &str,
        parent_record: u64,
        is_directory: bool,
        data: &[u8],
    ) -> Result<Vec<u8>, NtfsError> {
        let mut record = vec![0u8; self.mft_record_size];

        // ---- MFT record header ----
        record[0..4].copy_from_slice(b"FILE");
        record[4..6].copy_from_slice(&48u16.to_le_bytes());   // USO
        record[6..8].copy_from_slice(&3u16.to_le_bytes());    // USC
        record[8..16].copy_from_slice(&0u64.to_le_bytes());   // $LogFile LSN
        record[16..18].copy_from_slice(&(record_number as u16).to_le_bytes()); // seq
        record[18..20].copy_from_slice(&1u16.to_le_bytes());  // hard link count
        record[20..22].copy_from_slice(&56u16.to_le_bytes()); // first attr offset
        record[22..24].copy_from_slice(&1u16.to_le_bytes());  // flags: IN_USE
        record[24..28].copy_from_slice(&(self.mft_record_size as u32).to_le_bytes());
        record[28..32].copy_from_slice(&(self.mft_record_size as u32).to_le_bytes());

        // Update sequence array at offset 48.
        let seq = (record_number & 0xFFFF) as u16;
        record[48..50].copy_from_slice(&seq.to_le_bytes());
        // Fixup slots initialised to zero; will be stamped onto sector ends below.

        let mut off = 56usize;
        off = self.write_standard_info_attr(&mut record, off, is_directory)?;
        off = self.write_filename_attr(&mut record, off, name, parent_record)?;
        if is_directory {
            off = self.write_index_root_attr(&mut record, off)?;
        } else {
            off = self.write_data_attr(&mut record, off, data)?;
        }
        // End-of-attributes marker.
        record[off..off + 4].copy_from_slice(&0xFFFF_FFFFu32.to_le_bytes());

        // Stamp sequence number at the end of each 512-byte sector.
        let seq_bytes = seq.to_le_bytes();
        if self.mft_record_size >= 512  { record[510..512].copy_from_slice(&seq_bytes); }
        if self.mft_record_size >= 1024 { record[1022..1024].copy_from_slice(&seq_bytes); }

        Ok(record)
    }

    fn write_standard_info_attr(
        &self,
        record: &mut [u8],
        offset: usize,
        is_directory: bool,
    ) -> Result<usize, NtfsError> {
        let data_len: u32 = 56;
        let attr_len: u32 = 24 + data_len; // 80

        record[offset..offset+4].copy_from_slice(&0x10u32.to_le_bytes());
        record[offset+4..offset+8].copy_from_slice(&attr_len.to_le_bytes());
        record[offset+8]  = 0; // resident
        record[offset+9]  = 0; // no name
        record[offset+10..offset+12].copy_from_slice(&0u16.to_le_bytes());
        record[offset+12..offset+14].copy_from_slice(&0u16.to_le_bytes());
        record[offset+14..offset+16].copy_from_slice(&0u16.to_le_bytes());
        record[offset+16..offset+20].copy_from_slice(&data_len.to_le_bytes());
        record[offset+20..offset+22].copy_from_slice(&24u16.to_le_bytes());
        record[offset+22] = 0;
        record[offset+23] = 0;

        let d = offset + 24;
        let now = 0u64;
        let flags: u32 = if is_directory { 0x10 } else { 0x20 }; // Directory / Archive
        record[d..d+8].copy_from_slice(&now.to_le_bytes());    // creation
        record[d+8..d+16].copy_from_slice(&now.to_le_bytes()); // modification
        record[d+16..d+24].copy_from_slice(&now.to_le_bytes()); // mft change
        record[d+24..d+32].copy_from_slice(&now.to_le_bytes()); // access
        record[d+32..d+36].copy_from_slice(&flags.to_le_bytes());
        // remaining 20 bytes already zero

        Ok(offset + attr_len as usize)
    }

    fn write_filename_attr(
        &self,
        record: &mut [u8],
        offset: usize,
        name: &str,
        parent_record: u64,
    ) -> Result<usize, NtfsError> {
        let utf16: Vec<u16> = name.encode_utf16().collect();
        let name_len = utf16.len();
        let data_len = 66 + name_len * 2;
        let attr_len = (24 + data_len + 7) & !7; // align to 8

        record[offset..offset+4].copy_from_slice(&0x30u32.to_le_bytes());
        record[offset+4..offset+8].copy_from_slice(&(attr_len as u32).to_le_bytes());
        record[offset+8]  = 0;
        record[offset+9]  = 0;
        record[offset+10..offset+12].copy_from_slice(&0u16.to_le_bytes());
        record[offset+12..offset+14].copy_from_slice(&0u16.to_le_bytes());
        record[offset+14..offset+16].copy_from_slice(&0u16.to_le_bytes());
        record[offset+16..offset+20].copy_from_slice(&(data_len as u32).to_le_bytes());
        record[offset+20..offset+22].copy_from_slice(&24u16.to_le_bytes());
        record[offset+22] = 0;
        record[offset+23] = 0;

        let d = offset + 24;
        record[d..d+8].copy_from_slice(&parent_record.to_le_bytes());
        // timestamps at d+8 .. d+40: already zero
        // allocated size / real size at d+40 .. d+56: already zero
        // flags at d+56 .. d+60: already zero
        record[d+64] = name_len as u8; // name length in chars
        record[d+65] = 0;              // namespace: POSIX

        for (i, &c) in utf16.iter().enumerate() {
            let pos = d + 66 + i * 2;
            record[pos..pos+2].copy_from_slice(&c.to_le_bytes());
        }
        Ok(offset + attr_len)
    }

    fn write_data_attr(
        &self,
        record: &mut [u8],
        offset: usize,
        data: &[u8],
    ) -> Result<usize, NtfsError> {
        if data.len() > 700 {
            return Err(NtfsError::InvalidAttribute); // too large for resident
        }
        let data_len = data.len();
        let attr_len = (24 + data_len + 7) & !7;

        record[offset..offset+4].copy_from_slice(&0x80u32.to_le_bytes());
        record[offset+4..offset+8].copy_from_slice(&(attr_len as u32).to_le_bytes());
        record[offset+8]  = 0;
        record[offset+9]  = 0;
        record[offset+10..offset+12].copy_from_slice(&0u16.to_le_bytes());
        record[offset+12..offset+14].copy_from_slice(&0u16.to_le_bytes());
        record[offset+14..offset+16].copy_from_slice(&0u16.to_le_bytes());
        record[offset+16..offset+20].copy_from_slice(&(data_len as u32).to_le_bytes());
        record[offset+20..offset+22].copy_from_slice(&24u16.to_le_bytes());
        record[offset+22] = 0;
        record[offset+23] = 0;

        record[offset+24..offset+24+data_len].copy_from_slice(data);
        Ok(offset + attr_len)
    }

    fn write_index_root_attr(
        &self,
        record: &mut [u8],
        offset: usize,
    ) -> Result<usize, NtfsError> {
        // Minimal $INDEX_ROOT: 24-byte attr header + 16-byte index root + 16-byte last-entry.
        let data_len: u32 = 32;
        let attr_len: u32 = 24 + data_len;

        record[offset..offset+4].copy_from_slice(&0x90u32.to_le_bytes());
        record[offset+4..offset+8].copy_from_slice(&attr_len.to_le_bytes());
        record[offset+8]  = 0;
        record[offset+9]  = 0;
        record[offset+10..offset+12].copy_from_slice(&0u16.to_le_bytes());
        record[offset+12..offset+14].copy_from_slice(&0u16.to_le_bytes());
        record[offset+14..offset+16].copy_from_slice(&0u16.to_le_bytes());
        record[offset+16..offset+20].copy_from_slice(&data_len.to_le_bytes());
        record[offset+20..offset+22].copy_from_slice(&24u16.to_le_bytes());
        record[offset+22] = 0;
        record[offset+23] = 0;

        let d = offset + 24;
        // Index root: type=$FILE_NAME, collation=FILE_NAME
        record[d..d+4].copy_from_slice(&0x30u32.to_le_bytes()); // indexed attr type
        record[d+4..d+8].copy_from_slice(&0x01u32.to_le_bytes()); // collation: FILE_NAME
        record[d+8..d+12].copy_from_slice(&(self.mft_record_size as u32).to_le_bytes()); // index block size
        record[d+12] = self.boot.clusters_per_index_buffer as u8;

        // Index header (starts at d+16): first_entry_offset, total_size, alloc_size, flags
        let index_header_off = 16u32;
        let last_entry_size  = 16u32; // just the header, no key
        record[d+16..d+20].copy_from_slice(&index_header_off.to_le_bytes()); // first entry offset
        record[d+20..d+24].copy_from_slice(&(index_header_off + last_entry_size).to_le_bytes()); // total
        record[d+24..d+28].copy_from_slice(&(index_header_off + last_entry_size).to_le_bytes()); // alloc
        record[d+28] = 0; // flags: small index (no $INDEX_ALLOCATION)

        // Last-entry placeholder (16 bytes, LAST_ENTRY flag set)
        let le = d + 32;
        record[le..le+8].copy_from_slice(&0u64.to_le_bytes());   // file reference: none
        record[le+8..le+10].copy_from_slice(&16u16.to_le_bytes()); // entry length
        record[le+10..le+12].copy_from_slice(&0u16.to_le_bytes()); // key length
        record[le+12..le+14].copy_from_slice(&0x0002u16.to_le_bytes()); // LAST_ENTRY flag

        Ok(offset + attr_len as usize)
    }

    /// Placeholder — insert the new entry into the parent's $INDEX_ROOT.
    ///
    /// A full B-tree insertion is non-trivial; this stub exists so the rest of
    /// the create path compiles. Until implemented, `list_directory` on the
    /// parent will not show newly created children.
    fn add_directory_entry(
        &mut self,
        _parent: &NtfsFile,
        _child_record: u64,
        _name: &str,
    ) -> Result<(), NtfsError> {
        // TODO: implement proper $INDEX_ROOT / $INDEX_ALLOCATION insertion.
        Ok(())
    }
}