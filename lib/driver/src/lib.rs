#![no_std]

pub mod fs {
    pub use driver_fs::*;
}

pub mod pci {
    pub use driver_core::pci::*;
}

pub mod dma {
    pub use hal::dma::DmaAllocator;
}

pub mod core {
    pub use driver_core::{acpi_wrapper as acpi, set_debug_hook};
}
