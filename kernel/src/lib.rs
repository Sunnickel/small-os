#![no_std]
#![no_main]
#![feature(abi_x86_interrupt)]

use bus::Bus;
extern crate alloc;

#[macro_use]
mod macros;

pub mod device;
pub mod flags;
pub mod interrupts;
pub mod memory;
pub mod screen;
pub mod task;
pub mod driver;

use alloc::boxed::Box;

use boot::BootInfo;
use flags::*;
use hal::dma::DmaAllocator;
pub use macros::{_print, _print_raw, _print_serial};
use spin::Mutex;
use x86_64::VirtAddr;

use crate::{
    device::registry,
    interrupts::{
        gdt,
        hardware_interrupt::{PICS, enable_interrupts},
    },
    memory::{BootInfoFrameAllocator, dma_alloc::KernelDmaAllocator},
};

pub fn init(boot_info: &'static mut BootInfo) {
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

    memory::alloc::init_heap(&mut mapper, &mut frame_allocator)
        .expect("heap initialization failed");

    // ── DMA / Drivers ──
    // Leak frame allocator to make it 'static
    let frame_alloc_static = Box::leak(Box::new(frame_allocator));
    memory::dma_alloc::init_frame_allocator(frame_alloc_static);

    // Create and leak DMA allocator
    let dma_alloc =
        Box::leak(Box::new(Mutex::new(KernelDmaAllocator::new(phys_mem_offset.as_u64()))))
            as &'static Mutex<dyn DmaAllocator + Send + Sync>;

    ::driver::init(phys_mem_offset.as_u64(), dma_alloc);
    serial_println!("6. Driver subsystem initialized");

    // ── ACPI / PCI ──
    let acpi = hal::acpi::init_from_rsdp(boot_info.rsdp_addr as usize, phys_mem_offset.as_u64())
        .expect("ACPI init failed");
    hal::pci::init_ecam(acpi.ecam_base, acpi.phys_offset);
    serial_println!("7. ACPI initialized");

    // Enumerate PCI devices into device registry
    let pci_bus = bus::pci::PciBus;
    pci_bus.enumerate(registry()).expect("PCI enumeration failed");
    serial_println!("8. PCI enumerated: {:?} Devices", registry().len());


    // ── Filesystem ──
    // Extract first block device for root filesystem
    // if let Some((device_id, block_dev)) = registry().take_block_device() {
    //     serial_println!("10. Block device found: {:?}", device_id);
    //
    //     // Initialize NTFS filesystem
    //     let vfs = Vfs::new();
    //     let ntfs_fs = Box::new(NtfsDriver::<block_dev>);
    //     vfs.mount(*ntfs_fs, Path::new("/").expect("couldnt create path"))
    //         .expect("Failed to mount root filesystem");
    //     serial_println!("11. Root filesystem mounted");
    // } else {
    //     serial_println!("WARNING: No block device found for root filesystem");
    // }
}
