use core::sync::atomic::{AtomicBool, AtomicUsize};

use conquer_once::spin::OnceCell;
use crossbeam_queue::ArrayQueue;
use device::DeviceRegistry;
use futures_util::task::AtomicWaker;
use heapless::String;
use spin::{Mutex, Once};
use spinning_top::Spinlock;
use driver::DriverRegistry;
use crate::{
    memory::{alloc::Locked, types::fixed_size_block::FixedSizeBlockAllocator},
    screen::Writer,
};

// ==========================
// 🧠 MEMORY
// ==========================
//

#[global_allocator]
pub static GLOBAL_ALLOCATOR: Locked<FixedSizeBlockAllocator> =
    Locked::new(FixedSizeBlockAllocator::new());

// ==========================
// ⏱️ SCHEDULER / TIME
// ==========================
//

/// Global timer tick counter (incremented by timer interrupt)
pub static TIMER_TICKS: AtomicUsize = AtomicUsize::new(0);

/// Signals executor to reschedule tasks
pub static SHOULD_YIELD_FLAG: AtomicBool = AtomicBool::new(false);

// ==========================
// ⌨️ INPUT SYSTEM
// ==========================
//

/// Lock-free queue for incoming scancodoes scancodes
pub static SCANCODE_QUEUE: OnceCell<ArrayQueue<u8>> = OnceCell::uninit();

/// Waker for tasks waiting on scancodoes input
pub static KEYBOARD_WAKER: AtomicWaker = AtomicWaker::new();

/// Debug counter for scancodoes events
pub static KEYBOARD_EVENT_COUNT: AtomicUsize = AtomicUsize::new(0);

pub static TIMER_WAKER: AtomicWaker = AtomicWaker::new();

// ==========================
// 🖥️ OUTPUT / DEBUG
// ==========================
//

/// Global screen writer (initialized once at runtime)
pub static SCREEN_WRITER: Spinlock<Option<Writer>> = Spinlock::new(None);

/// Debug buffer for interrupt-safe logging
pub static INTERRUPT_LOG_BUFFER: Mutex<String<1024>> = Mutex::new(String::new());

// ==========================
// Driver
// ==========================
//
pub static DEVICE_REGISTRY: Once<DeviceRegistry> = Once::new();
pub static DRIVER_REGISTRY: Once<DriverRegistry> = Once::new();
