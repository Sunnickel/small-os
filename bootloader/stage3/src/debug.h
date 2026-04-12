#pragma once
#include <stdint.h>
#include "boot_info.h"

// Early debug output over COM1 (0x3F8) – always available before kernel
void serial_init(void);
void serial_putc(char c);
void serial_puts(const char *s);
void serial_puthex64(uint64_t v);
void serial_puthex32(uint32_t v);
