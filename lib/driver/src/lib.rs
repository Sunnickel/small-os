#![no_std]
#![no_main]

extern crate alloc;

pub mod fs;

mod block;
mod core;

pub mod pci {
    pub use crate::core::pci::*;
}

pub mod dma {
    pub use hal::dma::DmaAllocator;
}

pub mod acpi {
    pub use crate::core::acpi_wrapper::*;
}

pub mod util {
    static mut DEBUG_HOOK: Option<fn(&str)> = None;

    pub fn set_debug_hook(f: fn(&str)) {
        unsafe {
            DEBUG_HOOK = Some(f);
        }
    }

    pub(crate) fn debug(msg: &str) {
        unsafe {
            if let Some(f) = DEBUG_HOOK {
                f(msg);
            }
        }
    }
}