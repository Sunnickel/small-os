use alloc::{boxed::Box, sync::Arc};
use core::ptr;

use bus::pci::PciBusDevice;
use device::{Device, DeviceId};

use crate::{Driver, DriverError, DriverState, MatchRule};

mod constants;
mod queue;

use constants::*;
use hal::{block::BlockError, dma::DmaBuffer};
use queue::{VirtQueue, VirtioBlkReq, VirtqAvail, VirtqDesc, VirtqUsed};

use crate::block::BlockDeviceEnum;
// ── Driver entry point
// ────────────────────────────────────────────────────────

pub const VENDOR_VIRTIO: u16 = 0x1AF4;
pub const DEVICE_VIRTIO_BLK: u16 = 0x1042; // modern
pub const DEVICE_VIRTIO_BLK_L: u16 = 0x1001; // legacy

pub struct VirtioBlkDriver;

impl Driver for VirtioBlkDriver {
    fn name(&self) -> &'static str { "virtio-blk" }

    fn rules(&self) -> &[MatchRule] {
        &[
            MatchRule::PciId { vendor: VENDOR_VIRTIO, device: DEVICE_VIRTIO_BLK },
            MatchRule::PciId { vendor: VENDOR_VIRTIO, device: DEVICE_VIRTIO_BLK_L },
        ]
    }

    fn bind(
        &self,
        _device_id: DeviceId,
        device: Arc<dyn Device>,
    ) -> Result<Box<dyn DriverState>, DriverError> {
        let pci = device.as_any().downcast_ref::<PciBusDevice>().ok_or(DriverError::BindFailed)?;

        pci.enable_dma();
        pci.enable_mmio();

        let phys_offset = crate::phys_offset();

        let mut dma_guard = crate::dma().lock();
        let state = unsafe {
            VirtioBlkState::init(pci, phys_offset, &mut *dma_guard)
                .map_err(|_| DriverError::BindFailed)?
        };

        Ok(Box::new(state))
    }
}

// ── Driver state
// ──────────────────────────────────────────────────────────────

pub struct VirtioBlkState {
    common_cfg: usize,
    notify_base: usize,
    notify_off_multiplier: u32,
    pub(crate) sector_count: u64,
    queue: VirtQueue,
    desc_buf: DmaBuffer,
    avail_buf: DmaBuffer,
    used_buf: DmaBuffer,
    dma_buf: DmaBuffer,
}

impl VirtioBlkState {
    pub unsafe fn init(
        pci: &PciBusDevice,
        phys_offset: u64,
        dma: &mut dyn hal::dma::DmaAllocator,
    ) -> Result<Self, &'static str> {
        unsafe {
            let info = pci.info();

            let common_off = Self::find_virtio_cap(info, VIRTIO_PCI_CAP_COMMON_CFG)
                .ok_or("missing common cfg")?;
            let notify_off = Self::find_virtio_cap(info, VIRTIO_PCI_CAP_NOTIFY_CFG)
                .ok_or("missing notify cfg")?;
            let device_off = Self::find_virtio_cap(info, VIRTIO_PCI_CAP_DEVICE_CFG)
                .ok_or("missing device cfg")?;

            let common_cfg = Self::map_cap(info, common_off, phys_offset)?;
            let notify_base = Self::map_cap(info, notify_off, phys_offset)?;
            let device_cfg = Self::map_cap(info, device_off, phys_offset)?;

            let notify_off_multiplier = info.read32(notify_off.cap_offset + 16);

            Self::init_device(common_cfg, notify_base, device_cfg, notify_off_multiplier, dma)
        }
    }

    fn find_virtio_cap(info: &hal::pci::PciDeviceInfo, cfg_type: u8) -> Option<VirtioCapInfo> {
        let status = info.read16(0x06);
        if status & 0x10 == 0 {
            return None;
        }

        let mut ptr = (info.read8(0x34) & !0x3) as u16;
        for _ in 0..48 {
            if ptr < 0x40 {
                break;
            }
            let id = info.read8(ptr);
            let next = info.read8(ptr + 1) & !0x3;

            if id == 0x09 {
                // PCI_CAP_VENDOR
                let t = info.read8(ptr + 3);
                if t == cfg_type {
                    return Some(VirtioCapInfo {
                        cfg_type: t,
                        bar: info.read8(ptr + 4),
                        offset: info.read32(ptr + 8),
                        length: info.read32(ptr + 12),
                        cap_offset: ptr,
                    });
                }
            }
            if next == 0 {
                break;
            }
            ptr = next as u16;
        }
        None
    }

    fn map_cap(
        info: &hal::pci::PciDeviceInfo,
        cap: VirtioCapInfo,
        phys_offset: u64,
    ) -> Result<usize, &'static str> {
        let bar = info.bar_mmio(cap.bar as usize).ok_or("BAR not found")?;
        Ok((bar.as_u64() + cap.offset as u64 + phys_offset) as usize)
    }

    unsafe fn init_device(
        common_cfg: usize,
        notify_base: usize,
        device_cfg: usize,
        notify_off_multiplier: u32,
        dma: &mut dyn hal::dma::DmaAllocator,
    ) -> Result<Self, &'static str> {
        unsafe {
            // 1. Reset device
            ptr::write_volatile((common_cfg + COMMON_DEVICE_STATUS) as *mut u8, 0);

            // 2. Acknowledge
            ptr::write_volatile(
                (common_cfg + COMMON_DEVICE_STATUS) as *mut u8,
                VIRTIO_STATUS_ACKNOWLEDGE | VIRTIO_STATUS_DRIVER,
            );

            // 3. Negotiate features
            ptr::write_volatile((common_cfg + COMMON_DEVICE_FEATURE_SELECT) as *mut u32, 0);
            let features = ptr::read_volatile((common_cfg + COMMON_DEVICE_FEATURE) as *const u32);

            // Accept all features for now
            ptr::write_volatile((common_cfg + COMMON_DRIVER_FEATURE_SELECT) as *mut u32, 0);
            ptr::write_volatile((common_cfg + COMMON_DRIVER_FEATURE) as *mut u32, features);

            // 4. Features OK
            ptr::write_volatile(
                (common_cfg + COMMON_DEVICE_STATUS) as *mut u8,
                VIRTIO_STATUS_ACKNOWLEDGE | VIRTIO_STATUS_DRIVER | VIRTIO_STATUS_FEATURES_OK,
            );

            // Verify
            let status = ptr::read_volatile((common_cfg + COMMON_DEVICE_STATUS) as *const u8);
            if status & VIRTIO_STATUS_FEATURES_OK == 0 {
                return Err("FEATURES_OK rejected");
            }

            // 5. Configure queue 0
            ptr::write_volatile((common_cfg + COMMON_QUEUE_SELECT) as *mut u16, 0);

            let queue_size_max = ptr::read_volatile((common_cfg + COMMON_QUEUE_SIZE) as *const u16);

            if queue_size_max == 0 {
                return Err("queue 0 unavailable");
            }

            let queue_size = 64u16.min(queue_size_max);
            ptr::write_volatile((common_cfg + COMMON_QUEUE_SIZE) as *mut u16, queue_size);

            // 6. Allocate queue pages
            let desc_buf = dma.alloc(4096, 4096).ok_or("dma alloc failed")?;
            let desc_phys = desc_buf.phys;
            let desc_virt = desc_buf.virt as usize;

            let avail_buf = dma.alloc(4096, 4096).ok_or("dma alloc failed")?;
            let avail_phys = avail_buf.phys;
            let avail_virt = avail_buf.virt as usize;

            let used_buf = dma.alloc(4096, 4096).ok_or("dma alloc failed")?;
            let used_phys = used_buf.phys;
            let used_virt = used_buf.virt as usize;

            let dma_buf = dma.alloc(4096, 4096).ok_or("dma alloc failed")?;
            let _dma_phys = dma_buf.phys;
            let _dma_virt = dma_buf.virt as usize;

            ptr::write_bytes(desc_virt as *mut u8, 0, 4096);
            ptr::write_bytes(avail_virt as *mut u8, 0, 4096);
            ptr::write_bytes(used_virt as *mut u8, 0, 4096);

            // 7. Set queue addresses
            ptr::write_volatile(
                (common_cfg + COMMON_QUEUE_DESC_LOW) as *mut u32,
                desc_phys.low_u32(),
            );
            ptr::write_volatile(
                (common_cfg + COMMON_QUEUE_DESC_HIGH) as *mut u32,
                desc_phys.high_u32(),
            );

            ptr::write_volatile(
                (common_cfg + COMMON_QUEUE_AVAIL_LOW) as *mut u32,
                avail_phys.low_u32(),
            );
            ptr::write_volatile(
                (common_cfg + COMMON_QUEUE_AVAIL_HIGH) as *mut u32,
                avail_phys.high_u32(),
            );

            ptr::write_volatile(
                (common_cfg + COMMON_QUEUE_USED_LOW) as *mut u32,
                used_phys.low_u32(),
            );
            ptr::write_volatile(
                (common_cfg + COMMON_QUEUE_USED_HIGH) as *mut u32,
                used_phys.high_u32(),
            );

            // 8. Enable queue
            ptr::write_volatile((common_cfg + COMMON_QUEUE_ENABLE) as *mut u16, 1);

            // 9. DRIVER_OK
            ptr::write_volatile(
                (common_cfg + COMMON_DEVICE_STATUS) as *mut u8,
                VIRTIO_STATUS_ACKNOWLEDGE
                    | VIRTIO_STATUS_DRIVER
                    | VIRTIO_STATUS_FEATURES_OK
                    | VIRTIO_STATUS_DRIVER_OK,
            );

            // 10. Read capacity
            let sector_count = ptr::read_volatile(device_cfg as *const u64);

            let queue = VirtQueue {
                desc: desc_virt,
                avail: avail_virt,
                used: used_virt,
                queue_size,
                last_used_idx: 0,
            };

            Ok(Self {
                common_cfg,
                notify_base,
                notify_off_multiplier,
                sector_count,
                queue,
                desc_buf,
                avail_buf,
                used_buf,
                dma_buf,
            })
        }
    }

    /// Read sectors from disk
    pub fn read_sectors(&mut self, lba: u64, buf: &mut [u8]) -> Result<(), BlockError> {
        let sectors = (buf.len() + 511) / 512;

        // Build request
        let req = VirtioBlkReq {
            type_: VIRTIO_BLK_T_IN, // 0
            reserved: 0,
            sector: lba,
        };

        // Write request to dma_buf
        unsafe {
            let req_ptr = self.dma_buf.virt as *mut VirtioBlkReq;
            ptr::write_volatile(req_ptr, req);

            // Setup descriptor chain: header -> data -> status
            self.setup_chain(
                self.dma_buf.phys.as_u64(),       // Request header
                self.dma_buf.phys.as_u64() + 512, // Data buffer (after header)
                buf.as_mut_ptr(),
                sectors * 512,
                true, // Write to device? No, read from device
            );

            // Notify and wait
            self.notify_and_wait();

            // Copy data to user buffer
            ptr::copy_nonoverlapping(
                (self.dma_buf.virt as *const u8).add(512),
                buf.as_mut_ptr(),
                buf.len().min(sectors * 512),
            );
        }

        Ok(())
    }

    /// Write sectors to disk
    pub fn write_sectors(&mut self, lba: u64, buf: &[u8]) -> Result<(), BlockError> {
        let sectors = (buf.len() + 511) / 512;

        unsafe {
            // Write data to dma buffer (after header)
            ptr::copy_nonoverlapping(
                buf.as_ptr(),
                (self.dma_buf.virt as *mut u8).add(512),
                buf.len(),
            );

            let req = VirtioBlkReq {
                type_: VIRTIO_BLK_T_OUT, // 1
                reserved: 0,
                sector: lba,
            };

            let req_ptr = self.dma_buf.virt as *mut VirtioBlkReq;
            ptr::write_volatile(req_ptr, req);

            self.setup_chain(
                self.dma_buf.phys.as_u64(),
                self.dma_buf.phys.as_u64() + 512,
                ptr::null_mut(), // No output buffer for write
                sectors * 512,
                false, // Write to device
            );

            self.notify_and_wait();
        }

        Ok(())
    }

    unsafe fn setup_chain(
        &mut self,
        req_phys: u64,
        data_phys: u64,
        _user_buf: *mut u8,
        len: usize,
        is_read: bool,
    ) { unsafe {
        // Setup descriptor table entries
        let desc = self.desc_buf.virt as *mut VirtqDesc;

        // Descriptor 0: Request header (driver -> device)
        (*desc.add(0)).addr = req_phys;
        (*desc.add(0)).len = 16; // sizeof(VirtioBlkReq)
        (*desc.add(0)).flags = VIRTQ_DESC_F_NEXT;
        (*desc.add(0)).next = 1;

        // Descriptor 1: Data buffer
        (*desc.add(1)).addr = data_phys;
        (*desc.add(1)).len = len as u32;
        (*desc.add(1)).flags = if is_read { VIRTQ_DESC_F_WRITE } else { 0 } | VIRTQ_DESC_F_NEXT;
        (*desc.add(1)).next = 2;

        // Descriptor 2: Status (device -> driver)
        let status_phys = req_phys + 1024; // At offset 1024 in dma_buf
        (*desc.add(2)).addr = status_phys;
        (*desc.add(2)).len = 1;
        (*desc.add(2)).flags = VIRTQ_DESC_F_WRITE;
        (*desc.add(2)).next = 0;

        // Add to avail ring
        let avail = self.avail_buf.virt as *mut VirtqAvail;
        let idx = (*avail).idx;
        (*avail).ring[idx as usize % self.queue.queue_size as usize] = 0; // Chain starts at desc 0
        (*avail).idx = idx + 1;
    }}

    unsafe fn notify_and_wait(&self) { unsafe {
        // Write notify offset
        let notify_addr = self.notify_base as *mut u16;
        ptr::write_volatile(notify_addr, 0); // Queue 0

        // Poll completion (simplified - should use interrupts)
        let used = self.used_buf.virt as *mut VirtqUsed;
        while (*used).idx == 0 {} // Wait for device to update
    }}
}

impl DriverState for VirtioBlkState {
    fn stop(&self) {
        unsafe {
            ptr::write_volatile((self.common_cfg + COMMON_DEVICE_STATUS) as *mut u8, 0);
        }
    }

    fn as_block_device(self: Box<Self>) -> Option<BlockDeviceEnum> {
        Some(BlockDeviceEnum::Virtio(self))
    }
}

#[derive(Debug, Clone, Copy)]
struct VirtioCapInfo {
    cfg_type: u8,
    bar: u8,
    offset: u32,
    length: u32,
    cap_offset: u16,
}
