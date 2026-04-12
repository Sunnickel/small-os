#include "elf_loader.h"
#include "debug.h"
#include <stdint.h>

int elf_validate(const Elf64Ehdr *ehdr) {
    if (ehdr->magic    != ELF_MAGIC)       return 0;
    if (ehdr->ei_class != ELF_CLASS_64)    return 0;
    if (ehdr->ei_data  != ELF_DATA_LE)     return 0;
    if (ehdr->e_type   != ELF_TYPE_EXEC)   return 0;
    if (ehdr->e_machine != ELF_ARCH_X86_64) return 0;
    return 1;
}

uint64_t elf_load(const void *elf_data) {
    const uint8_t   *base = (const uint8_t *)elf_data;
    const Elf64Ehdr *ehdr = (const Elf64Ehdr *)base;

    if (!elf_validate(ehdr))
        return 0;

    const Elf64Phdr *phdrs =
        (const Elf64Phdr *)(base + ehdr->e_phoff);

    for (uint16_t i = 0; i < ehdr->e_phnum; i++) {
        const Elf64Phdr *ph = &phdrs[i];

        if (ph->p_type != PT_LOAD)
            continue;

        // Use p_paddr if set, otherwise fall back to p_vaddr
        uintptr_t dest_addr = ph->p_paddr ? ph->p_paddr : ph->p_vaddr;

        // Safety check: don't overwrite Stage 2/3 or BootInfo
        if (dest_addr < 0x20000 || dest_addr > 0xFFFFFFFF80000000) {
            serial_puts("ELF load address invalid: ");
            serial_puthex64(dest_addr);
            return 0;
        }

        uint8_t       *dst = (uint8_t *)dest_addr;
        const uint8_t *src = base + ph->p_offset;

        // Debug: show where we're loading
        serial_puts("Loading segment to ");
        serial_puthex64(dest_addr);
        serial_puts(" size ");
        serial_puthex64(ph->p_filesz);
        serial_puts("\n");

        for (uint64_t b = 0; b < ph->p_filesz; b++)
            dst[b] = src[b];

        for (uint64_t b = ph->p_filesz; b < ph->p_memsz; b++)
            dst[b] = 0;
    }

    serial_puts("Entry point: ");
    serial_puthex64(ehdr->e_entry);
    serial_puts("\n");

    return ehdr->e_entry;
}