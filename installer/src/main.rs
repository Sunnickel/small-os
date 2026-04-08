#![no_std]
#![no_main]
extern crate alloc;

use boot::BootInfo;
use installer::{init, outb};

#[unsafe(no_mangle)]
#[unsafe(link_section = ".text.entry")]
pub extern "C" fn _start(boot_info: &'static mut BootInfo) -> ! {
    unsafe { outb(0x3F8, b'R'); }

    unsafe extern "C" {
        static mut __bss_start: u8;
        static mut __bss_end: u8;
    }

    unsafe {
        let start = core::ptr::addr_of_mut!(__bss_start);
        let end = core::ptr::addr_of_mut!(__bss_end);

        let len = end as usize - start as usize;

        if len > 0 {
            core::ptr::write_bytes(start, 0, len);
        }
    }
    init(boot_info);

    loop { x86_64::instructions::hlt(); }
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    unsafe {
        outb(0x3F8, b'P');
    }
    loop {
        x86_64::instructions::hlt();
    }
}
