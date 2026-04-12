#pragma once
#include <stdint.h>
#include "fat32.h"   // ReadSectorFn


// Initialise the virtio-blk device.
// Returns 1 on success, 0 if no virtio-blk device was found.
int  virtio_blk_init(void);

// ReadSectorFn-compatible: read one 512-byte sector at `lba` into `buf`.
// Requires virtio_blk_init() to have succeeded.
void virtio_blk_read_sector(uint64_t lba, void *buf);
void virtio_blk_read_sector_1(uint64_t lba, void *buf);

// Returns 1 if virtio_blk_init() has been called successfully.
int  virtio_blk_ready(void);