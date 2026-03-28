pub const SECTOR_SIZE: usize = 512;

pub const VIRTIO_BLK_T_IN: u32 = 0;
pub const VIRTIO_BLK_T_OUT: u32 = 1;
pub const VIRTIO_BLK_S_OK: u8 = 0;

pub const VIRTQ_DESC_F_NEXT: u16 = 1;
pub const VIRTQ_DESC_F_WRITE: u16 = 2;

// Common config offsets
pub const DEVICE_FEATURE_SELECT: usize = 0x00;
pub const DEVICE_FEATURE: usize = 0x04;
pub const DRIVER_FEATURE_SELECT: usize = 0x08;
pub const DRIVER_FEATURE: usize = 0x0C;
pub const DEVICE_STATUS: usize = 0x14;
pub const QUEUE_SELECT: usize = 0x16;
pub const QUEUE_SIZE: usize = 0x18;
pub const QUEUE_MSIX_VECTOR: usize = 0x1A;
pub const QUEUE_ENABLE: usize = 0x1C;
pub const QUEUE_DESC_LO: usize = 0x20;
pub const QUEUE_DESC_HI: usize = 0x24;
pub const QUEUE_DRIVER_LO: usize = 0x28;
pub const QUEUE_DRIVER_HI: usize = 0x2C;
pub const QUEUE_DEVICE_LO: usize = 0x30;
pub const QUEUE_DEVICE_HI: usize = 0x34;
