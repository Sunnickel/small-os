#include "disk_probe.h"
#include "fat32.h"
#include "debug.h"
#include <stdint.h>

// ─────────────────────────────────────────────────────────────────────────────
// Helpers
// ─────────────────────────────────────────────────────────────────────────────

static uint8_t g_sector[512] __attribute__((aligned(4)));

static void read(ReadSectorFn fn, uint64_t lba) {
    fn(lba, g_sector);
}

// Check whether the boot sector at `part_lba` has the NTFS OEM signature.
static int is_ntfs_boot(ReadSectorFn fn, uint64_t part_lba) {
    fn(part_lba, g_sector);
    // "NTFS    " starts at byte 3 in the BPB
    const uint8_t ntfs_sig[8] = { 'N','T','F','S',' ',' ',' ',' ' };
    for (int i = 0; i < 8; i++)
        if (g_sector[3 + i] != ntfs_sig[i])
            return 0;
    return 1;
}

// Lightweight NTFS $MFT scan: look for KERNEL.ELF in the root directory.
//
// Full NTFS parsing is complex (MFT records, attribute lists, etc.).
// We use a practical shortcut that works for freshly-installed single-file
// roots: scan the first N sectors of the partition for the UTF-16LE string
// "KERNEL.ELF" which will appear in the $FILE_NAME attribute of any MFT
// record for that file.
//
// This covers the common case without a full NTFS driver. For a production
// bootloader you would parse MFT record 5 ($Root) properly.
#define NTFS_SCAN_SECTORS 512   // scan first 256 KB of NTFS partition

static int ntfs_has_kernel(ReadSectorFn fn, uint64_t part_lba) {
    // UTF-16LE bytes for "KERNEL.ELF"
    static const uint8_t kernel_u16[] = {
        'K',0, 'E',0, 'R',0, 'N',0, 'E',0, 'L',0,
        '.',0, 'E',0, 'L',0, 'F',0
    };
    const int klen = sizeof(kernel_u16);

    for (uint32_t s = 0; s < NTFS_SCAN_SECTORS; s++) {
        fn(part_lba + s, g_sector);

        // Sliding window search for the UTF-16LE filename
        for (int i = 0; i <= 512 - klen; i++) {
            int match = 1;
            for (int j = 0; j < klen; j++) {
                if (g_sector[i + j] != kernel_u16[j]) { match = 0; break; }
            }
            if (match) return 1;
        }
    }
    return 0;
}

// ─────────────────────────────────────────────────────────────────────────────
// MBR path
// ─────────────────────────────────────────────────────────────────────────────

static DiskProbe probe_mbr(ReadSectorFn fn) {
    DiskProbe result = { PROBE_NO_NTFS, 0 };

    Mbr *mbr = (Mbr *)g_sector;   // g_sector already holds LBA 0

    for (int i = 0; i < MBR_MAX_PARTS; i++) {
        MbrPartEntry *p = &mbr->parts[i];
        if (p->type == PART_TYPE_EMPTY) continue;
        if (p->lba_start == 0)          continue;

        if (p->type == PART_TYPE_NTFS) {
            uint64_t lba = (uint64_t)p->lba_start;
            serial_puts("[probe] MBR: NTFS partition at LBA ");
            serial_puthex64(lba);
            serial_puts("\n");

            if (!is_ntfs_boot(fn, lba)) continue;   // signature mismatch

            result.ntfs_lba = lba;
            result.result   = ntfs_has_kernel(fn, lba)
                              ? PROBE_FOUND : PROBE_NO_KERNEL;
            return result;
        }
    }

    return result;
}

// ─────────────────────────────────────────────────────────────────────────────
// GPT path
// ─────────────────────────────────────────────────────────────────────────────

// Basic Data / NTFS GUID (mixed-endian as stored on disk):
//   EBD0A0A2-B9E5-4433-87C0-68B6B72699C7
static const uint8_t BASIC_DATA_GUID[16] = {
    0xA2, 0xA0, 0xD0, 0xEB,   // time_low (LE)
    0xE5, 0xB9,               // time_mid (LE)
    0x33, 0x44,               // time_hi  (LE)
    0x87, 0xC0,               // clock_seq (BE)
    0x68, 0xB6, 0xB7, 0x26, 0x99, 0xC7
};

static int guid_match(const uint8_t *a, const uint8_t *b) {
    for (int i = 0; i < 16; i++)
        if (a[i] != b[i]) return 0;
    return 1;
}

static uint8_t g_gpt_sector[512] __attribute__((aligned(4)));

static DiskProbe probe_gpt(ReadSectorFn fn) {
    DiskProbe result = { PROBE_NO_NTFS, 0 };

    fn(GPT_HEADER_LBA, g_gpt_sector);
    GptHeader *hdr = (GptHeader *)g_gpt_sector;

    if (hdr->signature != GPT_SIGNATURE) {
        result.result = PROBE_NO_TABLE;
        return result;
    }

    serial_puts("[probe] GPT header valid, scanning ");
    serial_puthex32(hdr->num_partition_entries);
    serial_puts(" entries\n");

    uint64_t entry_lba  = hdr->partition_entry_lba;
    uint32_t entry_size = hdr->partition_entry_size;
    uint32_t count      = hdr->num_partition_entries;
    if (count > GPT_MAX_SCAN) count = GPT_MAX_SCAN;

    uint8_t entry_buf[512] __attribute__((aligned(4)));
    uint32_t entries_per_sector = 512 / entry_size;

    for (uint32_t i = 0; i < count; i++) {
        uint32_t sector_idx  = i / entries_per_sector;
        uint32_t sector_off  = (i % entries_per_sector) * entry_size;

        fn(entry_lba + sector_idx, entry_buf);
        GptEntry *e = (GptEntry *)(entry_buf + sector_off);

        // Skip empty entries
        int all_zero = 1;
        for (int b = 0; b < 16; b++) if (e->type_guid[b]) { all_zero = 0; break; }
        if (all_zero) continue;

        if (guid_match(e->type_guid, BASIC_DATA_GUID)) {
            uint64_t lba = e->start_lba;
            serial_puts("[probe] GPT: Basic Data partition at LBA ");
            serial_puthex64(lba);
            serial_puts("\n");

            if (!is_ntfs_boot(fn, lba)) continue;

            result.ntfs_lba = lba;
            result.result   = ntfs_has_kernel(fn, lba)
                              ? PROBE_FOUND : PROBE_NO_KERNEL;
            return result;
        }
    }

    return result;
}

// ─────────────────────────────────────────────────────────────────────────────
// Public API
// ─────────────────────────────────────────────────────────────────────────────

DiskProbe disk_probe(ReadSectorFn fn) {
    DiskProbe result = { PROBE_NO_DISK, 0 };

    // Read LBA 0 once; reused by both MBR check and first-sector checks
    fn(0, g_sector);

    Mbr *mbr = (Mbr *)g_sector;

    // Blank disk — LBA 0 is all zeros
    int all_zero = 1;
    for (int i = 0; i < 512; i++) if (g_sector[i]) { all_zero = 0; break; }
    if (all_zero) {
        serial_puts("[probe] disk is blank\n");
        return result;
    }

    // No valid MBR signature → treat as unpartitioned
    if (mbr->signature != MBR_SIGNATURE) {
        serial_puts("[probe] no MBR signature\n");
        result.result = PROBE_NO_TABLE;
        return result;
    }

    // GPT protective MBR?
    if (mbr->parts[0].type == PART_TYPE_GPT) {
        serial_puts("[probe] GPT protective MBR detected\n");
        return probe_gpt(fn);
    }

    // Classic MBR
    serial_puts("[probe] MBR partition table detected\n");
    return probe_mbr(fn);
}