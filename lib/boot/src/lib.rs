#![no_std]
#![no_main]

#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct BootInfo {
    pub physical_memory_offset: u64,  // +0
    pub memory_map: u64,              // +8  (physical addr of E820 entries)
    pub memory_map_len: u64,          // +16
    pub framebuffer: FrameBufferInfo, // +24 (7 × u64 = 56 bytes)
    pub rsdp_addr: u64,               // +80
    pub fat32_partition_lba: u64,     // +88
    pub boot_disk: u64,               // +96
}

#[derive(Debug, Clone, Copy)]
#[repr(C, packed)]
pub struct E820Entry {
    pub base: u64,
    pub length: u64,
    pub entry_type: u32,
    pub acpi_attrs: u32, // extended attrs, often 0
}

#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct FrameBufferInfo {
    pub addr: u64,
    pub size: u64,
    pub width: u64,
    pub height: u64,
    pub stride: u64,
    pub bytes_per_pixel: u64,
    pub pixel_format: u64, // 0=RGB, 1=BGR
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum PixelFormat {
    Rgb = 0,
    Bgr = 1,
    Unknown = 0xFF,
}

#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct MemoryRegion {
    pub start: u64,
    pub end: u64,
    pub kind: MemoryRegionKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum MemoryRegionKind {
    Unknown = 0,         // E820 doesn't use 0
    Usable = 1,          // BIOS Type 1
    Reserved = 2,        // BIOS Type 2
    AcpiReclaimable = 3, // BIOS Type 3
    AcpiNvs = 4,         // BIOS Type 4
    BadMemory = 5,       // BIOS Type 5
    // You can define custom types for your own use starting higher up
    Bootloader = 100,
    FrameBuffer = 101,
}
