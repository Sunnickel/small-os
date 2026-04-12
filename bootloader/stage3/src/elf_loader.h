#pragma once
#include <stdint.h>
#include "boot_info.h"

// ── ELF64 on-disk structures ──────────────────────────────────────────────────

#define ELF_MAGIC        0x464C457F   // "\x7FELF"
#define ELF_CLASS_64     2
#define ELF_DATA_LE      1
#define ELF_TYPE_EXEC    2
#define ELF_ARCH_X86_64  0x3E

#define PT_LOAD          1
#define PF_X             0x1
#define PF_W             0x2
#define PF_R             0x4

typedef struct {
    uint32_t magic;
    uint8_t  ei_class;
    uint8_t  ei_data;
    uint8_t  ei_version;
    uint8_t  ei_osabi;
    uint8_t  ei_abiversion;
    uint8_t  ei_pad[7];
    uint16_t e_type;
    uint16_t e_machine;
    uint32_t e_version;
    uint64_t e_entry;
    uint64_t e_phoff;
    uint64_t e_shoff;
    uint32_t e_flags;
    uint16_t e_ehsize;
    uint16_t e_phentsize;
    uint16_t e_phnum;
    uint16_t e_shentsize;
    uint16_t e_shnum;
    uint16_t e_shstrndx;
} __attribute__((packed)) Elf64Ehdr;

typedef struct {
    uint32_t p_type;
    uint32_t p_flags;
    uint64_t p_offset;
    uint64_t p_vaddr;
    uint64_t p_paddr;
    uint64_t p_filesz;
    uint64_t p_memsz;
    uint64_t p_align;
} __attribute__((packed)) Elf64Phdr;

// ── Loader API ────────────────────────────────────────────────────────────────

// Validate ELF header. Returns 1 if valid x86-64 ELF executable, 0 otherwise.
int elf_validate(const Elf64Ehdr *ehdr);

// Load all PT_LOAD segments from elf_data into their p_vaddr destinations.
// Zero-fills BSS (p_memsz > p_filesz).
// Returns the entry point virtual address, or 0 on error.
uint64_t elf_load(const void *elf_data);

// Typedef for the kernel entry point.
// Calling convention: RDI = BootInfo*  (System-V AMD64 ABI)
typedef void (*KernelEntry)(BootInfo *boot_info);