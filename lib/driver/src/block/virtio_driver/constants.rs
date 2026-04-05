pub const SECTOR_SIZE: usize = 512;

// VirtIO PCI capability types
pub const PCI_CAP_VENDOR: u8 = 0x09;
pub const VIRTIO_PCI_CAP_COMMON_CFG: u8 = 1;
pub const VIRTIO_PCI_CAP_NOTIFY_CFG: u8 = 2;
pub const VIRTIO_PCI_CAP_ISR_CFG: u8 = 3;
pub const VIRTIO_PCI_CAP_DEVICE_CFG: u8 = 4;

// Status bits
pub const VIRTIO_STATUS_ACKNOWLEDGE: u8 = 1;
pub const VIRTIO_STATUS_DRIVER: u8 = 2;
pub const VIRTIO_STATUS_DRIVER_OK: u8 = 4;
pub const VIRTIO_STATUS_FEATURES_OK: u8 = 8;

// Common config offsets
pub const COMMON_DEVICE_FEATURE_SELECT: usize = 0x00;
pub const COMMON_DEVICE_FEATURE: usize = 0x04;
pub const COMMON_DRIVER_FEATURE_SELECT: usize = 0x08;
pub const COMMON_DRIVER_FEATURE: usize = 0x0C;
pub const COMMON_CONFIG_MSIX_VECTOR: usize = 0x10;
pub const COMMON_NUM_QUEUES: usize = 0x12;
pub const COMMON_DEVICE_STATUS: usize = 0x14;
pub const COMMON_CONFIG_GENERATION: usize = 0x15;
pub const COMMON_QUEUE_SELECT: usize = 0x16;
pub const COMMON_QUEUE_SIZE: usize = 0x18;
pub const COMMON_QUEUE_MSIX_VECTOR: usize = 0x1A;
pub const COMMON_QUEUE_ENABLE: usize = 0x1C;
pub const COMMON_QUEUE_NOTIFY_OFF: usize = 0x1E;
pub const COMMON_QUEUE_DESC_LOW: usize = 0x20;
pub const COMMON_QUEUE_DESC_HIGH: usize = 0x24;
pub const COMMON_QUEUE_AVAIL_LOW: usize = 0x28;
pub const COMMON_QUEUE_AVAIL_HIGH: usize = 0x2C;
pub const COMMON_QUEUE_USED_LOW: usize = 0x30;
pub const COMMON_QUEUE_USED_HIGH: usize = 0x34;

// Descriptor flags
pub const VIRTQ_DESC_F_NEXT: u16 = 1;
pub const VIRTQ_DESC_F_WRITE: u16 = 2;

// Block request types
pub const VIRTIO_BLK_T_IN: u32 = 0;
pub const VIRTIO_BLK_T_OUT: u32 = 1;
pub const VIRTIO_BLK_S_OK: u8 = 0;
