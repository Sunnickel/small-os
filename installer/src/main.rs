#![no_std]
#![no_main]
extern crate alloc;

use bootloader_api::{BootInfo, BootloaderConfig, config::Mapping, entry_point};
use installer::init;

pub static BOOTLOADER_CONFIG: BootloaderConfig = {
    let mut config = BootloaderConfig::new_default();
    config.mappings.physical_memory = Some(Mapping::Dynamic);
    config
};

fn kernel_main(boot_info: &'static mut BootInfo) -> ! {
    init(boot_info);
    loop {
        x86_64::instructions::hlt();
    }
}

entry_point!(kernel_main, config = &BOOTLOADER_CONFIG);
