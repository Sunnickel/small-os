#![no_std]
#![no_main]
use bus::Bus;
extern crate alloc;

use spin::Mutex;
use alloc::boxed::Box;
use alloc::sync::Arc;
use core::fmt::Debug;

use boot::BootInfo;
use kernel::{
    memory,
    memory::{BootInfoFrameAllocator, dma_alloc::KernelDmaAllocator},
    serial_println,
};
use x86_64::VirtAddr;
use hal::dma::DmaAllocator;

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

    // Map BAR5 here, before init_frame_allocator borrows frame_alloc_static
    for (id, dev) in kernel::device::registry().by_type(device::DeviceType::Bus) {
        if let Some(pci) = dev.as_any().downcast_ref::<bus::pci::PciBusDevice>() {
            let (vendor, device) = pci.id_pair();

            if vendor == 0x8086 && device == 0x2922 {
                if let Some(bar5_phys) = pci.info().bar_mmio(5) {
                    serial_println!("[ahci] mapping BAR5 at 0x{:08x}", bar5_phys.as_u64());

                    use x86_64::{structures::paging::*, PhysAddr, VirtAddr};
                    use x86_64::structures::paging::FrameAllocator;

                    let flags = PageTableFlags::PRESENT
                        | PageTableFlags::WRITABLE
                        | PageTableFlags::NO_CACHE;

                    let start_addr = VirtAddr::new(bar5_phys.as_u64());
                    let end_addr = VirtAddr::new(bar5_phys.as_u64() + 0x4000);

                    let start_page = Page::<Size4KiB>::containing_address(start_addr);
                    let end_page = Page::<Size4KiB>::containing_address(end_addr);

                    // Shadow variable to allow reborrowing
                    let alloc: &mut _ = frame_alloc_static;
                    for page in Page::range(start_page, end_page) {
                        let frame = PhysFrame::containing_address(PhysAddr::new(page.start_address().as_u64()));
                        unsafe {
                            mapper.map_to(page, frame, flags, alloc)
                                .expect("AHCI BAR5 map failed")
                                .flush();
                        }
                    }
                }
            }
        }
    }

    // NOW init the frame allocator after we're done with it for BAR5 mapping
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

    match pci_bus.enumerate(kernel::device::registry()) {
        Ok(_) => {
            serial_println!(
                "[pci] enumeration complete: {} devices",
                kernel::device::registry().len()
            );
        }
        Err(e) => {
            serial_println!("[pci] ENUMERATION FAILED");
            serial_println!("[pci] error = {:?}", e);
            panic!("PCI enumeration failed");
        }
    }

    serial_println!("[driver] registering drivers...");
    let drv_registry = kernel::driver::registry();
    drv_registry.register_driver(Arc::new(driver::block::virtio::VirtioBlkDriver));
    drv_registry.register_driver(Arc::new(driver::block::ahci::AhciDriver));
    serial_println!("[driver] drivers registered");

    serial_println!("[driver] binding drivers to {} devices...", kernel::device::registry().len());
    drv_registry.bind_all(kernel::device::registry());
    serial_println!("[driver] bound {} drivers", drv_registry.bound_count());

    // Log every PCI device and whether it matched
    for (id, dev) in kernel::device::registry().by_type(device::DeviceType::Bus) {
        let pci = match dev.as_any().downcast_ref::<bus::pci::PciBusDevice>() {
            Some(p) => p,
            None => {
                serial_println!("[driver]   {:?} — not a PCI device", id);
                continue;
            }
        };
        let (vendor, device_id) = pci.id_pair();
        let info = pci.info();
        serial_println!(
        "[driver]   {:02x}:{:02x}.{} vendor={:04x} device={:04x} class={:02x} sub={:02x} — {}",
        info.address.bus, info.address.device, info.address.function,
        vendor, device_id,
        info.class, info.subclass,
        if drv_registry.binding_for(id).is_some() { "BOUND" } else { "no driver" }
    );
    }

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
