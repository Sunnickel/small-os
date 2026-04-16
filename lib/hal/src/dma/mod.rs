use crate::PhysAddr;

pub struct DmaBuffer {
    pub phys: PhysAddr,
    pub virt: *mut u8,
    pub size: usize,
    pub free_fn: unsafe fn(PhysAddr, usize), // set at alloc time
}

impl Drop for DmaBuffer {
    fn drop(&mut self) { unsafe { (self.free_fn)(self.phys, self.size) } }
}

unsafe impl Send for DmaBuffer {}
unsafe impl Sync for DmaBuffer {}

pub trait DmaAllocator {
    fn alloc(&mut self, size: usize, align: usize) -> Option<DmaBuffer>;
}
