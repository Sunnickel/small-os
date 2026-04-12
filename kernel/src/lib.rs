#![no_std]
#![no_main]
#![feature(abi_x86_interrupt)]

extern crate alloc;

#[macro_use]
mod macros;

pub mod flags;
pub mod interrupts;
pub mod memory;
pub mod screen;
pub mod task;

use boot::BootInfo;
use flags::*;
pub use macros::{_print, _print_raw, _print_serial};
use x86_64::VirtAddr;

use crate::{
    interrupts::{
        gdt,
        hardware_interrupt::{PICS, enable_interrupts},
    },
    memory::{BootInfoFrameAllocator, dma_alloc::KernelDmaAllocator},
};

pub fn init(boot_info: &'static mut BootInfo) {
    driver::util::set_debug_hook(|msg| serial_println!("{}", msg));

    // ── Screen ──
    let fb_info = boot_info.framebuffer;
    let buffer =
        unsafe { core::slice::from_raw_parts_mut(fb_info.addr as *mut u8, fb_info.size as usize) };
    screen::Writer::init(buffer, fb_info);
    serial_println!("1. Screen initialized");

    // ── GDT / IDT ──
    gdt::init();
    serial_println!("2. GDT initialized");
    interrupts::init_idt();
    serial_println!("3. IDT initialized");

    // ── PICs ──
    unsafe {
        PICS.lock().initialize();
        enable_interrupts();
    }
    serial_println!("4. PICS initialized");
    x86_64::instructions::interrupts::enable();
    serial_println!("5. Interrupts enabled");

    // ── Memory ──
    let phys_mem_offset = VirtAddr::new(boot_info.physical_memory_offset);
    let mut mapper = unsafe { memory::init(phys_mem_offset) };
    let mut frame_allocator = unsafe {
        BootInfoFrameAllocator::init_from_raw(boot_info.memory_map, boot_info.memory_map_len)
    };
    serial_println!("6. Memory initialized");

    memory::alloc::init_heap(&mut mapper, &mut frame_allocator)
        .expect("heap initialization failed");
    serial_println!("7. Heap initialized");

    // ── ACPI / FS ──
    let mut dma = KernelDmaAllocator::new(&mut frame_allocator, phys_mem_offset.as_u64());
    driver::acpi::init_from_rsdp(boot_info.rsdp_addr as usize, phys_mem_offset.as_u64());
    driver::fs::init_auto(phys_mem_offset.as_u64(), &mut dma).expect("no block device found");
    serial_println!("8. Filesystem initialized");
}
