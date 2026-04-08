use alloc::{borrow::ToOwned, vec::Vec};
use core::{
    ptr,
    sync::atomic::{fence, Ordering},
};

use hal::{
    block::{BlockDevice, BlockError},
    dma::DmaAllocator,
};

use crate::{
    block::virtio_driver::{
        constants::*,
        queue::{VirtQueue, VirtioBlkReq, VirtioCapInfo, VirtqAvail, VirtqDesc, VirtqUsed},
    },
    pci::PciDevice,
    util::debug,
};

mod constants;
mod queue;

pub enum VirtioMode {
    Modern,
    Legacy,
}

/// VirtIO Block device over PCI
pub struct VirtioDriver {
    common_cfg: usize,
    notify_base: usize,
    device_cfg: usize,
    notify_off_multiplier: u32,

    sector_count: u64,
    queue: VirtQueue,

    desc_phys: u64,
    avail_phys: u64,
    used_phys: u64,

    dma_phys: u64,
    dma_virt: usize,
}

impl VirtioDriver {
    /// Initialize VirtIO PCI device
    pub unsafe fn init(
        pci_dev: &PciDevice,
        phys_mem_offset: u64,
        dma: &mut impl DmaAllocator,
    ) -> Result<Self, &'static str> {
        debug("VirtIO PCI: scanning capabilities...");

        // Find all VirtIO PCI capabilities
        let caps = Self::find_virtio_caps(pci_dev);

        if caps.is_empty() {
            return Err("no virtio capabilities found");
        }

        debug(&alloc::format!("VirtIO PCI: found {} caps", caps.len()));

        // Find required capabilities
        let common = caps
            .iter()
            .find(|c| c.cfg_type == VIRTIO_PCI_CAP_COMMON_CFG)
            .ok_or("missing common cfg")?;
        let notify = caps
            .iter()
            .find(|c| c.cfg_type == VIRTIO_PCI_CAP_NOTIFY_CFG)
            .ok_or("missing notify cfg")?;
        let device = caps
            .iter()
            .find(|c| c.cfg_type == VIRTIO_PCI_CAP_DEVICE_CFG)
            .ok_or("missing device cfg")?;

        // Map BARs to virtual addresses
        let common_cfg = Self::map_cap(pci_dev, common, phys_mem_offset)?;
        let notify_base = Self::map_cap(pci_dev, notify, phys_mem_offset)?;
        let device_cfg = Self::map_cap(pci_dev, device, phys_mem_offset)?;

        // Read notify multiplier (at offset 16 within notify capability structure)
        let notify_off_multiplier = pci_dev.read_config_dword(notify.cap_offset + 16);

        debug(&alloc::format!(
            "VirtIO PCI: common={:#x}, notify={:#x}, device={:#x}, mult={}",
            common_cfg,
            notify_base,
            device_cfg,
            notify_off_multiplier
        ));

        // Initialize device
        Self::init_device(
            common_cfg,
            notify_base,
            device_cfg,
            notify_off_multiplier,
            phys_mem_offset,
            dma,
        )
    }

    /// Find all VirtIO PCI capabilities
    fn find_virtio_caps(pci_dev: &PciDevice) -> Vec<VirtioCapInfo> {
        let mut caps = Vec::new();

        // Check if capabilities are enabled
        let status = pci_dev.read_config_word(0x06);
        if status & 0x10 == 0 {
            return caps;
        }

        // Walk capability list
        let mut cap_ptr = pci_dev.read_config_byte(0x34) as u16;

        while cap_ptr != 0 && cap_ptr != 0xFF {
            let cap_id = pci_dev.read_config_byte(cap_ptr);

            if cap_id == PCI_CAP_VENDOR {
                let cfg_type = pci_dev.read_config_byte(cap_ptr + 3);

                // Check if it's a VirtIO capability (type 1-5)
                if cfg_type >= 1 && cfg_type <= 5 {
                    caps.push(VirtioCapInfo {
                        cfg_type,
                        bar: pci_dev.read_config_byte(cap_ptr + 4),
                        offset: pci_dev.read_config_dword(cap_ptr + 8),
                        length: pci_dev.read_config_dword(cap_ptr + 12),
                        cap_offset: cap_ptr,
                    });

                    debug(&alloc::format!(
                        "  VirtIO cap {}: bar={}, offset={:#x}, len={}",
                        cfg_type,
                        pci_dev.read_config_byte(cap_ptr + 4),
                        pci_dev.read_config_dword(cap_ptr + 8),
                        pci_dev.read_config_dword(cap_ptr + 12)
                    ));
                }
            }

            cap_ptr = pci_dev.read_config_byte(cap_ptr + 1) as u16;
        }

        caps
    }

    /// Map a capability's BAR to virtual address
    fn map_cap(
        pci_dev: &PciDevice,
        cap: &VirtioCapInfo,
        phys_offset: u64,
    ) -> Result<usize, &'static str> {
        let bar_addr = pci_dev.bar_phys(cap.bar as usize).ok_or("BAR not found")?;

        let virt = bar_addr + cap.offset as u64 + phys_offset;
        Ok(virt as usize)
    }

    /// Initialize the device
    unsafe fn init_device(
        common_cfg: usize,
        notify_base: usize,
        device_cfg: usize,
        notify_off_multiplier: u32,
        phys_offset: u64,
        dma: &mut impl DmaAllocator,
    ) -> Result<Self, &'static str> {
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
        debug(&alloc::format!("VirtIO PCI: features={:#x}", features));

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
        debug(&alloc::format!("VirtIO PCI: queue_max={}", queue_size_max));

        if queue_size_max == 0 {
            return Err("queue 0 unavailable");
        }

        let queue_size = 64u16.min(queue_size_max);
        ptr::write_volatile((common_cfg + COMMON_QUEUE_SIZE) as *mut u16, queue_size);

        // 6. Allocate queue pages
        let (desc_phys, desc_virt) = dma.allocate_dma_page().ok_or("dma alloc failed")?;
        let (avail_phys, avail_virt) = dma.allocate_dma_page().ok_or("dma alloc failed")?;
        let (used_phys, used_virt) = dma.allocate_dma_page().ok_or("dma alloc failed")?;

        core::ptr::write_bytes(desc_virt as *mut u8, 0, 4096);
        core::ptr::write_bytes(avail_virt as *mut u8, 0, 4096);
        core::ptr::write_bytes(used_virt as *mut u8, 0, 4096);

        // 7. Set queue addresses
        ptr::write_volatile((common_cfg + COMMON_QUEUE_DESC_LOW) as *mut u32, desc_phys as u32);
        ptr::write_volatile(
            (common_cfg + COMMON_QUEUE_DESC_HIGH) as *mut u32,
            (desc_phys >> 32) as u32,
        );
        ptr::write_volatile((common_cfg + COMMON_QUEUE_AVAIL_LOW) as *mut u32, avail_phys as u32);
        ptr::write_volatile(
            (common_cfg + COMMON_QUEUE_AVAIL_HIGH) as *mut u32,
            (avail_phys >> 32) as u32,
        );
        ptr::write_volatile((common_cfg + COMMON_QUEUE_USED_LOW) as *mut u32, used_phys as u32);
        ptr::write_volatile(
            (common_cfg + COMMON_QUEUE_USED_HIGH) as *mut u32,
            (used_phys >> 32) as u32,
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
        debug(&alloc::format!(
            "VirtIO PCI: {} sectors ({} MB)",
            sector_count,
            sector_count * 512 / 1024 / 1024
        ));

        // 11. Allocate DMA page for requests
        let (dma_phys, dma_virt) = dma.allocate_dma_page().ok_or("dma alloc failed")?;

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
            device_cfg,
            notify_off_multiplier,
            sector_count,
            queue,
            desc_phys,
            avail_phys,
            used_phys,
            dma_phys,
            dma_virt,
        })
    }

    /// Submit a block request
    unsafe fn do_request(
        &mut self,
        sector: u64,
        buf: &mut [u8],
        is_write: bool,
    ) -> Result<(), BlockError> {
        const REQ_SIZE: usize = 16;

        if buf.len() > 4096 - REQ_SIZE - 1 {
            return Err(BlockError::InvalidRequest);
        }

        // Setup request header
        let req = self.dma_virt as *mut VirtioBlkReq;
        (*req).type_ = if is_write { VIRTIO_BLK_T_OUT } else { VIRTIO_BLK_T_IN };
        (*req).reserved = 0;
        (*req).sector = sector;

        let data_virt = self.dma_virt + REQ_SIZE;
        let data_phys = self.dma_phys + REQ_SIZE as u64;
        let status_virt = data_virt + buf.len();
        let status_phys = data_phys + buf.len() as u64;

        // Copy write data
        if is_write {
            ptr::copy_nonoverlapping(buf.as_ptr(), data_virt as *mut u8, buf.len());
        }
        *(status_virt as *mut u8) = 0xFF;

        // Build descriptor chain
        let desc = self.queue.desc as *mut VirtqDesc;

        ptr::write_volatile(
            &mut (*desc.add(0)),
            VirtqDesc {
                addr: self.dma_phys,
                len: REQ_SIZE as u32,
                flags: VIRTQ_DESC_F_NEXT,
                next: 1,
            },
        );

        ptr::write_volatile(
            &mut (*desc.add(1)),
            VirtqDesc {
                addr: data_phys,
                len: buf.len() as u32,
                flags: if is_write {
                    VIRTQ_DESC_F_NEXT
                } else {
                    VIRTQ_DESC_F_NEXT | VIRTQ_DESC_F_WRITE
                },
                next: 2,
            },
        );

        ptr::write_volatile(
            &mut (*desc.add(2)),
            VirtqDesc { addr: status_phys, len: 1, flags: VIRTQ_DESC_F_WRITE, next: 0 },
        );

        // Add to avail ring
        let avail = self.queue.avail as *mut VirtqAvail;
        let idx = (*avail).idx;
        let ring_idx = (idx as usize) % (self.queue.queue_size as usize);
        (*avail).ring[ring_idx] = 0;

        fence(Ordering::SeqCst);
        (*avail).idx = idx.wrapping_add(1);
        fence(Ordering::SeqCst);

        // Notify device
        let notify_off =
            ptr::read_volatile((self.common_cfg + COMMON_QUEUE_NOTIFY_OFF) as *const u16);
        let notify_addr =
            self.notify_base + (notify_off as u32 * self.notify_off_multiplier) as usize;
        ptr::write_volatile(notify_addr as *mut u16, 0);

        // Poll for completion
        let used = self.queue.used as *const VirtqUsed;
        let mut timeout: u64 = 10_000_000;

        loop {
            fence(Ordering::SeqCst);
            if (*used).idx != self.queue.last_used_idx {
                break;
            }
            timeout -= 1;
            if timeout == 0 {
                debug("VirtIO: timeout");
                return Err(BlockError::Timeout);
            }
        }

        self.queue.last_used_idx = self.queue.last_used_idx.wrapping_add(1);

        // Check status
        if *(status_virt as *const u8) != VIRTIO_BLK_S_OK {
            debug(&alloc::format!("VirtIO: error status={}", *(status_virt as *const u8)));
            return Err(BlockError::DeviceError);
        }

        // Copy read data
        if !is_write {
            ptr::copy_nonoverlapping(data_virt as *const u8, buf.as_mut_ptr(), buf.len());
        }

        Ok(())
    }
}

impl BlockDevice for VirtioDriver {
    fn read_at(&mut self, offset: u64, buf: &mut [u8]) -> Result<(), hal::io::IoError> {
        if offset + buf.len() as u64 > self.size() {
            return Err(BlockError::OutOfBounds.into());
        }
        let sector = offset / SECTOR_SIZE as u64;
        unsafe { self.do_request(sector, buf, false).map_err(|e| e.into()) }
    }

    fn write_at(&mut self, offset: u64, buf: &[u8]) -> Result<(), hal::io::IoError> {
        if offset + buf.len() as u64 > self.size() {
            return Err(BlockError::OutOfBounds.into());
        }
        let sector = offset / SECTOR_SIZE as u64;
        let mut tmp = buf.to_owned();
        unsafe { self.do_request(sector, &mut tmp, true).map_err(|e| e.into()) }
    }

    fn size(&self) -> u64 { self.sector_count * SECTOR_SIZE as u64 }

    fn sector_size(&self) -> usize { SECTOR_SIZE }
}
