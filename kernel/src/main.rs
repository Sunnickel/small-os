#![no_std]
#![no_main]
extern crate alloc;

use core::panic;
use boot::BootInfo;
use kernel::{
    init,
    serial_println,
    task::{Task, executor::Executor, shell::shell_task},
};
use x86_64::instructions::{nop, port::Port};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum QemuExitCode {
    Success = 0x10,
    Failed = 0x11,
}

#[unsafe(no_mangle)]
#[unsafe(link_section = ".text.entry")]
pub extern "C" fn _start(boot_info: &'static mut BootInfo) -> ! {
    unsafe extern "C" {
        static mut __bss_start: u8;
        static mut __bss_end: u8;
    }

    unsafe {
        let start_ptr = core::ptr::addr_of_mut!(__bss_start);
        let end_ptr = core::ptr::addr_of_mut!(__bss_end);
        let size = end_ptr as usize - start_ptr as usize;

        // Zero out the memory
        core::ptr::write_bytes(start_ptr, 0, size);
    }

    init(boot_info);
    serial_println!("Starting up...");

    let mut executor = Executor::new();
    executor.spawn(Task::new(shell_task()));
    executor.run();
}

pub fn exit_qemu(exit_code: QemuExitCode) -> ! {
    unsafe {
        let mut port = Port::new(0xF4);
        port.write(exit_code as u32);
    }

    loop {
        nop();
    }
}

#[panic_handler]
fn panic(_info: &panic::PanicInfo) -> ! {
    loop {
        x86_64::instructions::hlt();
    }
}
