#![no_std]

extern crate alloc;

use core::ptr;
use driver_core::block_device::{BlockDevice, BlockError};
use driver_core::dma_allocator::DmaAllocator;

const SECTOR_SIZE: usize = 512;

const VIRTIO_BLK_T_IN: u32 = 0;
const VIRTIO_BLK_T_OUT: u32 = 1;
const VIRTIO_BLK_S_OK: u8 = 0;
const VIRTQ_DESC_F_NEXT: u16 = 1;
const VIRTQ_DESC_F_WRITE: u16 = 2;

/// VirtIO Block device driver
pub struct VirtioBlkDevice {
    base_addr: usize,
    notify_addr: usize,
    device_cfg: usize,
    sector_count: u64,
    queue: VirtQueue,
    req_phys: u64,
    req_virt: usize,
    status_phys: u64,
    status_virt: usize,
}

#[repr(C)]
struct VirtqDesc {
    addr: u64,
    len: u32,
    flags: u16,
    next: u16,
}

#[repr(C)]
struct VirtqAvail {
    flags: u16,
    idx: u16,
    ring: [u16; 64],
    used_event: u16, // Only if VIRTIO_F_EVENT_IDX
}

#[repr(C)]
struct VirtqUsedElem {
    id: u32,
    len: u32,
}

#[repr(C)]
struct VirtqUsed {
    flags: u16,
    idx: u16,
    ring: [VirtqUsedElem; 64],
    avail_event: u16, // Only if VIRTIO_F_EVENT_IDX
}

struct VirtQueue {
    desc: usize,
    avail: usize,
    used: usize,
    queue_size: u16,
    last_used_idx: u16,
}

#[repr(C)]
struct VirtioBlkReq {
    type_: u32,
    reserved: u32,
    sector: u64,
}

impl VirtioBlkDevice {
    pub unsafe fn new(
        base_addr: usize,
        dma: &mut impl DmaAllocator,
    ) -> Result<Self, BlockError> {
        let read8 = |off: usize| -> u8 { core::ptr::read_volatile((base_addr + off) as *const u8) };
        let read16 = |off: usize| -> u16 { core::ptr::read_volatile((base_addr + off) as *const u16) };
        let read32 = |off: usize| -> u32 { core::ptr::read_volatile((base_addr + off) as *const u32) };
        let write8 = |off: usize, v: u8| { core::ptr::write_volatile((base_addr + off) as *mut u8, v) };
        let write16 = |off: usize, v: u16| { core::ptr::write_volatile((base_addr + off) as *mut u16, v) };
        let write32 = |off: usize, v: u32| { core::ptr::write_volatile((base_addr + off) as *mut u32, v) };

        const DEVICE_FEATURE_SELECT: usize = 0x00;
        const DEVICE_FEATURE: usize = 0x04;
        const DRIVER_FEATURE_SELECT: usize = 0x08;
        const DRIVER_FEATURE: usize = 0x0C;
        const DEVICE_STATUS: usize = 0x14;
        const QUEUE_SELECT: usize = 0x16;
        const QUEUE_SIZE: usize = 0x18;
        const QUEUE_MSIX_VECTOR: usize = 0x1A;
        const QUEUE_ENABLE: usize = 0x1C;
        const QUEUE_DESC_LO: usize = 0x20;
        const QUEUE_DESC_HI: usize = 0x24;
        const QUEUE_DRIVER_LO: usize = 0x28;
        const QUEUE_DRIVER_HI: usize = 0x2C;
        const QUEUE_DEVICE_LO: usize = 0x30;
        const QUEUE_DEVICE_HI: usize = 0x34;

        // Reset device
        write8(DEVICE_STATUS, 0);
        while read8(DEVICE_STATUS) != 0 {}

        write8(DEVICE_STATUS, 0x01); // ACKNOWLEDGE
        write8(DEVICE_STATUS, 0x01 | 0x02); // DRIVER

        // Feature negotiation
        write32(DRIVER_FEATURE_SELECT, 0);
        write32(DRIVER_FEATURE, 0);

        write8(DEVICE_STATUS, 0x01 | 0x02 | 0x08); // FEATURES_OK
        if read8(DEVICE_STATUS) & 0x08 == 0 {
            return Err(BlockError::DeviceError);
        }

        // Setup virtqueue 0
        write16(QUEUE_SELECT, 0);
        let queue_size = read16(QUEUE_SIZE).min(64);
        write16(QUEUE_SIZE, queue_size);

        let (desc_phys, desc_virt) = dma.allocate_dma_page().ok_or(BlockError::DeviceError)?;
        let (avail_phys, avail_virt) = dma.allocate_dma_page().ok_or(BlockError::DeviceError)?;
        let (used_phys, used_virt) = dma.allocate_dma_page().ok_or(BlockError::DeviceError)?;

        core::ptr::write_bytes(desc_virt as *mut u8, 0, 4096);
        core::ptr::write_bytes(avail_virt as *mut u8, 0, 4096);
        core::ptr::write_bytes(used_virt as *mut u8, 0, 4096);

        write32(QUEUE_DESC_LO, (desc_phys & 0xFFFF_FFFF) as u32);
        write32(QUEUE_DESC_HI, (desc_phys >> 32) as u32);
        write32(QUEUE_DRIVER_LO, (avail_phys & 0xFFFF_FFFF) as u32);
        write32(QUEUE_DRIVER_HI, (avail_phys >> 32) as u32);
        write32(QUEUE_DEVICE_LO, (used_phys & 0xFFFF_FFFF) as u32);
        write32(QUEUE_DEVICE_HI, (used_phys >> 32) as u32);
        write16(QUEUE_MSIX_VECTOR, 0xFFFF);
        write16(QUEUE_ENABLE, 1);

        let (req_phys, req_virt) = dma.allocate_dma_page().ok_or(BlockError::DeviceError)?;
        let (status_phys, status_virt) = dma.allocate_dma_page().ok_or(BlockError::DeviceError)?;
        core::ptr::write_bytes(req_virt as *mut u8, 0, 4096);
        core::ptr::write_bytes(status_virt as *mut u8, 0, 4096);

        write8(DEVICE_STATUS, 0x01 | 0x02 | 0x08 | 0x04); // DRIVER_OK

        let device_cfg = base_addr + 0x2000;
        let sector_count = core::ptr::read_volatile(device_cfg as *const u64);

        Ok(Self {
            base_addr,
            notify_addr: base_addr + 0x3000,
            device_cfg,
            sector_count,
            queue: VirtQueue {
                desc: desc_virt,
                avail: avail_virt,
                used: used_virt,
                queue_size,
                last_used_idx: 0,
            },
            req_phys,
            req_virt,
            status_phys,
            status_virt,
        })
    }

    unsafe fn do_request(
        &mut self,
        sector: u64,
        mut data_buf: Option<&mut [u8]>,
        data_len: usize,
        is_write: bool,
    ) -> Result<(), BlockError> {
        let q = &mut self.queue;

        // Setup request header
        let req_ptr = self.req_virt as *mut VirtioBlkReq;
        (*req_ptr).type_ = if is_write { VIRTIO_BLK_T_OUT } else { VIRTIO_BLK_T_IN };
        (*req_ptr).reserved = 0;
        (*req_ptr).sector = sector;

        // Clear status
        (self.status_virt as *mut u8).write_volatile(0xFF);

        // Data buffer (at offset 64 in req page)
        let data_virt = self.req_virt + 64;
        let data_phys = self.req_phys + 64;

        // If writing, copy data to DMA buffer
        if is_write {
            if let Some(buf) = &mut data_buf {
                unsafe {
                    ptr::copy_nonoverlapping(
                        buf.as_ptr(),
                        data_virt as *mut u8,
                        buf.len().min(data_len),
                    );
                }
            }
        }

        // Build descriptor chain
        let desc_base = q.desc as *mut VirtqDesc;

        // Desc 0: Header (device reads)
        unsafe {
            ptr::write(
                desc_base.add(0),
                VirtqDesc {
                    addr: self.req_phys,
                    len: 16,
                    flags: VIRTQ_DESC_F_NEXT,
                    next: 1,
                },
            );
        }



        // Desc 1: Data (device writes for read, reads for write)
        // Desc 1: Data (device writes for read, reads for write)
        let data_flags = if is_write {
            VIRTQ_DESC_F_NEXT
        } else {
            VIRTQ_DESC_F_NEXT | VIRTQ_DESC_F_WRITE
        };
        unsafe {
            ptr::write(
                desc_base.add(1),
                VirtqDesc {
                    addr: data_phys,
                    len: data_len as u32,
                    flags: data_flags,
                    next: 2,
                },
            );
        }


        // Desc 2: Status (device writes)
        unsafe {
            ptr::write(
                desc_base.add(2),
                VirtqDesc {
                    addr: self.status_phys,
                    len: 1,
                    flags: VIRTQ_DESC_F_WRITE,
                    next: 0,
                },
            );
        }


        // Publish to avail ring
        let avail = q.avail as *mut VirtqAvail;
        let idx = (*avail).idx;
        (*avail).ring[(idx as usize) % q.queue_size as usize] = 0;
        core::sync::atomic::fence(core::sync::atomic::Ordering::SeqCst);
        (*avail).idx = idx.wrapping_add(1);
        core::sync::atomic::fence(core::sync::atomic::Ordering::SeqCst);

        // Notify
        unsafe {
            ptr::write_volatile(self.notify_addr as *mut u16, 0);
        }

        // Poll for completion
        let used = q.used as *const VirtqUsed;
        let mut timeout = 10_000_000u32;
        loop {
            core::sync::atomic::fence(core::sync::atomic::Ordering::SeqCst);
            if (*used).idx != q.last_used_idx {
                break;
            }
            timeout -= 1;
            if timeout == 0 {
                return Err(BlockError::Timeout);
            }
        }
        q.last_used_idx = q.last_used_idx.wrapping_add(1);

        // Check status
        let status = (self.status_virt as *const u8).read_volatile();
        if status != VIRTIO_BLK_S_OK {
            return Err(BlockError::DeviceError);
        }

        // If reading, copy data from DMA buffer
        if !is_write {
            if let Some(buf) = data_buf {
                unsafe {
                    ptr::copy_nonoverlapping(
                        data_virt as *const u8,
                        buf.as_mut_ptr(),
                        buf.len().min(data_len),
                    );
                }
            }
        }

        Ok(())
    }
}

impl BlockDevice for VirtioBlkDevice {
    fn read_at(&mut self, offset: u64, buf: &mut [u8]) -> Result<(), BlockError> {
        if offset + buf.len() as u64 > self.size() {
            return Err(BlockError::OutOfBounds);
        }

        let mut remaining = buf;
        let mut current_offset = offset;

        while !remaining.is_empty() {
            let lba = current_offset / SECTOR_SIZE as u64;
            let sector_off = (current_offset % SECTOR_SIZE as u64) as usize;
            let can_read = (SECTOR_SIZE - sector_off).min(remaining.len());

            if sector_off == 0 && can_read == SECTOR_SIZE {
                // Aligned single sector
                unsafe {
                    self.do_request(lba, Some(&mut remaining[..SECTOR_SIZE]), SECTOR_SIZE, false)?;
                }
                remaining = &mut remaining[SECTOR_SIZE..];
                current_offset += SECTOR_SIZE as u64;
            } else {
                // Unaligned - use bounce buffer
                let mut sector_buf = [0u8; SECTOR_SIZE];
                unsafe {
                    self.do_request(lba, Some(&mut sector_buf[..]), SECTOR_SIZE, false)?;
                }
                remaining[..can_read].copy_from_slice(&sector_buf[sector_off..sector_off + can_read]);
                remaining = &mut remaining[can_read..];
                current_offset += can_read as u64;
            }
        }

        Ok(())
    }

    fn write_at(&mut self, offset: u64, buf: &[u8]) -> Result<(), BlockError> {
        if offset + buf.len() as u64 > self.size() {
            return Err(BlockError::OutOfBounds);
        }

        let mut remaining = buf;
        let mut current_offset = offset;

        while !remaining.is_empty() {
            let lba = current_offset / SECTOR_SIZE as u64;
            let sector_off = (current_offset % SECTOR_SIZE as u64) as usize;
            let can_write = (SECTOR_SIZE - sector_off).min(remaining.len());

            if sector_off == 0 && can_write == SECTOR_SIZE {
                // Aligned single sector
                unsafe {
                    self.do_request(lba, Some(&mut remaining[..SECTOR_SIZE].to_vec().as_mut_slice()), SECTOR_SIZE, true)?;
                }
                remaining = &remaining[SECTOR_SIZE..];
                current_offset += SECTOR_SIZE as u64;
            } else {
                // Unaligned - need read-modify-write
                let mut sector_buf = [0u8; SECTOR_SIZE];

                // Read existing sector
                unsafe {
                    self.do_request(lba, Some(&mut sector_buf[..]), SECTOR_SIZE, false)?;
                }

                // Modify
                sector_buf[sector_off..sector_off + can_write].copy_from_slice(&remaining[..can_write]);

                // Write back
                unsafe {
                    self.do_request(lba, Some(&mut sector_buf[..]), SECTOR_SIZE, true)?;
                }

                remaining = &remaining[can_write..];
                current_offset += can_write as u64;
            }
        }

        Ok(())
    }

    fn size(&self) -> u64 {
        self.sector_count * SECTOR_SIZE as u64
    }

    fn sector_size(&self) -> usize {
        SECTOR_SIZE
    }
}