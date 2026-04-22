#include "debug.h"

#define COM1 0x3F8

static inline void outb(uint16_t port, uint8_t val) {
	__asm__ volatile (
	"outb %0, %1"
	:
	:
	"a"(val), "Nd"(port)
	)
	;
}

static inline uint8_t inb(uint16_t port) {
	uint8_t ret;
	__asm__ volatile (
	"inb %1, %0"
	:
	"=a"(ret)
	:
	"Nd"(port)
	)
	;
	return ret;
}

void serial_init(void) {
	outb(COM1 + 1, 0x00);
	outb(COM1 + 3, 0x80);
	outb(COM1 + 0, 0x03);
	outb(COM1 + 1, 0x00);
	outb(COM1 + 3, 0x03);
	outb(COM1 + 2, 0xC7);
	outb(COM1 + 4, 0x0B);
}

static int serial_is_transmit_empty(void) { return inb(COM1 + 5) & 0x20; }

void serial_putc(char c) {
	while (!serial_is_transmit_empty());
	outb(COM1, (uint8_t)c);
}

void serial_puts(const char *s) {
	while (*s)
		serial_putc(*s++);
}

static void print_hex(uint64_t v) {
	const char* hex = "0123456789ABCDEF";
	for (int i = 60; i >= 0; i -= 4)
		serial_putc(hex[(v >> i) & 0xF]);
}

void serial_puthex64(uint64_t v) { print_hex(v); }

void serial_puthex32(uint32_t v) { print_hex((uint64_t)v); }
