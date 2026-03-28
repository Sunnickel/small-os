pub trait DmaAllocator {
    fn allocate_dma_page(&mut self) -> Option<(u64, usize)>;
}
