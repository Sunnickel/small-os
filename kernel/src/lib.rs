#![no_std]
#![feature(abi_x86_interrupt)]

pub mod interrupts;
mod memory;
pub mod screen;
#[macro_use]
mod macros;

use crate::interrupts::hardware_interrupt::{enable_interrupts, PICS};
use crate::interrupts::{gdt, get_key};
use crate::macros::flush_interrupt_buffer;
use bootloader_api::BootInfo;
use core::sync::atomic::Ordering;
pub use macros::{_print, _print_raw, _print_serial};
pub use macros::{INTERRUPT_BUFFER, KEYBOARD_EVENTS, TIMER_TICKS};
use pc_keyboard::{layouts, DecodedKey, HandleControl, Keyboard, ScancodeSet1};
use x86_64::structures::paging::{Page, Translate};
use x86_64::VirtAddr;

pub fn init(boot_info: &'static mut BootInfo) {
    let framebuffer = boot_info.framebuffer.as_mut().unwrap();
    let info = framebuffer.info();
    let buffer = framebuffer.buffer_mut();
    screen::Writer::init(buffer, info);

    serial_println!("1. Screen initialized");

    gdt::init();
    serial_println!("2. GDT initialized");

    interrupts::init_idt();
    serial_println!("3. IDT initialized");

    unsafe {
        PICS.lock().initialize();
        enable_interrupts()
    };
    serial_println!("4. PICS initialized");

    x86_64::instructions::interrupts::enable();
    serial_println!("5. Interrupts enabled");

    let phys_mem_offset = VirtAddr::new(boot_info.physical_memory_offset.take().expect("no physical memory offset"));
    let mut mapper = unsafe { memory::init(phys_mem_offset) };
    let mut frame_allocator = memory::EmptyFrameAllocator;

    let page = Page::containing_address(VirtAddr::new(0));
    memory::create_example_mapping(page, &mut mapper, &mut frame_allocator);

    let page_ptr: *mut u64 = page.start_address().as_mut_ptr();
    unsafe { page_ptr.offset(400).write_volatile(0x_f021_f077_f065_f04e)};
    serial_println!("6. Memory Pages loaded");
}

pub fn hlt_loop() -> ! {
    serial_println!("Entering hlt_loop");

    let mut keyboard = Keyboard::new(
        ScancodeSet1::new(),
        layouts::Us104Key,
        HandleControl::Ignore,
    );

    let mut last_timer = 0;
    loop {
        x86_64::instructions::interrupts::enable();
        x86_64::instructions::hlt();
        flush_interrupt_buffer();

        while let Some(scancode) = get_key() {
            if let Ok(Some(key_event)) = keyboard.add_byte(scancode) {
                if let Some(key) = keyboard.process_keyevent(key_event) {
                    match key {
                        DecodedKey::Unicode(c) => print!("{}", c),
                        DecodedKey::RawKey(k) => serial_println!("Raw: {:?}", k),
                    }
                }
            }
        }

        let timer = TIMER_TICKS.load(Ordering::Relaxed);
        if timer != last_timer {
            last_timer = timer;
        }
    }
}
