#ifndef VIRTIO_BLK_H
#define VIRTIO_BLK_H

#include <stdint.h>
#include <stddef.h>

#define QUEUE_SIZE 256

// VirtIO MMIO registers
#define VIRTIO_MMIO_MAGIC          0x000
#define VIRTIO_MMIO_VERSION        0x004
#define VIRTIO_MMIO_DEVICE_ID      0x008
#define VIRTIO_MMIO_VENDOR_ID      0x00C
#define VIRTIO_MMIO_DEVICE_FEATURES 0x010
#define VIRTIO_MMIO_DRIVER_FEATURES 0x020
#define VIRTIO_MMIO_QUEUE_SEL      0x030
#define VIRTIO_MMIO_QUEUE_NUM_MAX  0x034
#define VIRTIO_MMIO_QUEUE_NUM      0x038
#define VIRTIO_MMIO_QUEUE_PFN      0x040
#define VIRTIO_MMIO_QUEUE_READY    0x044
#define VIRTIO_MMIO_STATUS         0x070
#define VIRTIO_MMIO_QUEUE_NOTIFY   0x050

// Status bits
#define VIRTIO_STATUS_ACK         0x01
#define VIRTIO_STATUS_DRIVER      0x02
#define VIRTIO_STATUS_DRIVER_OK   0x04
#define VIRTIO_STATUS_FEATURES_OK 0x08

// Device ID
#define VIRTIO_ID_BLOCK 2

// Request types
#define VIRTIO_BLK_T_IN  0
#define VIRTIO_BLK_T_OUT 1

// Descriptor flags
#define VIRTQ_DESC_F_NEXT  1
#define VIRTQ_DESC_F_WRITE  2

typedef struct
{
    uint32_t type;
    uint32_t reserved;
    uint64_t sector;
} __attribute__((packed)) virtio_blk_req_t;

typedef struct
{
    uint64_t addr;
    uint32_t len;
    uint16_t flags;
    uint16_t next;
} __attribute__((packed)) vring_desc_t;

typedef struct
{
    uint16_t flags;
    uint16_t idx;
    uint16_t ring[QUEUE_SIZE];
} __attribute__((packed)) vring_avail_t;

typedef struct
{
    uint32_t id;
    uint32_t len;
} __attribute__((packed)) vring_used_elem_t;

typedef struct
{
    uint16_t flags;
    uint16_t idx;
    vring_used_elem_t ring[QUEUE_SIZE];
} __attribute__((packed)) vring_used_t;

// API
int virtio_blk_init(uintptr_t mmio_base);
int virtio_blk_read(uint64_t sector, void* buffer);

void virtio_blk_read_sector(void* buf);
void virtio_blk_read_sector_1(void* buf);

#endif
