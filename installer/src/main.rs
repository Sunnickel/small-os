#![no_std]
#![no_main]
extern crate alloc;

use core::panic;

use boot::BootInfo;
use installer::init;
use kernel::serial_println;

#[unsafe(no_mangle)]
#[unsafe(link_section = ".text.entry")]
pub extern "C" fn _start(boot_info: &'static mut BootInfo) -> ! {
    let ptr = boot_info as *mut BootInfo as u64;
    unsafe {
        let mut port = x86_64::instructions::port::Port::<u8>::new(0x3F8);
        for byte in b"ptr=" {
            port.write(*byte);
        }
        for i in (0..16).rev() {
            let nibble = ((ptr >> (i * 4)) & 0xF) as u8;
            port.write(if nibble < 10 { b'0' + nibble } else { b'a' + nibble - 10 });
        }
        port.write(b'\n');
    }

    unsafe {
        let mut port = x86_64::instructions::port::Port::<u8>::new(0x3F8);
        for char in "[kernel] Starting up \n".chars() {
            port.write(char as u8);
        }
    }

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

    loop {
        x86_64::instructions::hlt();
    }
}

#[panic_handler]
fn panic(_info: &panic::PanicInfo) -> ! {
    serial_println!("panic! {}\n", _info);
    loop {
        x86_64::instructions::hlt();
    }
}
