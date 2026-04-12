#include "virtio_blk.h"
#include "debug.h"
#include <stdint.h>
#include <stddef.h>

// PCI
#define PCI_CFG_ADDR  0xCF8
#define PCI_CFG_DATA  0xCFC
#define VIRTIO_VENDOR 0x1AF4
#define VIRTIO_BLK_LEGACY 0x1001  // Transitional device (supports legacy)
#define VIRTIO_BLK_MODERN 0x1042  // Modern only (not supported by this code)

// Legacy Register Offsets (I/O space)
#define REG_DEVICE_FEATURES  0x00
#define REG_GUEST_FEATURES   0x04
#define REG_QUEUE_PFN        0x08
#define REG_QUEUE_NUM        0x0C
#define REG_QUEUE_SEL        0x0E
#define REG_QUEUE_NOTIFY     0x10
#define REG_STATUS           0x12
#define REG_ISR              0x13

// Status bits
#define STATUS_ACKNOWLEDGE  0x01
#define STATUS_DRIVER       0x02
#define STATUS_DRIVER_OK    0x04
#define STATUS_FEATURES_OK  0x08

// Virtqueue
#define QUEUE_SIZE 256
#define DESC_F_NEXT  1
#define DESC_F_WRITE 2

// Request types
#define VIRTIO_BLK_T_IN  0

// Request struct
struct virtio_blk_req {
    uint32_t type;
    uint32_t reserved;
    uint64_t sector;
} __attribute__((packed));

struct vring_desc {
    uint64_t addr;
    uint32_t len;
    uint16_t flags;
    uint16_t next;
} __attribute__((packed));

struct vring_avail {
    uint16_t flags;
    uint16_t idx;
    uint16_t ring[QUEUE_SIZE];
} __attribute__((packed));

struct vring_used_elem {
    uint32_t id;
    uint32_t len;
} __attribute__((packed));

struct vring_used {
    uint16_t flags;
    uint16_t idx;
    struct vring_used_elem ring[QUEUE_SIZE];
} __attribute__((packed));

// State
static uint32_t io_base[2] = {0, 0};
static uint8_t  ready[2]   = {0, 0};
static int      dev_count   = 0;

// Queue memory (4KB aligned)
static uint8_t queue_mem[2][16384] __attribute__((aligned(4096)));
static struct vring_desc  *desc[2];
static struct vring_avail *avail[2];
static struct vring_used  *used[2];

static struct virtio_blk_req req_hdr[2];
static uint8_t req_status[2];

// PCI helpers
static uint32_t pci_read(uint8_t bus, uint8_t slot, uint8_t func, uint8_t offset) {
    uint32_t addr = (1u << 31) | (bus << 16) | (slot << 11) | (func << 8) | (offset & 0xFC);
    uint32_t data;

    __asm__ volatile (
        "movl $0xCF8, %%edx\n\t"
        "outl %%eax, %%dx\n\t"
        "movl $0xCFC, %%edx\n\t"
        "inl %%dx, %%eax"
        : "=a"(data)      // Output: data in EAX
        : "a"(addr)       // Input: address in EAX
        : "edx", "memory" // Clobbers: EDX
    );

    return data;
}

// I/O helpers
static inline void outl_p(uint16_t port, uint32_t val) {
    __asm__ volatile("outl %0, %1" :: "a"(val), "Nd"(port));
}
static inline uint32_t inl_p(uint16_t port) {
    uint32_t val;
    __asm__ volatile("inl %1, %0" : "=a"(val) : "Nd"(port));
    return val;
}
static inline void outw_p(uint16_t port, uint16_t val) {
    __asm__ volatile("outw %0, %1" :: "a"(val), "Nd"(port));
}
static inline void outb_p(uint16_t port, uint8_t val) {
    __asm__ volatile("outb %0, %1" :: "a"(val), "Nd"(port));
}
static inline uint8_t inb_p(uint16_t port) {
    uint8_t val;
    __asm__ volatile("inb %1, %0" : "=a"(val) : "Nd"(port));
    return val;
}

// Virtio register access
#define virtio_reg(offset) (io_base + (offset))

int virtio_blk_init(void) {
    dev_count = 0;

    for (uint8_t slot = 0; slot < 32; slot++) {
        uint32_t vendor_device = pci_read(0, slot, 0, 0);
        uint16_t vendor = vendor_device & 0xFFFF;
        uint16_t device = (vendor_device >> 16) & 0xFFFF;

        if (vendor != VIRTIO_VENDOR) continue;
        if (device != VIRTIO_BLK_LEGACY) continue;
        if (dev_count >= 2) break;

        int d = dev_count;

        serial_puts("[virtio] Found virtio-blk at slot ");
        serial_puthex64(slot);
        serial_puts("\n");

        uint32_t bar0 = pci_read(0, slot, 0, 0x10);
        if (!(bar0 & 1)) continue;

        io_base[d] = bar0 & ~0x3;

        // Reset + init (same as before but use io_base[d])
        outb_p(io_base[d] + REG_STATUS, 0);
        outb_p(io_base[d] + REG_STATUS, STATUS_ACKNOWLEDGE);
        outb_p(io_base[d] + REG_STATUS, STATUS_ACKNOWLEDGE | STATUS_DRIVER);

        outw_p(io_base[d] + REG_QUEUE_SEL, 0);
        uint16_t qsize = inl_p(io_base[d] + REG_QUEUE_NUM) & 0xFFFF;
        if (qsize < QUEUE_SIZE) continue;

        desc[d]  = (struct vring_desc *)queue_mem[d];
        avail[d] = (struct vring_avail *)(queue_mem[d] + sizeof(struct vring_desc) * QUEUE_SIZE);
        size_t used_offset = (sizeof(struct vring_desc) * QUEUE_SIZE +
                             sizeof(struct vring_avail) + 4095) & ~4095;
        used[d] = (struct vring_used *)(queue_mem[d] + used_offset);

        for (int i = 0; i < 16384; i++) queue_mem[d][i] = 0;

        uint32_t pfn = (uint32_t)((uintptr_t)queue_mem[d] >> 12);
        outl_p(io_base[d] + REG_QUEUE_PFN, pfn);
        outb_p(io_base[d] + REG_STATUS,
               STATUS_ACKNOWLEDGE | STATUS_DRIVER | STATUS_DRIVER_OK);

        serial_puts("[virtio] Device ");
        serial_puthex32(d);
        serial_puts(" ready\n");

        ready[d] = 1;
        dev_count++;
    }

    return dev_count > 0;
}

static void virtio_blk_read(int d, uint64_t lba, void *buf) {
    if (!ready[d]) return;

    req_hdr[d].type     = VIRTIO_BLK_T_IN;
    req_hdr[d].reserved = 0;
    req_hdr[d].sector   = lba;
    req_status[d]       = 0xFF;

    desc[d][0].addr  = (uint64_t)(uintptr_t)&req_hdr[d];
    desc[d][0].len   = sizeof(req_hdr[d]);
    desc[d][0].flags = DESC_F_NEXT;
    desc[d][0].next  = 1;

    desc[d][1].addr  = (uint64_t)(uintptr_t)buf;
    desc[d][1].len   = 512;
    desc[d][1].flags = DESC_F_WRITE | DESC_F_NEXT;
    desc[d][1].next  = 2;

    desc[d][2].addr  = (uint64_t)(uintptr_t)&req_status[d];
    desc[d][2].len   = 1;
    desc[d][2].flags = DESC_F_WRITE;
    desc[d][2].next  = 0;

    avail[d]->ring[avail[d]->idx % QUEUE_SIZE] = 0;
    avail[d]->idx++;

    __asm__ volatile("" ::: "memory");
    outw_p(io_base[d] + REG_QUEUE_NOTIFY, 0);

    uint16_t used_idx = used[d]->idx;
    while (used[d]->idx == used_idx) __asm__ volatile("pause");
}

void virtio_blk_read_sector(uint64_t lba, void *buf)      { virtio_blk_read(0, lba, buf); }
void virtio_blk_read_sector_1(uint64_t lba, void *buf)    { virtio_blk_read(1, lba, buf); }