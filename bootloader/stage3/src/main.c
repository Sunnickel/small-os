#include <stdint.h>
#include <stdbool.h>

#include "boot_info.h"
#include "fat32.h"
#include "elf_loader.h"
#include "debug.h"
#include "disk_probe.h"
#include "virtio_blk.h"

// ─────────────────────────────────────────────────────────────────────────────
// BOOT INFO
// ─────────────────────────────────────────────────────────────────────────────

#define BOOT_INFO_ADDR ((BootInfo*)0xFF00)

// ─────────────────────────────────────────────────────────────────────────────
// DISK CONFIG
// ─────────────────────────────────────────────────────────────────────────────

#define FAT32_BOOT_LBA 2048ULL
#define FAT32_DISK_LBA 2048ULL

#define KERNEL_NAME83    "KERNEL  ELF"
#define INSTALLER_NAME83 "INSTALL ELF"

// ─────────────────────────────────────────────────────────────────────────────
// MEMORY
// ─────────────────────────────────────────────────────────────────────────────

#define ELF_SCRATCH_ADDR ((void*)0x1000000)
#define ELF_MAX_BYTES    (32ULL * 1024 * 1024)

// ─────────────────────────────────────────────────────────────────────────────
// LOGGING
// ─────────────────────────────────────────────────────────────────────────────

static void log(const char* msg)
{
    serial_puts("[stage3] ");
    serial_puts(msg);
    serial_puts("\n");
}

static void panic(const char* msg)
{
    serial_puts("[PANIC] ");
    serial_puts(msg);
    serial_puts("\n");
    for (;;) __asm__ volatile("hlt");
}

// ─────────────────────────────────────────────────────────────────────────────
// ACPI (Stage3 responsibility)
// ─────────────────────────────────────────────────────────────────────────────

typedef struct {
    char signature[8];
    uint8_t checksum;
    char oem[6];
    uint8_t revision;
    uint32_t rsdt_addr;
} __attribute__((packed)) RSDP;

static void acpi_init(BootInfo* info)
{
    if (!info->rsdp_addr)
    {
        log("ACPI: no RSDP provided");
        return;
    }

    RSDP* rsdp = (RSDP*)(uintptr_t)info->rsdp_addr;

    if (__builtin_memcmp(rsdp->signature, "RSD PTR ", 8) != 0)
    {
        panic("ACPI: invalid RSDP");
    }

    log("ACPI: RSDP detected");

    // TODO:
    // - parse XSDT
    // - locate MADT (APIC)
    // - initialize interrupts
}

// ─────────────────────────────────────────────────────────────────────────────
// ELF LAUNCH
// ─────────────────────────────────────────────────────────────────────────────

static void launch_elf(const char* name83,
                       const char* label,
                       BootInfo* info,
                       uint64_t fat32_lba,
                       uint64_t boot_disk,
                       void (*read_sector)(uint64_t, void*))
{
    log("loading binary");

    info->fat32_partition_lba = fat32_lba;
    info->boot_disk = boot_disk;

    uint32_t cluster = fat32_find_root(name83);
    if (cluster < 2)
        panic("file not found");

    uint64_t size = fat32_read_file(cluster, ELF_SCRATCH_ADDR, ELF_MAX_BYTES);
    if (!size)
        panic("read failed");

    uint64_t entry = elf_load(ELF_SCRATCH_ADDR);
    if (!entry)
        panic("invalid ELF");

    log("jumping to kernel");

    KernelEntry fn = (KernelEntry)(uintptr_t)entry;
    fn(info);

    panic("kernel returned");
}

// ─────────────────────────────────────────────────────────────────────────────
// DISK SELECTION
// ─────────────────────────────────────────────────────────────────────────────

static void boot_from_disk(BootInfo* info)
{
    log("checking installed disk");

    DiskProbe probe = disk_probe(virtio_blk_read_sector_1);

    if (probe.result == PROBE_FOUND)
    {
        log("disk.img detected");

        if (!fat32_init(FAT32_DISK_LBA, virtio_blk_read_sector_1))
            panic("FAT32 disk init failed");

        launch_elf(KERNEL_NAME83,
                   "kernel",
                   info,
                   FAT32_DISK_LBA,
                   BOOT_DISK_VIRTIO,
                   virtio_blk_read_sector_1);
    }
}

static void boot_from_usb(BootInfo* info)
{
    log("booting installer");

    if (!fat32_init(FAT32_BOOT_LBA, virtio_blk_read_sector))
        panic("FAT32 boot init failed");

    launch_elf(INSTALLER_NAME83,
               "installer",
               info,
               FAT32_BOOT_LBA,
               BOOT_DISK_VIRTIO,
               virtio_blk_read_sector);
}

// ─────────────────────────────────────────────────────────────────────────────
// ENTRY
// ─────────────────────────────────────────────────────────────────────────────

void stage3_main(BootInfo* boot_info)
{
    serial_init();

    log("stage3 start");

    // 1. ACPI INIT (IMPORTANT)
    acpi_init(boot_info);

    // 2. STORAGE INIT
    if (!virtio_blk_init())
        panic("no virtio disk");

    // 3. BOOT LOGIC
    boot_from_disk(boot_info);
    boot_from_usb(boot_info);

    panic("no boot path");
}