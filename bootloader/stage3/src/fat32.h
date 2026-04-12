#pragma once
#include <stdint.h>

// ── FAT32 on-disk structures ──────────────────────────────────────────────────

typedef struct {
    uint8_t  jump[3];
    uint8_t  oem[8];
    uint16_t bytes_per_sector;
    uint8_t  sectors_per_cluster;
    uint16_t reserved_sectors;
    uint8_t  num_fats;
    uint16_t root_entry_count;   // 0 for FAT32
    uint16_t total_sectors_16;
    uint8_t  media_type;
    uint16_t fat_size_16;        // 0 for FAT32
    uint16_t sectors_per_track;
    uint16_t num_heads;
    uint32_t hidden_sectors;
    uint32_t total_sectors_32;

    // FAT32 extended BPB
    uint32_t fat_size_32;
    uint16_t ext_flags;
    uint16_t fs_version;
    uint32_t root_cluster;       // Usually 2
    uint16_t fs_info;
    uint16_t backup_boot;
    uint8_t  reserved[12];
    uint8_t  drive_number;
    uint8_t  reserved1;
    uint8_t  boot_sig;
    uint32_t volume_id;
    uint8_t  volume_label[11];
    uint8_t  fs_type[8];
} __attribute__((packed)) Fat32BPB;

typedef struct {
    uint8_t  name[11];
    uint8_t  attributes;
    uint8_t  reserved;
    uint8_t  create_time_tenth;
    uint16_t create_time;
    uint16_t create_date;
    uint16_t access_date;
    uint16_t cluster_hi;
    uint16_t write_time;
    uint16_t write_date;
    uint16_t cluster_lo;
    uint32_t file_size;
} __attribute__((packed)) Fat32DirEntry;

typedef struct {
    uint8_t  order;
    uint16_t name1[5];
    uint8_t  attributes;   // Always 0x0F
    uint8_t  type;
    uint8_t  checksum;
    uint16_t name2[6];
    uint16_t cluster_lo;   // Always 0
    uint16_t name3[2];
} __attribute__((packed)) Fat32LFNEntry;

#define FAT32_ATTR_DIRECTORY  0x10
#define FAT32_ATTR_LFN        0x0F
#define FAT32_EOC             0x0FFFFFF8  // End of chain marker

// ── Driver API ────────────────────────────────────────────────────────────────

// fat32_init: parse BPB from the FAT32 partition base (bytes offset on disk)
// disk_base_lba: LBA of the first sector of the FAT32 partition
// read_sector_fn: caller-supplied function to read one 512-byte sector into buf
typedef void (*ReadSectorFn)(uint64_t lba, void *buf);

int  fat32_init(uint64_t partition_lba, ReadSectorFn read_fn);

// Find a file by 8.3 name (uppercase, e.g. "KERNEL  ELF") in the root dir.
// Returns the starting cluster (>= 2) or 0 on failure.
uint32_t fat32_find_root(const char *name83);

// Read the entire cluster chain starting at first_cluster into dest.
// Returns bytes read, or 0 on error.
uint64_t fat32_read_file(uint32_t first_cluster, void *dest, uint64_t max_bytes);