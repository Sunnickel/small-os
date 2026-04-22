#include "fat32.h"
#include <stdint.h>

// ── Internal state
// ────────────────────────────────────────────────────────────

static Fat32BPB bpb;
static ReadSectorFn g_read;

static uint64_t g_partition_lba; // LBA of FAT32 partition start
static uint64_t g_fat_lba; // LBA of FAT table
static uint64_t g_cluster_lba; // LBA of cluster 2 (data region start)
static uint32_t g_sectors_per_cluster;
static uint32_t g_bytes_per_cluster;
static uint32_t g_root_cluster;

// Scratch sector buffer – 512 bytes, 4-byte aligned
static uint8_t g_sector_buf[512] __attribute__((aligned(4)));

// ── Helpers
// ───────────────────────────────────────────────────────────────────

static void read_sector(uint64_t lba, void* buf) { g_read(lba, buf); }

// Read the FAT entry for a given cluster number.
static uint32_t fat_entry(uint32_t cluster) {
	uint32_t fat_offset = cluster * 4;
	uint32_t sector_idx = fat_offset / 512;
	uint32_t byte_offset = fat_offset % 512;

	read_sector(g_fat_lba + sector_idx, g_sector_buf);

	uint32_t val = *(uint32_t*)(g_sector_buf + byte_offset);
	return val & 0x0FFFFFFF;
}

// First LBA of a given cluster number (clusters start at 2)
static uint64_t cluster_to_lba(uint32_t cluster) {
	return g_cluster_lba + (uint64_t)(cluster - 2) * g_sectors_per_cluster;
}

// Compare a directory entry's raw 8.3 name (11 bytes, space-padded, uppercase)
// against a caller-supplied 11-byte string. Returns 1 if equal.
static int name83_match(const uint8_t *entry_name, const char *target) {
	for (int i = 0; i < 11; i++)
	{
		if (entry_name[i] != (uint8_t)target[i])
			return 0;
	}
	return 1;
}

// ── Public API
// ────────────────────────────────────────────────────────────────

int fat32_init(uint64_t partition_lba, ReadSectorFn read_fn) {
	g_partition_lba = partition_lba;
	g_read = read_fn;

	// Read BPB from the boot sector of the partition
  read_sector(partition_lba, g_sector_buf);

	// Copy BPB (starts at offset 0 in the boot sector for our purposes)
	uint8_t* src = g_sector_buf;
	uint8_t* dst = (uint8_t*)&bpb;
	for (uint32_t i = 0; i < sizeof(Fat32BPB); i++)
		dst[i] = src[i];

	// Validate
  if (bpb.bytes_per_sector == 0 || bpb.sectors_per_cluster == 0)
    return 0;

	// Pre-compute offsets
	g_sectors_per_cluster = bpb.sectors_per_cluster;
	g_bytes_per_cluster = bpb.bytes_per_sector * bpb.sectors_per_cluster;
	g_root_cluster = bpb.root_cluster;

	g_fat_lba = partition_lba + bpb.reserved_sectors;
  g_cluster_lba = g_fat_lba + (uint64_t)bpb.num_fats * bpb.fat_size_32;

	return 1;
}

uint32_t fat32_find_root(const char *name83) {
	uint32_t cluster = g_root_cluster;

	while (cluster < FAT32_EOC) {
    uint64_t lba = cluster_to_lba(cluster);

    for (uint32_t s = 0; s < g_sectors_per_cluster; s++)
    {
	    read_sector(lba + s, g_sector_buf);

	    Fat32DirEntry *dir = (Fat32DirEntry *)g_sector_buf;
      uint32_t entries_per_sector = 512 / sizeof(Fat32DirEntry);

	    for (uint32_t i = 0; i < entries_per_sector; i++)
	    {
		    Fat32DirEntry* e = &dir[i];

		    if (e->name[0] == 0x00)
          return 0; // end of directory
        if (e->name[0] == 0xE5)
          continue; // deleted
        if (e->attributes == FAT32_ATTR_LFN)
          continue; // LFN entry

		    if (name83_match(e->name, name83))
		    {
			    uint32_t cluster_hi = (uint32_t)e->cluster_hi << 16;
			    uint32_t cluster_lo = e->cluster_lo;
			    return cluster_hi | cluster_lo;
		    }
	    }
    }

    cluster = fat_entry(cluster);
	}

	return 0; // not found
}

uint64_t fat32_read_file(uint32_t first_cluster, void* dest,
                         uint64_t max_bytes)
{
	uint8_t* out = (uint8_t*)dest;
	uint64_t total = 0;
	uint32_t cluster = first_cluster;

	// Temporary per-sector buffer separate from g_sector_buf
  uint8_t sector_buf[512] __attribute__((aligned(4)));

	while (cluster >= 2 && cluster < FAT32_EOC)
	{
		if (total >= max_bytes)
			break;

		uint64_t lba = cluster_to_lba(cluster);

		for (uint32_t s = 0; s < g_sectors_per_cluster; s++)
		{
			if (total >= max_bytes)
				break;

			g_read(lba + s, sector_buf);

			uint64_t remaining = max_bytes - total;
			uint64_t to_copy = remaining < 512 ? remaining : 512;

			uint8_t *src = sector_buf;
      for (uint64_t b = 0; b < to_copy; b++)
        out[total + b] = src[b];

			total += to_copy;
		}

		cluster = fat_entry(cluster);
  }

	return total;
}