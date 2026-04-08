use core::{
    alloc::{GlobalAlloc, Layout},
    ptr,
};

use x86_64::structures::paging::{Page, PageTableFlags as Flags};
use x86_64::{
    VirtAddr,
    structures::paging::{FrameAllocator, Size4KiB},
};

use crate::{flags::GLOBAL_ALLOCATOR, memory::bump::BumpAllocator, outb};

/// The heap memory range.
pub const HEAP_START: usize = 0x100000;
pub const HEAP_SIZE: usize = 100 * 1024;

/// Align the given address upwards to the given alignment.
/// `align` must be a power of two.
fn align_up(addr: usize, align: usize) -> usize { (addr + align - 1) & !(align - 1) }

unsafe impl GlobalAlloc for Locked<BumpAllocator> {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let mut bump = self.lock();

        let alloc_start = align_up(bump.next, layout.align());
        let alloc_end = match alloc_start.checked_add(layout.size()) {
            Some(end) => end,
            None => return ptr::null_mut(),
        };

        if alloc_end > bump.heap_end {
            ptr::null_mut() // out of memory
        } else {
            bump.next = alloc_end;
            bump.allocations += 1;
            alloc_start as *mut u8
        }
    }

    unsafe fn dealloc(&self, _ptr: *mut u8, _layout: Layout) {
        let mut bump = self.lock();
        bump.allocations -= 1;
        if bump.allocations == 0 {
            // Only reclaim all memory when every allocation has been freed
            bump.next = bump.heap_start;
        }
    }
}

/// A spinlock wrapper so we can safely mutate the allocator from multiple
/// contexts.
pub struct Locked<A> {
    inner: spin::Mutex<A>,
}

impl<A> Locked<A> {
    pub const fn new(inner: A) -> Self { Locked { inner: spin::Mutex::new(inner) } }

    pub fn lock(&'_ self) -> spin::MutexGuard<'_, A> { self.inner.lock() }
}

/// Maps the heap pages and initializes the allocator.
/// Call this once during kernel init after the frame allocator and mapper are
/// ready.
pub fn init_heap(
    mapper: &mut impl x86_64::structures::paging::Mapper<Size4KiB>,
    frame_allocator: &mut impl FrameAllocator<Size4KiB>,
) -> Result<(), x86_64::structures::paging::mapper::MapToError<Size4KiB>> {
    let page_range = {
        let heap_start = VirtAddr::new(HEAP_START as u64);
        let heap_end = heap_start + HEAP_SIZE as u64 - 1u64;
        let heap_start_page = Page::containing_address(heap_start);
        let heap_end_page = Page::containing_address(heap_end);
        Page::range_inclusive(heap_start_page, heap_end_page)
    };

    for page in page_range {
        let frame = frame_allocator
            .allocate_frame()
            .ok_or({
                unsafe { outb(0x3F8, b'!'); }
                x86_64::structures::paging::mapper::MapToError::FrameAllocationFailed
            })?;
        let flags = Flags::PRESENT | Flags::WRITABLE;
        unsafe { mapper.map_to(page, frame, flags, frame_allocator)?.flush() };
    }

    unsafe {
        GLOBAL_ALLOCATOR.lock().init(HEAP_START, HEAP_SIZE);
    }

    Ok(())
}
