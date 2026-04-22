#include "virtio_blk.h"

// MMIO base
static volatile uint32_t* mmio;

// Queue memory (must be 4KB aligned)
static uint8_t queue_mem[16384] __attribute__((aligned(4096)));

static vring_desc_t* desc;
static vring_avail_t* avail;
static vring_used_t* used;

static virtio_blk_req_t* req;
static uint8_t* status_byte;

static int initialized = 0;
static uint16_t last_used_idx = 0;

#define REG(off) (*(volatile uint32_t *)((uintptr_t)mmio + (off)))

// ------------------------- INIT -------------------------

int virtio_blk_init(uintptr_t base)
{
    mmio = (volatile uint32_t*)base;

    // Verify device
    if (REG(VIRTIO_MMIO_MAGIC) != 0x74726976) return 0; // "virt"
    if (REG(VIRTIO_MMIO_VERSION) != 2) return 0;
    if (REG(VIRTIO_MMIO_DEVICE_ID) != VIRTIO_ID_BLOCK) return 0;

    // Reset
    REG(VIRTIO_MMIO_STATUS) = 0;

    // ACK + DRIVER
    REG(VIRTIO_MMIO_STATUS) = VIRTIO_STATUS_ACK | VIRTIO_STATUS_DRIVER;

    // Feature negotiation (minimal: accept all)
    uint32_t features = REG(VIRTIO_MMIO_DEVICE_FEATURES);
    REG(VIRTIO_MMIO_DRIVER_FEATURES) = features;

    REG(VIRTIO_MMIO_STATUS) |= VIRTIO_STATUS_FEATURES_OK;
    if (!(REG(VIRTIO_MMIO_STATUS) & VIRTIO_STATUS_FEATURES_OK))
        return 0;

    // Queue setup
    REG(VIRTIO_MMIO_QUEUE_SEL) = 0;

    uint32_t max = REG(VIRTIO_MMIO_QUEUE_NUM_MAX);
    if (max < QUEUE_SIZE) return 0;

    REG(VIRTIO_MMIO_QUEUE_NUM) = QUEUE_SIZE;

    // vring layout
    uintptr_t base_ptr = (uintptr_t)queue_mem;

    desc = (vring_desc_t*)base_ptr;
    avail = (vring_avail_t*)(base_ptr + 0x1000);
    used = (vring_used_t*)(base_ptr + 0x2000);

    for (int i = 0; i < 16384; i++)
        queue_mem[i] = 0;

    req = (virtio_blk_req_t*)(base_ptr + 0x3000);
    status_byte = (uint8_t*)(base_ptr + 0x3008);

    // Legacy-style queue enable
    REG(VIRTIO_MMIO_QUEUE_PFN) = base_ptr >> 12;
    REG(VIRTIO_MMIO_QUEUE_READY) = 1;

    // DRIVER_OK
    REG(VIRTIO_MMIO_STATUS) |= VIRTIO_STATUS_DRIVER_OK;

    initialized = 1;
    return 1;
}

// ------------------------- READ -------------------------

int virtio_blk_read(uint64_t sector, void* buffer)
{
    if (!initialized) return 0;

    req->type = VIRTIO_BLK_T_IN;
    req->reserved = 0;
    req->sector = sector;

    *status_byte = 0xFF;

    // Descriptor chain
    desc[0].addr = (uint64_t)(uintptr_t)
    req;
    desc[0].len = sizeof(virtio_blk_req_t);
    desc[0].flags = VIRTQ_DESC_F_NEXT;
    desc[0].next = 1;

    desc[1].addr = (uint64_t)(uintptr_t)
    buffer;
    desc[1].len = 512;
    desc[1].flags = VIRTQ_DESC_F_WRITE | VIRTQ_DESC_F_NEXT;
    desc[1].next = 2;

    desc[2].addr = (uint64_t)(uintptr_t)
    status_byte;
    desc[2].len = 1;
    desc[2].flags = VIRTQ_DESC_F_WRITE;

    // Submit
    uint16_t idx = avail->idx % QUEUE_SIZE;
    avail->ring[idx] = 0;
    __asm__ volatile("" ::: "memory");
    avail->idx++;

    REG(VIRTIO_MMIO_QUEUE_NOTIFY) = 0;

    // Wait for completion
    while (used->idx == last_used_idx)
        __asm__ volatile("pause");

    last_used_idx = used->idx;

    return (*status_byte == 0) ? 1 : 0;
}

void virtio_blk_read_sector(void* buf)
{
    virtio_blk_read(0, buf);
}

void virtio_blk_read_sector_1(void* buf)
{
    virtio_blk_read(1, buf);
}