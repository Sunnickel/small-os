use x86_64::structures::paging::FrameAllocator;
use driver::dma::DmaAllocator;

pub struct KernelDmaAllocator<'a> {
    frame_alloc: &'a mut crate::memory::BootInfoFrameAllocator,
    phys_mem_offset: u64,
}

impl<'a> KernelDmaAllocator<'a> {
    pub fn new(
        frame_alloc: &'a mut crate::memory::BootInfoFrameAllocator,
        phys_mem_offset: u64,
    ) -> Self {
        Self { frame_alloc, phys_mem_offset }
    }
}

impl DmaAllocator for KernelDmaAllocator<'_> {
    fn allocate_dma_page(&mut self) -> Option<(u64, usize)> {
        let frame = self.frame_alloc.allocate_frame()?;
        let phys = frame.start_address().as_u64();
        let virt = (phys + self.phys_mem_offset) as usize;
        Some((phys, virt))
    }
}