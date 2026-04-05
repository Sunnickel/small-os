#![no_std]
#![no_main]
extern crate alloc;

use bootloader_api::{BootInfo, BootloaderConfig, config::Mapping, entry_point};
use kernel::{init, serial_println, task::{Task, executor::Executor, shell::shell_task}};
use x86_64::instructions::{nop, port::Port};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum QemuExitCode {
    Success = 0x10,
    Failed = 0x11,
}

pub static BOOTLOADER_CONFIG: BootloaderConfig = {
    let mut config = BootloaderConfig::new_default();
    config.mappings.physical_memory = Some(Mapping::Dynamic);
    config
};

fn kernel_main(boot_info: &'static mut BootInfo) -> ! {
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

entry_point!(kernel_main, config = &BOOTLOADER_CONFIG);
