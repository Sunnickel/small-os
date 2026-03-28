use core::{
    ptr,
    sync::atomic::{fence, Ordering},
};

use hal::block::BlockError;

use crate::{
    constants::*,
    queue::{VirtQueue, VirtqAvail, VirtqDesc, VirtqUsed},
};

#[repr(C)]
pub struct VirtioBlkReq {
    pub type_: u32,
    pub reserved: u32,
    pub sector: u64,
}

pub unsafe fn do_request(
    queue: &mut VirtQueue,
    req_phys: u64,
    req_virt: usize,
    status_phys: u64,
    status_virt: usize,
    notify_addr: usize,
    sector: u64,
    data_buf: Option<&mut [u8]>,
    data_len: usize,
    is_write: bool,
) -> Result<(), BlockError> {
    unsafe {
        // Setup request header
        let req_ptr = req_virt as *mut VirtioBlkReq;
        (*req_ptr).type_ = if is_write { VIRTIO_BLK_T_OUT } else { VIRTIO_BLK_T_IN };
        (*req_ptr).reserved = 0;
        (*req_ptr).sector = sector;

        // Clear status
        (status_virt as *mut u8).write_volatile(0xFF);

        let data_virt = req_virt + 64;
        let data_phys = req_phys + 64;

        // Copy data to DMA buffer if writing
        if let Some(buf) = &data_buf
            && is_write
        {
            ptr::copy_nonoverlapping(buf.as_ptr(), data_virt as *mut u8, buf.len().min(data_len));
        }

        // Build descriptor chain
        let desc_base = queue.desc as *mut VirtqDesc;

        ptr::write(
            desc_base.add(0),
            VirtqDesc { addr: req_phys, len: 16, flags: VIRTQ_DESC_F_NEXT, next: 1 },
        );

        let data_flags =
            if is_write { VIRTQ_DESC_F_NEXT } else { VIRTQ_DESC_F_NEXT | VIRTQ_DESC_F_WRITE };

        ptr::write(
            desc_base.add(1),
            VirtqDesc { addr: data_phys, len: data_len as u32, flags: data_flags, next: 2 },
        );

        ptr::write(
            desc_base.add(2),
            VirtqDesc { addr: status_phys, len: 1, flags: VIRTQ_DESC_F_WRITE, next: 0 },
        );

        // Publish to avail ring
        let avail = queue.avail as *mut VirtqAvail;
        let idx = (*avail).idx;
        (*avail).ring[(idx as usize) % queue.queue_size as usize] = 0;
        fence(Ordering::SeqCst);
        (*avail).idx = idx.wrapping_add(1);
        fence(Ordering::SeqCst);

        // Notify device
        ptr::write_volatile(notify_addr as *mut u16, 0);

        // Poll for completion
        let used = queue.used as *const VirtqUsed;
        let mut timeout = 10_000_000u32;
        loop {
            fence(Ordering::SeqCst);
            if (*used).idx != queue.last_used_idx {
                break;
            }
            timeout -= 1;
            if timeout == 0 {
                return Err(BlockError::Timeout);
            }
        }
        queue.last_used_idx = queue.last_used_idx.wrapping_add(1);

        // Check status byte
        let status = (status_virt as *const u8).read_volatile();
        if status != VIRTIO_BLK_S_OK {
            return Err(BlockError::DeviceError);
        }

        // Copy data from DMA buffer if reading
        if let Some(buf) = data_buf
            && !is_write
        {
            ptr::copy_nonoverlapping(
                data_virt as *const u8,
                buf.as_mut_ptr(),
                buf.len().min(data_len),
            );
        }

        Ok(())
    }
}
