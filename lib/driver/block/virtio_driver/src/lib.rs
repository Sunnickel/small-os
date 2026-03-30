#![no_std]

extern crate alloc;
mod constants;
mod device;
mod queue;

pub use device::VirtioBlkPci as VirtioBlkDevice;

pub enum VirtioMode {
    Modern,
    Legacy,
}
