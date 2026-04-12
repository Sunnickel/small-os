#pragma once
#include <stdint.h>
#include "fat32.h"

// ── MBR ───────────────────────────────────────────────────────────────────────

#define MBR_SIGNATURE     0xAA55
#define MBR_PART_OFFSET   446       // offset of partition table in MBR
#define MBR_MAX_PARTS     4

// Partition type codes
#define PART_TYPE_EMPTY   0x00
#define PART_TYPE_NTFS    0x07      // NTFS / exFAT
#define PART_TYPE_GPT     0xEE      // GPT protective MBR entry

typedef struct {
    uint8_t  status;
    uint8_t  chs_first[3];
    uint8_t  type;
    uint8_t  chs_last[3];
    uint32_t lba_start;
    uint32_t lba_size;
} __attribute__((packed)) MbrPartEntry;

typedef struct {
    uint8_t       bootstrap[446];
    MbrPartEntry  parts[4];
    uint16_t      signature;        // Must be 0xAA55
} __attribute__((packed)) Mbr;

// ── GPT ───────────────────────────────────────────────────────────────────────

#define GPT_HEADER_LBA    1
#define GPT_SIGNATURE     0x5452415020494645ULL   // "EFI PART"
#define GPT_MAX_SCAN      128                     // max partition entries to scan

// NTFS / Basic Data partition GUID: {EBD0A0A2-B9E5-4433-87C0-68B6B72699C7}
// Stored little-endian in the first three fields.
#define GPT_BASIC_DATA_GUID_P1  0xEBD0A0A2
#define GPT_BASIC_DATA_GUID_P2  0xB9E54433
#define GPT_BASIC_DATA_GUID_P3  0x68B6B726  // bytes 8-11

typedef struct {
    uint64_t signature;
    uint32_t revision;
    uint32_t header_size;
    uint32_t header_crc32;
    uint32_t reserved;
    uint64_t my_lba;
    uint64_t alternate_lba;
    uint64_t first_usable_lba;
    uint64_t last_usable_lba;
    uint8_t  disk_guid[16];
    uint64_t partition_entry_lba;
    uint32_t num_partition_entries;
    uint32_t partition_entry_size;
    uint32_t partition_array_crc32;
} __attribute__((packed)) GptHeader;

typedef struct {
    uint8_t  type_guid[16];
    uint8_t  unique_guid[16];
    uint64_t start_lba;
    uint64_t end_lba;
    uint64_t attributes;
    uint16_t name[36];    // UTF-16LE
} __attribute__((packed)) GptEntry;

// ── NTFS boot sector (enough to identify the filesystem) ─────────────────────

#define NTFS_SIGNATURE  0x202020205346544EULL   // "NTFS    " (little-endian)
#define NTFS_SIG_OFFSET 3                        // byte offset in boot sector

// ── Result ────────────────────────────────────────────────────────────────────

typedef enum {
    PROBE_NO_DISK   = 0,   // blank / unreadable
    PROBE_NO_TABLE  = 1,   // no valid MBR or GPT
    PROBE_NO_NTFS   = 2,   // partition table present, no NTFS partition
    PROBE_NO_KERNEL = 3,   // NTFS partition found, KERNEL.ELF absent
    PROBE_FOUND     = 4,   // NTFS partition found + KERNEL.ELF confirmed
} ProbeResult;

typedef struct {
    ProbeResult result;
    uint64_t    ntfs_lba;   // LBA of the NTFS partition start (valid if result >= PROBE_NO_KERNEL)
} DiskProbe;

// ── API ───────────────────────────────────────────────────────────────────────

// Probe disk.img via read_fn.
// read_fn must read from disk.img (NOT boot.img — caller supplies the right fn).
// Returns a DiskProbe describing what was found.
DiskProbe disk_probe(ReadSectorFn read_fn);