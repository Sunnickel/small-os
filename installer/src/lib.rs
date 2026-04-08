#![no_std]
#![no_main]
extern crate alloc;

use driver::fs::{DiskInfo, detect_disks, format_disk, get_disk_info, is_initialized};
use kernel::{
    memory,
    memory::{BootInfoFrameAllocator, dma_alloc::KernelDmaAllocator},
    serial_println,
};
use x86_64::VirtAddr;
use boot::BootInfo;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum QemuExitCode {
    Success = 0x10,
    Failed = 0x11,
}

pub fn init(boot_info: &'static mut BootInfo) {
    unsafe {
        // Print the number of memory regions found
        let len = boot_info.memory_map_len;
        if len == 0 {
            outb(0x3F8, b'0'); // '0' regions found
        } else {
            outb(0x3F8, (len as u8).wrapping_add(b'0')); // Prints the digit
        }
    }

    driver::util::set_debug_hook(|msg| serial_println!("{}", msg));

    let phys_mem_offset = VirtAddr::new(boot_info.physical_memory_offset);

    let mut mapper = unsafe { memory::init(phys_mem_offset) };

    let mut frame_allocator = unsafe {
        BootInfoFrameAllocator::init_from_raw(
            boot_info.memory_map,
            boot_info.memory_map_len,
        )
    };

    // In init()
    match memory::alloc::init_heap(&mut mapper, &mut frame_allocator) {
        Ok(_) => unsafe { outb(0x3F8, b'f'); },
        Err(_) => unsafe { outb(0x3F8, b'X'); }, // 'X' for Error
    }

    unsafe { outb(0x3F8, b'H'); }
    let mut dma = KernelDmaAllocator::new(&mut frame_allocator, phys_mem_offset.as_u64());

    // ── Filesystem / disk init ──
    serial_println!("Scanning PCI bus for installer disk...");

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

    let target_disk = &disks[0];
    let phys_offset = phys_mem_offset.as_u64();

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

fn format_and_mount(
    disk: &DiskInfo,
    phys_offset: u64,
    dma: &mut impl driver::dma::DmaAllocator,
) {
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

fn mount_disk(
    disk: &DiskInfo,
    phys_offset: u64,
    dma: &mut impl driver::dma::DmaAllocator,
) {
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

pub unsafe fn outb(port: i32, char: u8) {
    unsafe {
        core::arch::asm!(
        "out dx, al",
        in("dx") port,
        in("al") char,
        );
    }
}