#![no_std]
extern crate alloc;

pub mod acpi_wrapper;
pub mod cluster;
pub mod partition;
pub mod pci;
pub mod stream;

pub static mut DEBUG_HOOK: Option<fn(&str)> = None;

pub fn set_debug_hook(f: fn(&str)) {
    unsafe {
        DEBUG_HOOK = Some(f);
    }
}

pub fn debug(msg: &str) {
    unsafe {
        if let Some(f) = DEBUG_HOOK {
            f(msg);
        }
    }
}
