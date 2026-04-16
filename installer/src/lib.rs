#![no_std]
#![no_main]
extern crate alloc;

use core::fmt::Debug;

use boot::BootInfo;
use driver::{
    fs::{
        DiskInfo,
        detect_disks,
        format_disk,
        get_disk_info,
        is_initialized,
    },
    pci,
};
use kernel::{
    memory,
    memory::{BootInfoFrameAllocator, dma_alloc::KernelDmaAllocator},
    serial_println,
};
use x86_64::VirtAddr;
use driver::fs::init_auto;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum QemuExitCode {
    Success = 0x10,
    Failed = 0x11,
}

pub fn init(boot_info: &'static mut BootInfo) {
    driver::util::set_debug_hook(|msg| serial_println!("{}", msg));

    let phys_mem_offset = VirtAddr::new(boot_info.physical_memory_offset);

    serial_println!("Mapping memory...");
    let mut mapper = unsafe { memory::init(phys_mem_offset) };

    serial_println!("Frame allocator initializing...");
    let mut frame_allocator = unsafe {
        BootInfoFrameAllocator::init_from_raw(boot_info.memory_map, boot_info.memory_map_len)
    };

    serial_println!("mmap ptr={:#x} len={}", boot_info.memory_map, boot_info.memory_map_len);
    for i in 0..boot_info.memory_map_len.min(16) {
        let base =
            unsafe { core::ptr::read_unaligned((boot_info.memory_map + i * 24) as *const u64) };
        let len =
            unsafe { core::ptr::read_unaligned((boot_info.memory_map + i * 24 + 8) as *const u64) };
        let kind = unsafe {
            core::ptr::read_unaligned((boot_info.memory_map + i * 24 + 16) as *const u32)
        };
        serial_println!("  [{:2}] base={:#018x} len={:#018x} type={}", i, base, len, kind);
    }

    serial_println!("Heap initializing...");
    match memory::alloc::init_heap(&mut mapper, &mut frame_allocator) {
        Err(e) => unsafe {
            serial_println!("Heap couldn't be initialized! {:?}", e);
        },
        Ok(_) => {}
    }

    serial_println!("DMA allocator initializing...");
    let mut dma = KernelDmaAllocator::new(&mut frame_allocator, phys_mem_offset.as_u64());

    // ── Filesystem / disk init ──
    serial_println!("Scanning PCI bus for build disk...");

    pci::init_ecam(0xB0000000, phys_mem_offset.as_u64());

    // Step 1: Detect Disks
    let disks = detect_disks();
    if disks.is_empty() {
        serial_println!("No disks detected!");
        exit_qemu(QemuExitCode::Failed);
    }

    serial_println!("Found {} disk(s)", disks.len());
    for (i, disk) in disks.iter().enumerate() {
        serial_println!("  {}: {} ({:?})", i, disk.id, disk.disk_type);
    }

    init_auto(phys_mem_offset.as_u64(), &mut dma).expect("No Devices");

    let phys_offset = phys_mem_offset.as_u64();

    let target_disk =
        disks.iter().find(|d| d.role == DiskRole::Data).expect("Unable to find a proper device");

    // Step 2: Check if disk has GPT/NTFS already
    match get_disk_info(target_disk, phys_offset, &mut dma) {
        Ok(info) => {
            serial_println!("Disk info:\n{}", info);
            // Disk has GPT, try to mount existing NTFS
            if !info.contains("NTFS") {
                serial_println!("No NTFS partition found, need to format");
                format_and_mount(target_disk, phys_offset, &mut dma);
            } else {
                serial_println!("NTFS found, mounting...");
                mount_disk(target_disk, phys_offset, &mut dma);
            }
        }
        Err(e) => {
            serial_println!("No valid GPT ({}), formatting disk...", e);
            format_and_mount(target_disk, phys_offset, &mut dma);
        }
    }

    if is_initialized() {
        serial_println!("Filesystem mounted successfully!");
        // Now you can use fs::fs_mutex().lock() to access files
        test_filesystem();
    } else {
        serial_println!("Failed to mount filesystem!");
        exit_qemu(QemuExitCode::Failed);
    }

    serial_println!("Installer ready!");
}

fn format_and_mount(disk: &DiskInfo, phys_offset: u64, dma: &mut impl driver::dma::DmaAllocator) {
    serial_println!("Formatting disk with GPT + NTFS...");

    match format_disk(disk, phys_offset, dma) {
        Ok(part) => {
            serial_println!(
                "Created partition: LBA {}-{} ({} MB)",
                part.start_lba,
                part.end_lba,
                part.size_bytes / 1024 / 1024
            );
            // Now mount it
            mount_disk(disk, phys_offset, dma);
        }
        Err(e) => {
            serial_println!("Format failed: {}", e);
        }
    }
}

fn mount_disk(disk: &DiskInfo, phys_offset: u64, dma: &mut impl driver::dma::DmaAllocator) {
    serial_println!("Mounting NTFS filesystem...");

    if let Err(e) = driver::fs::init_driver(disk, phys_offset, dma) {
        serial_println!("Mount failed: {}", e);
    }
}

fn test_filesystem() {
    use driver::fs::fs_mutex;

    let mut fs = fs_mutex().lock();

    // Try to open root directory
    match fs.root_directory() {
        Ok(root) => {
            serial_println!("Opened root directory (record {})", root.record_number());

            // List directory contents
            match fs.list_directory(&root) {
                Ok(entries) => {
                    serial_println!("Root directory entries: {:?}", entries);
                }
                Err(e) => {
                    serial_println!("Failed to list directory: {:?}", e);
                }
            }
        }
        Err(e) => {
            serial_println!("Failed to open root: {:?}", e);
        }
    }
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
