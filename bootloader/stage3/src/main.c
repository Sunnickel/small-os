#include <stdint.h>
#include "boot_info.h"
#include "fat32.h"
#include "elf_loader.h"
#include "debug.h"
#include "disk_probe.h"
#include "virtio_blk.h"

// ─────────────────────────────────────────────────────────────────────────────
// CONSTANTS
// ─────────────────────────────────────────────────────────────────────────────

#define BOOT_INFO_ADDR  ((BootInfo *)0xFF00)

// FAT32 partition LBA on boot.img (IDE primary, standard layout).
#define FAT32_BOOT_LBA       2048ULL

// FAT32 partition LBA on disk.img (first GPT partition, written by build).
// Must match DISK_FAT32_START_LBA in the build.
#define FAT32_DISK_LBA       2048ULL

// 8.3 filenames on the FAT32 partition (uppercase, space-padded to 11 chars).
#define KERNEL_NAME83        "KERNEL  ELF"   // KERNEL.ELF
#define INSTALLER_NAME83     "INSTALL ELF"   // INSTALL.ELF
#define STAGE1_NAME83        "STAGE1  BIN"   // STAGE1.BIN
#define STAGE2_NAME83        "STAGE2  BIN"   // STAGE2.BIN
#define STAGE3_NAME83        "STAGE3  BIN"   // STAGE3.BIN

// Scratch buffer for ELF loading — large enough for kernel or build.
#define ELF_SCRATCH_ADDR  ((void *)0x1000000)
#define ELF_MAX_BYTES        (32ULL * 1024 * 1024)

// ─────────────────────────────────────────────────────────────────────────────
// HELPERS
// ─────────────────────────────────────────────────────────────────────────────

static void log(const char* msg)
{
    serial_puts("[stage3] ");
    serial_puts(msg);
    serial_puts("\n");
}

static void panic(const char* msg)
{
    serial_puts(" - [PANIC] - ");
    serial_puts(msg);
    serial_puts(" - [PANIC] - \n");
    for (;;) __asm__ volatile("hlt");
}

// ─────────────────────────────────────────────────────────────────────────────
// ATA PIO — boot.img (IDE index 0)
// ─────────────────────────────────────────────────────────────────────────────

static inline void outb(uint16_t p, uint8_t v) { __asm__ volatile("outb %0,%1"::"a"(v),"Nd"(p)); }
static inline void outw(uint16_t p, uint16_t v) { __asm__ volatile("outw %0,%1"::"a"(v),"Nd"(p)); }

static inline uint8_t inb(uint16_t p)
{
    uint8_t r;
    __asm__ volatile("inb %1,%0":"=a"(r):"Nd"(p));
    return r;
}

static inline uint16_t inw(uint16_t p)
{
    uint16_t r;
    __asm__ volatile("inw %1,%0":"=a"(r):"Nd"(p));
    return r;
}

#define ATA_DATA       0x1F0
#define ATA_SECCOUNT   0x1F2
#define ATA_LBA_LO     0x1F3
#define ATA_LBA_MID    0x1F4
#define ATA_LBA_HI     0x1F5
#define ATA_DRIVE_HEAD 0x1F6
#define ATA_CMD        0x1F7
#define ATA_STATUS     0x1F7
#define ATA_CMD_READ   0x20
#define ATA_BSY        0x80
#define ATA_DRQ        0x08


static void ata_read_sector(uint64_t lba, void* buf)
{
    while (inb(ATA_STATUS) & ATA_BSY);
    outb(ATA_DRIVE_HEAD, (uint8_t)(0xE0 | ((lba >> 24) & 0x0F)));

    inb(0x3F6);
    inb(0x3F6);
    inb(0x3F6);
    inb(0x3F6);

    while (inb(ATA_STATUS) & ATA_BSY);
    outb(0x1F1, 0x00);
    outb(ATA_SECCOUNT, 1);
    outb(ATA_LBA_LO, (uint8_t)(lba));
    outb(ATA_LBA_MID, (uint8_t)(lba >> 8));
    outb(ATA_LBA_HI, (uint8_t)(lba >> 16));
    outb(ATA_CMD, ATA_CMD_READ);

    inb(0x3F6);
    inb(0x3F6);
    inb(0x3F6);
    inb(0x3F6);

    while (inb(ATA_STATUS) & ATA_BSY);
    uint8_t status = inb(ATA_STATUS);
    if (status & 0x01) panic("ATA error after READ command");
    while (!(inb(ATA_STATUS) & ATA_DRQ));
    uint16_t* dst = (uint16_t*)buf;
    for (int i = 0; i < 256; i++) dst[i] = inw(ATA_DATA);
}

static void launch_elf(const char* name83, const char* label,
                       BootInfo* info, uint64_t fat32_lba, uint64_t boot_disk)
{
    serial_puts("[stage3] loading ");
    serial_puts(label);
    serial_puts("\n");

    // Record which disk/partition we booted from so the target can use it
    info->fat32_partition_lba = fat32_lba;
    info->boot_disk = boot_disk;

    uint32_t cluster = fat32_find_root(name83);
    if (cluster < 2)
    {
        serial_puts("[stage3] not found: ");
        serial_puts(name83);
        serial_puts("\n");
        panic("ELF not found on FAT32");
    }

    uint64_t bytes = fat32_read_file(cluster, ELF_SCRATCH_ADDR, ELF_MAX_BYTES);
    if (bytes == 0) panic("fat32_read_file returned 0 bytes");

    serial_puts("[stage3] ");
    serial_puthex64(bytes);
    serial_puts(" bytes read\n");

    uint64_t entry = elf_load(ELF_SCRATCH_ADDR);
    if (entry == 0) panic("ELF load failed");

    serial_puts("[stage3] entry=");
    serial_puthex64(entry);
    serial_puts("\n");

    KernelEntry fn = (KernelEntry)(uintptr_t)entry;
    fn(info);
    panic("ELF returned");
}

// ─────────────────────────────────────────────────────────────────────────────
// ENTRY POINT
// stage2: mov rdi, 0xFF00 ; jmp 0x200000
// ─────────────────────────────────────────────────────────────────────────────
void stage3_main(BootInfo* boot_info)
{
    serial_init();
    log("stage3 starting");

    if (!virtio_blk_init())
        panic("no virtio-blk devices found");

    // Check disk.img (device 1) for installed kernel
    DiskProbe probe = disk_probe(virtio_blk_read_sector_1);
    if (probe.result == PROBE_FOUND)
    {
        log("installed system detected — booting kernel from disk.img");
        if (!fat32_init(FAT32_DISK_LBA, virtio_blk_read_sector_1))
            panic("FAT32 init failed on disk.img");
        launch_elf(KERNEL_NAME83, "kernel", boot_info,
                   FAT32_DISK_LBA, BOOT_DISK_VIRTIO);
    }

    // Fall through to boot.img (device 0) — run installer
    log("disk.img not installed — running installer from boot.img");
    if (!fat32_init(FAT32_BOOT_LBA, virtio_blk_read_sector))
        panic("FAT32 init failed on boot.img");
    launch_elf(INSTALLER_NAME83, "installer", boot_info,
               FAT32_BOOT_LBA, BOOT_DISK_VIRTIO);

    panic("unreachable");
}
