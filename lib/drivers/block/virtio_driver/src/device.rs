use core::ptr;

use hal::{
    block::{BlockDevice, BlockError},
    dma::DmaAllocator,
    io::IoError,
};

use crate::{constants::*, queue::VirtQueue, request::do_request};

pub struct VirtioBlkDevice {
    notify_addr: usize,
    sector_count: u64,
    queue: VirtQueue,
    req_phys: u64,
    req_virt: usize,
    status_phys: u64,
    status_virt: usize,
}

impl VirtioBlkDevice {
    pub unsafe fn new(base_addr: usize, dma: &mut impl DmaAllocator) -> Result<Self, BlockError> {
        unsafe {
            let read8 = |off: usize| ptr::read_volatile((base_addr + off) as *const u8);
            let read16 = |off: usize| ptr::read_volatile((base_addr + off) as *const u16);
            let write8 = |off: usize, v: u8| ptr::write_volatile((base_addr + off) as *mut u8, v);
            let write16 =
                |off: usize, v: u16| ptr::write_volatile((base_addr + off) as *mut u16, v);
            let write32 =
                |off: usize, v: u32| ptr::write_volatile((base_addr + off) as *mut u32, v);

            // Reset
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

            // Setup queue 0
            write16(QUEUE_SELECT, 0);
            let queue_size = read16(QUEUE_SIZE).min(64);
            write16(QUEUE_SIZE, queue_size);

            let (desc_phys, desc_virt) = dma.allocate_dma_page().ok_or(BlockError::NoMemory)?;
            let (avail_phys, avail_virt) = dma.allocate_dma_page().ok_or(BlockError::NoMemory)?;
            let (used_phys, used_virt) = dma.allocate_dma_page().ok_or(BlockError::NoMemory)?;
            let (req_phys, req_virt) = dma.allocate_dma_page().ok_or(BlockError::NoMemory)?;
            let (status_phys, status_virt) = dma.allocate_dma_page().ok_or(BlockError::NoMemory)?;

            for &virt in &[desc_virt, avail_virt, used_virt, req_virt, status_virt] {
                ptr::write_bytes(virt as *mut u8, 0, 4096);
            }

            write32(QUEUE_DESC_LO, (desc_phys & 0xFFFF_FFFF) as u32);
            write32(QUEUE_DESC_HI, (desc_phys >> 32) as u32);
            write32(QUEUE_DRIVER_LO, (avail_phys & 0xFFFF_FFFF) as u32);
            write32(QUEUE_DRIVER_HI, (avail_phys >> 32) as u32);
            write32(QUEUE_DEVICE_LO, (used_phys & 0xFFFF_FFFF) as u32);
            write32(QUEUE_DEVICE_HI, (used_phys >> 32) as u32);
            write16(QUEUE_MSIX_VECTOR, 0xFFFF);
            write16(QUEUE_ENABLE, 1);

            write8(DEVICE_STATUS, 0x01 | 0x02 | 0x08 | 0x04); // DRIVER_OK

            let device_cfg = base_addr + 0x2000;
            let sector_count = ptr::read_volatile(device_cfg as *const u64);

            Ok(Self {
                notify_addr: base_addr + 0x3000,
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
    }
}

impl BlockDevice for VirtioBlkDevice {
    fn read_at(&mut self, offset: u64, buf: &mut [u8]) -> Result<(), IoError> {
        if offset + buf.len() as u64 > self.size() {
            return Err(BlockError::OutOfBounds.into());
        }

        let mut remaining = buf;
        let mut current_offset = offset;

        while !remaining.is_empty() {
            let lba = current_offset / SECTOR_SIZE as u64;
            let sector_off = (current_offset % SECTOR_SIZE as u64) as usize;
            let can_read = (SECTOR_SIZE - sector_off).min(remaining.len());

            if sector_off == 0 && can_read == SECTOR_SIZE {
                unsafe {
                    do_request(
                        &mut self.queue,
                        self.req_phys,
                        self.req_virt,
                        self.status_phys,
                        self.status_virt,
                        self.notify_addr,
                        lba,
                        Some(&mut remaining[..SECTOR_SIZE]),
                        SECTOR_SIZE,
                        false,
                    )?;
                }
                remaining = &mut remaining[SECTOR_SIZE..];
                current_offset += SECTOR_SIZE as u64;
            } else {
                let mut sector_buf = [0u8; SECTOR_SIZE];
                unsafe {
                    do_request(
                        &mut self.queue,
                        self.req_phys,
                        self.req_virt,
                        self.status_phys,
                        self.status_virt,
                        self.notify_addr,
                        lba,
                        Some(&mut sector_buf),
                        SECTOR_SIZE,
                        false,
                    )?;
                }
                remaining[..can_read]
                    .copy_from_slice(&sector_buf[sector_off..sector_off + can_read]);
                remaining = &mut remaining[can_read..];
                current_offset += can_read as u64;
            }
        }

        Ok(())
    }

    fn write_at(&mut self, offset: u64, buf: &[u8]) -> Result<(), IoError> {
        if offset + buf.len() as u64 > self.size() {
            return Err(BlockError::OutOfBounds.into());
        }

        let mut remaining = buf;
        let mut current_offset = offset;

        while !remaining.is_empty() {
            let lba = current_offset / SECTOR_SIZE as u64;
            let sector_off = (current_offset % SECTOR_SIZE as u64) as usize;
            let can_write = (SECTOR_SIZE - sector_off).min(remaining.len());

            if sector_off == 0 && can_write == SECTOR_SIZE {
                let mut tmp = [0u8; SECTOR_SIZE];
                tmp.copy_from_slice(&remaining[..SECTOR_SIZE]);
                unsafe {
                    do_request(
                        &mut self.queue,
                        self.req_phys,
                        self.req_virt,
                        self.status_phys,
                        self.status_virt,
                        self.notify_addr,
                        lba,
                        Some(&mut tmp),
                        SECTOR_SIZE,
                        true,
                    )?;
                }
                remaining = &remaining[SECTOR_SIZE..];
                current_offset += SECTOR_SIZE as u64;
            } else {
                let mut sector_buf = [0u8; SECTOR_SIZE];
                unsafe {
                    do_request(
                        &mut self.queue,
                        self.req_phys,
                        self.req_virt,
                        self.status_phys,
                        self.status_virt,
                        self.notify_addr,
                        lba,
                        Some(&mut sector_buf),
                        SECTOR_SIZE,
                        false,
                    )?;
                }
                sector_buf[sector_off..sector_off + can_write]
                    .copy_from_slice(&remaining[..can_write]);
                unsafe {
                    do_request(
                        &mut self.queue,
                        self.req_phys,
                        self.req_virt,
                        self.status_phys,
                        self.status_virt,
                        self.notify_addr,
                        lba,
                        Some(&mut sector_buf),
                        SECTOR_SIZE,
                        true,
                    )?;
                }
                remaining = &remaining[can_write..];
                current_offset += can_write as u64;
            }
        }

        Ok(())
    }

    fn size(&self) -> u64 { self.sector_count * SECTOR_SIZE as u64 }
    fn sector_size(&self) -> usize { SECTOR_SIZE }
}
