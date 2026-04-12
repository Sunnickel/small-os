#pragma once
#include <stdint.h>

// ─────────────────────────────────────────────────────────────────────────────
// Installer protocol
//
// Stage3 calls the build ELF with:
//   RDI = BootInfo*   (System-V AMD64)
//
// The build receives a fully populated BootInfo and must:
//   1. Read STAGE1.BIN, STAGE2.BIN, STAGE3.BIN from boot.img FAT32
//      (fat32_partition_lba, boot_disk == BOOT_DISK_ATA)
//   2. Read KERNEL.ELF from boot.img FAT32
//   3. Partition disk.img (virtio-blk) with GPT:
//        Partition 1: FAT32, DISK_FAT32_SIZE_LBA sectors, starts at DISK_FAT32_START_LBA
//        Partition 2: NTFS,  remainder
//   4. Write raw bootloader sectors to disk.img LBA 0..2047:
//        LBA 0   ← STAGE1.BIN  (512 bytes)
//        LBA 1   ← STAGE2.BIN  (up to 31 sectors)
//        LBA 32  ← STAGE3.BIN  (up to 64 sectors)
//   5. Format FAT32 partition on disk.img and write KERNEL.ELF to it
//   6. Format NTFS partition on disk.img (write NTFS boot record)
//   7. Reboot (or halt with a success message — user reboots manually)
//
// Disk geometry constants (must match stage3's FAT32_DISK_LBA):
// ─────────────────────────────────────────────────────────────────────────────

// GPT layout on disk.img
#define DISK_GPT_START_LBA      1
#define DISK_USABLE_START_LBA   2048        // standard GPT alignment

// FAT32 partition (partition 1)
#define DISK_FAT32_START_LBA    2048
#define DISK_FAT32_SIZE_MB      64
#define DISK_FAT32_SIZE_LBA     (DISK_FAT32_SIZE_MB * 1024 * 1024 / 512)  // = 131072

// NTFS partition (partition 2) starts immediately after FAT32
#define DISK_NTFS_START_LBA     (DISK_FAT32_START_LBA + DISK_FAT32_SIZE_LBA)

// Raw bootloader sector layout (written before GPT usable area)
#define DISK_STAGE1_LBA         0
#define DISK_STAGE2_LBA         1
#define DISK_STAGE3_LBA         32

// FAT32 filenames the build reads from boot.img (8.3, uppercase)
#define FNAME_STAGE1    "STAGE1  BIN"
#define FNAME_STAGE2    "STAGE2  BIN"
#define FNAME_STAGE3    "STAGE3  BIN"
#define FNAME_KERNEL    "KERNEL  ELF"
#define FNAME_INSTALLER "INSTALL ELF"