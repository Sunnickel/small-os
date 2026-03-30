use alloc::{string::String, vec::Vec};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NtfsFile {
    pub(crate) record_number: u64,
}

impl NtfsFile {
    pub fn record_number(&self) -> u64 { self.record_number }
}

pub struct NtfsStat {
    pub is_directory: bool,
    pub size: u64,
    pub name: Option<String>,
    pub data_runs: Vec<DataRun>,
    pub index_root: Option<Vec<u8>>,
}

#[derive(Clone)]
pub enum DataRun {
    Resident { data: Vec<u8> },
    NonResident(Vec<(u64, u64)>),
}

#[derive(Debug, Clone)]
pub struct VolumeInfo {
    pub sector_size: u16,
    pub cluster_size: u32,
    pub file_record_size: u32,
    pub mft_position: u64,
    pub serial_number: u64,
}

pub struct CreateOptions {
    pub is_directory: bool,
    pub data: Vec<u8>,
}

#[repr(u32)]
#[derive(Clone, Copy, PartialEq)]
pub enum AttributeType {
    StandardInformation = 0x10,
    AttributeList = 0x20,
    FileName = 0x30,
    ObjectId = 0x40,
    SecurityDescriptor = 0x50,
    VolumeName = 0x60,
    VolumeInformation = 0x70,
    Data = 0x80,
    IndexRoot = 0x90,
    IndexAllocation = 0xA0,
    Bitmap = 0xB0,
    ReparsePoint = 0xC0,
    EaInformation = 0xD0,
    Ea = 0xE0,
    LoggedUtilityStream = 0x100,
    End = 0xFFFFFFFF,
}
