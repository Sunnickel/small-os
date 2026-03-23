#![no_std]
#![no_main]
#![feature(custom_test_frameworks)]
#![test_runner(crate::test_runner)]
#![reexport_test_harness_main = "test_main"]

pub mod frame_buffer;
pub mod port;
pub mod qemu;
mod macros;

use crate::macros::_print;
use crate::frame_buffer::Writer;
use bootloader_api::{entry_point, BootInfo};
use core::panic::PanicInfo;

entry_point!(kernel_main);

fn kernel_main(boot_info: &'static mut BootInfo) -> ! {
    init(boot_info);
    loop {}
}

fn init(boot_info: &'static mut BootInfo) {
    let framebuffer = boot_info.framebuffer.as_mut().unwrap();
    let info = framebuffer.info();
    let buffer = framebuffer.buffer_mut();

    Writer::init(buffer, info);

    println!("Hello OS!");
    println!("Hello OS!");
    println!("Hello OS!");
    println!("Hello OS!");
    println!("Hello OS!");
    println!("Hello OS!");

    #[cfg(test)]
    run_tests();
}

#[cfg(test)]
fn run_tests() {
    test_main();
    use crate::qemu::{exit_qemu, QemuExitCode};
    exit_qemu(QemuExitCode::Success);
}

#[cfg(not(test))]
#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    loop {}
}

#[cfg(test)]
#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    use crate::qemu::{exit_qemu, QemuExitCode};
    exit_qemu(QemuExitCode::Failure)
}

#[cfg(test)]
pub fn test_runner(tests: &[&dyn Fn()]) {
    for test in tests {
        test();
    }
    use crate::qemu::{exit_qemu, QemuExitCode};
    exit_qemu(QemuExitCode::Success);
}


#[test_case]
fn trivial_assertion() {
    print!("trivial assertion... ");
    assert_eq!(1, 1);
    println!("[ok]");
}
