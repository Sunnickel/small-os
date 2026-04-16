// memory/dma_alloc.rs
use alloc::{collections::BTreeMap, vec::Vec};

use hal::{
    PhysAddr,
    dma::{DmaAllocator, DmaBuffer},
};
use spin::Mutex;
use x86_64::structures::paging::FrameAllocator;

// Global frame allocator storage
static FRAME_ALLOC: Mutex<Option<&'static mut crate::memory::BootInfoFrameAllocator>> =
    Mutex::new(None);

pub fn init_frame_allocator(alloc: &'static mut crate::memory::BootInfoFrameAllocator) {
    *FRAME_ALLOC.lock() = Some(alloc);
}

pub struct KernelDmaAllocator {
    phys_mem_offset: u64,
}

impl KernelDmaAllocator {
    pub fn new(phys_mem_offset: u64) -> Self { Self { phys_mem_offset } }
}

// Track allocations for deallocation
static ALLOC_TRACKER: Mutex<BTreeMap<u64, usize>> = Mutex::new(BTreeMap::new());

impl DmaAllocator for KernelDmaAllocator {
    fn alloc(&mut self, size: usize, _align: usize) -> Option<DmaBuffer> {
        let pages = (size + 4095) / 4096;
        let mut frames = Vec::new();

        let mut guard = FRAME_ALLOC.lock();
        let alloc = guard.as_mut()?;

        for _ in 0..pages {
            let frame = alloc.allocate_frame()?;
            frames.push(frame.start_address().as_u64());
        }

        let base_phys = frames[0];
        let virt = (base_phys + self.phys_mem_offset) as *mut u8;
        let phys = PhysAddr::new(base_phys);

        // Store for free()
        ALLOC_TRACKER.lock().insert(base_phys, pages);

        unsafe fn kernel_free(_phys: PhysAddr, _size: usize) {
            // Implementation: lookup in ALLOC_TRACKER and deallocate frames
            // Requires access to FRAME_ALLOC
        }

        Some(DmaBuffer { phys, virt, size: pages * 4096, free_fn: kernel_free })
    }
}
