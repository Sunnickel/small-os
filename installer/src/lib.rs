#![no_std]
#![no_main]
use bus::Bus;
extern crate alloc;

use spin::Mutex;
use alloc::boxed::Box;
use core::fmt::Debug;

use boot::BootInfo;
use kernel::{
    memory,
    memory::{BootInfoFrameAllocator, dma_alloc::KernelDmaAllocator},
    serial_println,
};
use x86_64::VirtAddr;
use device::DeviceType;
use hal::dma::DmaAllocator;
use kernel::device::registry;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum QemuExitCode {
    Success = 0x10,
    Failed = 0x11,
}

pub fn init(boot_info: &'static mut BootInfo) {
    serial_println!("=== KERNEL INIT START ===");

    // ─────────────────────────────────────────────
    // MEMORY SETUP
    // ─────────────────────────────────────────────
    serial_println!("[mem] physical_memory_offset = 0x{:016x}", boot_info.physical_memory_offset);

    let phys_mem_offset = VirtAddr::new(boot_info.physical_memory_offset);

    serial_println!("[mem] initializing mapper...");
    let mut mapper = unsafe { memory::init(phys_mem_offset) };
    serial_println!("[mem] mapper ready");

    serial_println!(
        "[mem] memory_map @ 0x{:016x}, entries = {}",
        boot_info.memory_map,
        boot_info.memory_map_len
    );

    let mut frame_allocator = unsafe {
        BootInfoFrameAllocator::init_from_raw(
            boot_info.memory_map,
            boot_info.memory_map_len,
        )
    };

    serial_println!("[mem] frame allocator initialized");

    serial_println!("[mem] initializing heap...");
    memory::alloc::init_heap(&mut mapper, &mut frame_allocator)
        .expect("heap initialization failed");
    serial_println!("[mem] heap ready");

    // ─────────────────────────────────────────────
    // DMA / DRIVER SETUP
    // ─────────────────────────────────────────────
    serial_println!("[dma] leaking frame allocator...");
    let frame_alloc_static = Box::leak(Box::new(frame_allocator));
    memory::dma_alloc::init_frame_allocator(frame_alloc_static);
    serial_println!("[dma] frame allocator registered");

    serial_println!("[dma] creating DMA allocator...");
    let dma_alloc =
        Box::leak(Box::new(Mutex::new(KernelDmaAllocator::new(
            phys_mem_offset.as_u64(),
        )))) as &'static Mutex<dyn DmaAllocator + Send + Sync>;

    serial_println!("[driver] initializing driver subsystem...");
    driver::init(phys_mem_offset.as_u64(), dma_alloc);
    serial_println!("[driver] driver subsystem initialized");

    // ─────────────────────────────────────────────
    // ACPI
    // ─────────────────────────────────────────────
    serial_println!("[acpi] rsdp = 0x{:016x}", boot_info.rsdp_addr);

    let acpi = hal::acpi::init_from_rsdp(
        boot_info.rsdp_addr as usize,
        phys_mem_offset.as_u64(),
    )
        .expect("ACPI init failed");

    serial_println!("[acpi] initialized");
    serial_println!("[acpi] ecam_base = 0x{:016x}", acpi.ecam_base);
    serial_println!("[acpi] phys_offset = 0x{:016x}", acpi.phys_offset);

    // ─────────────────────────────────────────────
    // PCI INIT
    // ─────────────────────────────────────────────
    serial_println!("[pci] initializing ECAM...");
    hal::pci::init_ecam(acpi.ecam_base, acpi.phys_offset);
    serial_println!("[pci] ECAM ready");

    // ─────────────────────────────────────────────
    // PCI ENUMERATION (DETAILED)
    // ─────────────────────────────────────────────
    serial_println!("[pci] starting enumeration...");

    let pci_bus = bus::pci::PciBus;

    match pci_bus.enumerate(registry()) {
        Ok(_) => {
            serial_println!(
                "[pci] enumeration complete: {} devices",
                registry().len()
            );
        }
        Err(e) => {
            serial_println!("[pci] ENUMERATION FAILED");
            serial_println!("[pci] error = {:?}", e);
            panic!("PCI enumeration failed");
        }
    }

    let mut disks = 0;

    for (_, dev) in registry().by_type(DeviceType::Block) {
        if let Some(block) = dev.as_block() {
            serial_println!("[disk] {}", dev.name());

            let mut buf = [0u8; 512];
            block.read_blocks(0, &mut buf).unwrap();

            serial_println!("[disk] first bytes: {:02x} {:02x}", buf[0], buf[1]);

            disks += 1;
        }
    }

    serial_println!("total disks: {}", disks);
    serial_println!("=== KERNEL INIT DONE ===");
}

pub fn exit_qemu(exit_code: QemuExitCode) -> ! {
    use x86_64::instructions::{nop, port::Port};

    unsafe {
        let mut port = Port::new(0xF4);
        port.write(exit_code as u32);
    }

    loop {
        nop();
    }
}
