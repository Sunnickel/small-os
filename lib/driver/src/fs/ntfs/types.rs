use alloc::{string::String, vec::Vec};

use bitflags::bitflags;

/// Represents an NTFS file by its MFT record number
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct NtfsFile {
    pub(crate) record_number: u64,
}

impl NtfsFile {
    pub const fn new(record_number: u64) -> Self { Self { record_number } }

    pub const fn record_number(&self) -> u64 { self.record_number }
}

/// File statistics returned by stat operations
#[derive(Clone, Debug)]
pub struct NtfsStat {
    pub is_directory: bool,
    pub size: u64,
    pub name: Option<String>,
    pub data_runs: Vec<DataRun>,
    pub index_root: Option<Vec<u8>>,
    pub standard_info: Option<StandardInformation>,
    pub security_descriptor: Option<SecurityDescriptor>,
    pub object_id: Option<ObjectId>,
    pub reparse_point: Option<ReparsePoint>,
    pub alternate_data_streams: Vec<AlternateDataStream>,
}

/// Data location representation - either resident (in-MFT) or non-resident
/// (cluster runs)
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DataRun {
    Resident { data: Vec<u8> },
    NonResident(Vec<(u64, u64)>), // (start_cluster, length_in_clusters)
}

/// $STANDARD_INFORMATION attribute (0x10) - Always resident
#[derive(Clone, Debug)]
pub struct StandardInformation {
    pub created: u64, // FILETIME format
    pub modified: u64,
    pub mft_modified: u64,
    pub accessed: u64,
    pub file_attributes: FileAttributes,
    // NTFS 3.0+ extensions
    pub owner_id: Option<u32>,
    pub security_id: Option<u32>,
    pub quota_charged: Option<u64>,
    pub usn: Option<u64>,
}

bitflags! {
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub struct FileAttributes: u32 {
        const READ_ONLY       = 0x0001;
        const HIDDEN          = 0x0002;
        const SYSTEM          = 0x0004;
        const ARCHIVE         = 0x0020;
        const DEVICE          = 0x0040;
        const NORMAL          = 0x0080;
        const TEMPORARY       = 0x0100;
        const SPARSE_FILE     = 0x0200;
        const REPARSE_POINT   = 0x0400;
        const COMPRESSED      = 0x0800;
        const OFFLINE         = 0x1000;
        const NOT_INDEXED     = 0x2000;
        const ENCRYPTED       = 0x4000;
        const DIRECTORY       = 0x1000_0000;
        const INDEX_VIEW      = 0x2000_0000;
    }
}

impl StandardInformation {
    pub fn parse(attr_data: &[u8]) -> Option<Self> {
        // Resident attribute header is 24 bytes, data follows
        let data = attr_data.get(24..)?;
        if data.len() < 48 {
            return None;
        }

        let created = u64::from_le_bytes(data[0..8].try_into().ok()?);
        let modified = u64::from_le_bytes(data[8..16].try_into().ok()?);
        let mft_modified = u64::from_le_bytes(data[16..24].try_into().ok()?);
        let accessed = u64::from_le_bytes(data[24..32].try_into().ok()?);
        let file_attributes =
            FileAttributes::from_bits_truncate(u32::from_le_bytes(data[32..36].try_into().ok()?));

        let (owner_id, security_id, quota_charged, usn) = if data.len() >= 72 {
            (
                Some(u32::from_le_bytes(data[48..52].try_into().ok()?)),
                Some(u32::from_le_bytes(data[52..56].try_into().ok()?)),
                Some(u64::from_le_bytes(data[56..64].try_into().ok()?)),
                Some(u64::from_le_bytes(data[64..72].try_into().ok()?)),
            )
        } else {
            (None, None, None, None)
        };

        Some(Self {
            created,
            modified,
            mft_modified,
            accessed,
            file_attributes,
            owner_id,
            security_id,
            quota_charged,
            usn,
        })
    }

    /// Convert FILETIME (100ns since 1601-01-01) to Unix timestamp
    pub fn filetime_to_unix(filetime: u64) -> i64 {
        const EPOCH_DIFF: u64 = 11_644_473_600;
        const HUNDRED_NS_PER_SEC: u64 = 10_000_000;
        let secs = filetime / HUNDRED_NS_PER_SEC;
        secs.saturating_sub(EPOCH_DIFF) as i64
    }
}

/// $OBJECT_ID attribute (0x40) - Always resident
#[derive(Clone, Debug)]
pub struct ObjectId {
    pub object_id: [u8; 16],
    pub birth_volume_id: Option<[u8; 16]>,
    pub birth_object_id: Option<[u8; 16]>,
    pub domain_id: Option<[u8; 16]>,
}

impl ObjectId {
    pub fn parse(attr_data: &[u8]) -> Option<Self> {
        let data = attr_data.get(24..)?;
        if data.len() < 16 {
            return None;
        }

        let mut object_id = [0u8; 16];
        object_id.copy_from_slice(&data[0..16]);

        let birth_volume_id = if data.len() >= 32 {
            let mut v = [0u8; 16];
            v.copy_from_slice(&data[16..32]);
            Some(v)
        } else {
            None
        };

        let birth_object_id = if data.len() >= 48 {
            let mut v = [0u8; 16];
            v.copy_from_slice(&data[32..48]);
            Some(v)
        } else {
            None
        };

        let domain_id = if data.len() >= 64 {
            let mut v = [0u8; 16];
            v.copy_from_slice(&data[48..64]);
            Some(v)
        } else {
            None
        };

        Some(Self { object_id, birth_volume_id, birth_object_id, domain_id })
    }

    pub fn format_guid(guid: &[u8; 16]) -> String {
        alloc::format!(
            "{:08x}-{:04x}-{:04x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
            u32::from_le_bytes([guid[0], guid[1], guid[2], guid[3]]),
            u16::from_le_bytes([guid[4], guid[5]]),
            u16::from_le_bytes([guid[6], guid[7]]),
            guid[8],
            guid[9],
            guid[10],
            guid[11],
            guid[12],
            guid[13],
            guid[14],
            guid[15],
        )
    }
}

/// $SECURITY_DESCRIPTOR attribute (0x50) - Resident or non-resident
#[derive(Clone, Debug)]
pub struct SecurityDescriptor {
    pub raw: Vec<u8>,
    pub revision: u8,
    pub control: SecurityDescriptorControl,
    pub owner_offset: Option<u32>,
    pub group_offset: Option<u32>,
    pub dacl_offset: Option<u32>,
    pub sacl_offset: Option<u32>,
}

bitflags! {
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub struct SecurityDescriptorControl: u16 {
        const OWNER_DEFAULTED    = 0x0001;
        const GROUP_DEFAULTED    = 0x0002;
        const DACL_PRESENT       = 0x0004;
        const DACL_DEFAULTED     = 0x0008;
        const SACL_PRESENT       = 0x0010;
        const SACL_DEFAULTED     = 0x0020;
        const DACL_AUTO_INHERIT  = 0x0400;
        const SACL_AUTO_INHERIT  = 0x0800;
        const DACL_PROTECTED     = 0x1000;
        const SACL_PROTECTED     = 0x2000;
        const RM_CONTROL_VALID   = 0x4000;
        const SELF_RELATIVE      = 0x8000;
    }
}

impl SecurityDescriptor {
    pub fn parse(attr_data: &[u8], is_resident: bool) -> Option<Self> {
        let data = if is_resident {
            let value_offset = u16::from_le_bytes(attr_data.get(20..22)?.try_into().ok()?) as usize;
            attr_data.get(value_offset..)?
        } else {
            return None; // Non-resident requires reading data runs first
        };

        if data.len() < 20 {
            return None;
        }

        let revision = data[0];
        let control =
            SecurityDescriptorControl::from_bits_truncate(u16::from_le_bytes([data[2], data[3]]));
        let owner_offset = u32::from_le_bytes(data[4..8].try_into().ok()?);
        let group_offset = u32::from_le_bytes(data[8..12].try_into().ok()?);
        let sacl_offset = u32::from_le_bytes(data[12..16].try_into().ok()?);
        let dacl_offset = u32::from_le_bytes(data[16..20].try_into().ok()?);

        Some(Self {
            raw: data.to_vec(),
            revision,
            control,
            owner_offset: if owner_offset != 0 { Some(owner_offset) } else { None },
            group_offset: if group_offset != 0 { Some(group_offset) } else { None },
            dacl_offset: if dacl_offset != 0 { Some(dacl_offset) } else { None },
            sacl_offset: if sacl_offset != 0 { Some(sacl_offset) } else { None },
        })
    }
}

/// $REPARSE_POINT attribute (0xC0) - Symlinks, mount points, etc.
#[derive(Clone, Debug)]
pub struct ReparsePoint {
    pub reparse_tag: ReparseTag,
    pub reparse_data: Vec<u8>,
    pub substitute_name: Option<String>,
    pub print_name: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReparseTag {
    SymbolicLink,
    MountPoint,
    Other(u32),
}

impl ReparsePoint {
    const TAG_SYMLINK: u32 = 0xA000000C;
    const TAG_MOUNT_POINT: u32 = 0xA0000003;

    pub fn parse(attr_data: &[u8]) -> Option<Self> {
        let data = attr_data.get(24..)?;
        if data.len() < 8 {
            return None;
        }

        let tag = u32::from_le_bytes(data[0..4].try_into().ok()?);
        let data_length = u16::from_le_bytes(data[4..6].try_into().ok()?) as usize;
        let reparse_data = data.get(8..8usize.saturating_add(data_length))?.to_vec();
        
        let reparse_tag = match tag {
            Self::TAG_SYMLINK => ReparseTag::SymbolicLink,
            Self::TAG_MOUNT_POINT => ReparseTag::MountPoint,
            other => ReparseTag::Other(other),
        };

        let (substitute_name, print_name) = match reparse_tag {
            ReparseTag::SymbolicLink | ReparseTag::MountPoint => {
                Self::parse_name_buffer(&reparse_data, reparse_tag).unwrap_or((None, None))
            }
            _ => (None, None),
        };

        Some(Self { reparse_tag, reparse_data, substitute_name, print_name })
    }

    fn parse_name_buffer(data: &[u8], tag: ReparseTag) -> Option<(Option<String>, Option<String>)> {
        if data.len() < 8 {
            return None;
        }

        let sub_off = u16::from_le_bytes([data[0], data[1]]) as usize;
        let sub_len = u16::from_le_bytes([data[2], data[3]]) as usize;
        let prt_off = u16::from_le_bytes([data[4], data[5]]) as usize;
        let prt_len = u16::from_le_bytes([data[6], data[7]]) as usize;

        let buf_start = match tag {
            ReparseTag::SymbolicLink => 12, // Includes flags field
            _ => 8,
        };

        let sub = Self::decode_utf16(data.get(buf_start + sub_off..buf_start + sub_off + sub_len)?);
        let prt = Self::decode_utf16(data.get(buf_start + prt_off..buf_start + prt_off + prt_len)?);
        Some((Some(sub), Some(prt)))
    }

    fn decode_utf16(raw: &[u8]) -> String {
        let units: Vec<u16> =
            raw.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
        units.iter().map(|&c| if c < 128 { c as u8 as char } else { '?' }).collect()
    }
}

/// Alternate Data Stream ($DATA attribute with name)
#[derive(Clone, Debug)]
pub struct AlternateDataStream {
    pub name: String,
    pub size: u64,
    pub data: DataRun,
}

/// Volume geometry information
#[derive(Debug, Clone)]
pub struct VolumeInfo {
    pub sector_size: u16,
    pub cluster_size: u32,
    pub file_record_size: u32,
    pub mft_position: u64,
    pub serial_number: u64,
}

/// Options for creating new files/directories
#[derive(Debug, Clone)]
pub struct CreateOptions {
    pub is_directory: bool,
    pub data: Vec<u8>,
}

/// NTFS attribute type codes
#[repr(u32)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
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
