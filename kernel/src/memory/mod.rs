pub mod alloc;
pub mod bump;
pub mod dma_alloc;
pub mod types;

use boot::{MemoryRegion, MemoryRegionKind};
use x86_64::{
    PhysAddr,
    VirtAddr,
    structures::paging::{FrameAllocator, OffsetPageTable, PageTable, PhysFrame, Size4KiB},
};

pub unsafe fn init(physical_memory_offset: VirtAddr) -> OffsetPageTable<'static> {
    unsafe {
        let level_4_table = active_level_4_table(physical_memory_offset);
        OffsetPageTable::new(level_4_table, physical_memory_offset)
    }
}

pub unsafe fn active_level_4_table(physical_memory_offset: VirtAddr) -> &'static mut PageTable {
    use x86_64::registers::control::Cr3;

    let (level_4_table_frame, _) = Cr3::read();
    let phys = level_4_table_frame.start_address();
    let virt = physical_memory_offset + phys.as_u64();
    let page_table_ptr: *mut PageTable = virt.as_mut_ptr();

    unsafe { &mut *page_table_ptr }
}

pub struct EmptyFrameAllocator;

unsafe impl FrameAllocator<Size4KiB> for EmptyFrameAllocator {
    fn allocate_frame(&mut self) -> Option<PhysFrame> { None }
}

pub unsafe fn translate_addr(addr: VirtAddr, physical_memory_offset: VirtAddr) -> Option<PhysAddr> {
    translate_addr_inner(addr, physical_memory_offset)
}

fn translate_addr_inner(addr: VirtAddr, physical_memory_offset: VirtAddr) -> Option<PhysAddr> {
    use x86_64::{registers::control::Cr3, structures::paging::page_table::FrameError};

    let (level_4_table_frame, _) = Cr3::read();
    let table_indexes = [addr.p4_index(), addr.p3_index(), addr.p2_index(), addr.p1_index()];
    let mut frame = level_4_table_frame;

    for &index in &table_indexes {
        let virt = physical_memory_offset + frame.start_address().as_u64();
        let table_ptr: *const PageTable = virt.as_ptr();
        let table = unsafe { &*table_ptr };

        let entry = &table[index];
        frame = match entry.frame() {
            Ok(frame) => frame,
            Err(FrameError::FrameNotPresent) => return None,
            Err(FrameError::HugeFrame) => panic!("huge pages not supported"),
        };
    }

    Some(frame.start_address() + u64::from(addr.page_offset()))
}

pub struct BootInfoFrameAllocator {
    memory_map: &'static [MemoryRegion],
    next: usize,
}

impl BootInfoFrameAllocator {
    pub unsafe fn init(memory_map: &'static [MemoryRegion]) -> Self {
        BootInfoFrameAllocator { memory_map, next: 0 }
    }

    pub unsafe fn init_from_raw(ptr: u64, len: u64) -> Self {
        static mut REGIONS: [MemoryRegion; 256] =
            [MemoryRegion { start: 0, end: 0, kind: MemoryRegionKind::Unknown }; 256];

        let mut count = 0;
        for i in 0..len {
            let base = unsafe { core::ptr::read_unaligned((ptr + i * 24) as *const u64) };
            let length = unsafe { core::ptr::read_unaligned((ptr + i * 24 + 8) as *const u64) };
            let entry_type =
                unsafe { core::ptr::read_unaligned((ptr + i * 24 + 16) as *const u32) };

            if count >= 256 {
                break;
            }
            if length == 0 {
                continue;
            }

            let kind = match entry_type {
                1 => MemoryRegionKind::Usable,
                2 => MemoryRegionKind::Reserved,
                3 => MemoryRegionKind::AcpiReclaimable,
                4 => MemoryRegionKind::AcpiNvs,
                5 => MemoryRegionKind::BadMemory,
                _ => MemoryRegionKind::Unknown,
            };

            unsafe {
                REGIONS[count] = MemoryRegion { start: base, end: base + length, kind };
            }
            count += 1;
        }

        BootInfoFrameAllocator { memory_map: unsafe { &REGIONS[..count] }, next: 0 }
    }

    fn usable_frames(&self) -> impl Iterator<Item = PhysFrame> + '_ {
        // Skip first 32MB — bootloader, page tables, ELF, scratch buffer
        const ALLOC_START: u64 = 0x200_0000;

        self.memory_map
            .iter()
            .filter(|r| r.kind == MemoryRegionKind::Usable)
            .flat_map(move |r| {
                let start = r.start.max(ALLOC_START);
                let end = r.end;
                if start >= end { (0u64..0u64).step_by(4096) } else { (start..end).step_by(4096) }
            })
            .map(|addr| PhysFrame::<Size4KiB>::containing_address(PhysAddr::new(addr)))
    }
}

unsafe impl FrameAllocator<Size4KiB> for BootInfoFrameAllocator {
    fn allocate_frame(&mut self) -> Option<PhysFrame> {
        let frame = self.usable_frames().nth(self.next);
        self.next += 1;
        frame
    }
}
