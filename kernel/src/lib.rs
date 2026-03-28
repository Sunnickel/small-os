#![no_std]
#![feature(abi_x86_interrupt)]

extern crate alloc;

#[macro_use]
mod macros;

pub mod flags;
pub mod interrupts;
pub mod memory;
pub mod screen;
pub mod task;

use bootloader_api::BootInfo;
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
    // ── Screen ────────────────────────────────────────────────────────────────
    let framebuffer = boot_info.framebuffer.as_mut().unwrap();
    let info = framebuffer.info();
    let buffer = framebuffer.buffer_mut();
    screen::Writer::init(buffer, info);
    serial_println!("1. Screen initialized");

    // ── GDT / IDT ─────────────────────────────────────────────────────────────
    gdt::init();
    serial_println!("2. GDT initialized");

    interrupts::init_idt();
    serial_println!("3. IDT initialized");

    // ── PICs ──────────────────────────────────────────────────────────────────
    unsafe {
        PICS.lock().initialize();
        enable_interrupts();
    }
    serial_println!("4. PICS initialized");

    x86_64::instructions::interrupts::enable();
    serial_println!("5. Interrupts enabled");

    // ── Memory ────────────────────────────────────────────────────────────────
    let phys_mem_offset = VirtAddr::new(boot_info.physical_memory_offset.into_option().unwrap());

    let mut mapper = unsafe { memory::init(phys_mem_offset) };

    let mut frame_allocator = unsafe { BootInfoFrameAllocator::init(&boot_info.memory_regions) };

    serial_println!("6. Memory / frame allocator initialized");

    memory::alloc::init_heap(&mut mapper, &mut frame_allocator)
        .expect("heap initialization failed");
    serial_println!("7. Heap allocator initialized");

    // ── File System
    // ────────────────────────────────────────────────────────────────
    serial_println!("8. Scanning PCI bus...");

    driver::pci::assign_bar64(0, 4, 0, 0x20, 0x8200_0000u64);
    let mut dma = KernelDmaAllocator::new(&mut frame_allocator, phys_mem_offset.as_u64());
    driver::fs::init_auto(phys_mem_offset.as_u64(), &mut dma).expect("no block device found");
    serial_println!("9. Filesystem initialized");
}

#[cfg(not(test))]
#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    serial_println!("[kernel panic] {}", info);
    loop {
        x86_64::instructions::hlt();
    }
}
