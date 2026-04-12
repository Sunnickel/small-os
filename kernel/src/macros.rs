use core::{fmt, fmt::Write};

use heapless::String;
use lazy_static::lazy_static;
use spin::Mutex;
use uart_16550::{Config, Uart16550, backend::PioBackend};

use crate::INTERRUPT_LOG_BUFFER;

lazy_static! {
    pub static ref SERIAL1: Mutex<Uart16550<PioBackend>> = {
        let mut uart = unsafe { Uart16550::new_port(0x3F8).expect("should be valid port") };
        uart.init(Config::default()).expect("should init device successfully");
        uart.test_loopback().expect("should have working loopback mode");
        uart.check_connected().expect("should have physically connected receiver");

        Mutex::new(uart)
    };
}

#[macro_export]
macro_rules! print {
    ($($arg:tt)*) => {
        $crate::_print(format_args!($($arg)*))
    };
}

#[macro_export]
macro_rules! println {
    () => ($crate::print!("\n"));
    ($($arg:tt)*) => ($crate::print!("{}\n", format_args!($($arg)*)));
}

#[macro_export]
macro_rules! serial_print {
    ($($arg:tt)*) => ($crate::_print_serial(format_args!($($arg)*)));
}

#[macro_export]
macro_rules! serial_println {
    () => ($crate::serial_print!("\n"));
    ($fmt:expr) => ($crate::serial_print!(concat!($fmt, "\n")));
    ($fmt:expr, $($arg:tt)*) => ($crate::serial_print!(concat!($fmt, "\n"), $($arg)*));
}

// In macros.rs - use try_lock to avoid deadlock
#[doc(hidden)]
pub fn _print(args: fmt::Arguments) {
    use core::fmt::Write;

    use x86_64::instructions::interrupts;

    if !interrupts::are_enabled() {
        buffer_from_interrupt(args);
        return;
    }

    interrupts::without_interrupts(|| {
        if let Some(mut writer) = crate::screen::SCREEN_WRITER.try_lock() {
            if let Some(w) = writer.as_mut() {
                let _ = w.write_fmt(args);
            }
        }
    });
}

#[doc(hidden)]
pub fn _print_serial(args: fmt::Arguments) {
    use core::fmt::Write;

    use x86_64::instructions::interrupts;

    interrupts::without_interrupts(|| {
        let mut buf: String<256> = String::new();
        if write!(&mut buf, "[kernel] {}", args).is_ok() {
            SERIAL1.lock().send_bytes_exact(buf.as_bytes());
        }
    });
}

pub(crate) fn buffer_from_interrupt(args: fmt::Arguments) {
    let mut buf = INTERRUPT_LOG_BUFFER.lock();
    let _ = buf.write_fmt(args);
}

#[doc(hidden)]
pub fn _print_raw(s: &str) {
    use core::fmt::Write;

    use x86_64::instructions::interrupts;

    interrupts::without_interrupts(|| {
        if let Some(writer) = crate::screen::SCREEN_WRITER.lock().as_mut() {
            let _ = writer.write_str(s);
        }
    });
}
