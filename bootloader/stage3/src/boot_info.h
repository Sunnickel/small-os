#pragma once
#include <stdint.h>

// BootInfo is written by stage2 at 0x5000 and extended by stage3 before
// handing off to the kernel or build.
//
// Offset map (each field = 8 bytes):
//   0   mmap_addr
//   8   mmap_count
//   16  fb_addr
//   24  fb_width
//   32  fb_height
//   40  fb_stride
//   48  fb_bpp
//   56  fat32_partition_lba   ← added by stage3
//   64  boot_disk             ← added by stage3 (0=ATA/boot.img, 1=virtio/disk.img)
typedef struct {
    uint64_t physical_memory_offset; // +0  — set to 0 for now (identity mapped)
    uint64_t memory_map;             // +8  — physical address of E820 entries
    uint64_t memory_map_len;         // +16 — number of entries

    // FrameBufferInfo (7 fields)
    uint64_t fb_addr;                // +24
    uint64_t fb_size;                // +32 — add this, was missing
    uint64_t fb_width;               // +40
    uint64_t fb_height;              // +48
    uint64_t fb_stride;              // +56
    uint64_t fb_bpp;                 // +64
    uint64_t fb_pixel_format;        // +72 — add this, was missing

    uint64_t rsdp_addr;              // +80

    // stage3 additions
    uint64_t fat32_partition_lba;    // +88
    uint64_t boot_disk;              // +96
} __attribute__((packed)) BootInfo;

// E820 memory map entry
typedef struct {
    uint64_t base;
    uint64_t length;
    uint32_t type;          // 1 = usable RAM
    uint32_t acpi_attrs;
} __attribute__((packed)) E820Entry;

#define MMAP_TYPE_USABLE   1
#define BOOT_DISK_ATA      0    // booted from boot.img via IDE
#define BOOT_DISK_VIRTIO   1    // booted from disk.img via virtio-blk