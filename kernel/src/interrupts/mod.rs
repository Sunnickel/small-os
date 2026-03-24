pub mod gdt;
pub mod hardware_interrupt;
pub mod interrupt_index;

use core::sync::atomic::Ordering;
use heapless::Vec;
use crate::interrupts::hardware_interrupt::PICS;
use crate::interrupts::interrupt_index::InterruptIndex;
use crate::{hlt_loop, println, serial_println, TIMER_TICKS};
use lazy_static::lazy_static;
use spin::Mutex;
use x86_64::instructions::port::Port;
use x86_64::structures::idt::{InterruptDescriptorTable, InterruptStackFrame, PageFaultErrorCode};

static KEYBOARD_BUFFER: Mutex<Vec<u8, 64>> = Mutex::new(Vec::new());

lazy_static! {
    static ref IDT: InterruptDescriptorTable = {
        let mut idt = InterruptDescriptorTable::default();
        idt.breakpoint.set_handler_fn(breakpoint_handler);
        unsafe {
            idt.double_fault
                .set_handler_fn(double_fault_handler)
                .set_stack_index(gdt::DOUBLE_FAULT_IST_INDEX as u16);
        }
        idt[InterruptIndex::Timer.as_irq()].set_handler_fn(timer_interrupt_handler);
        idt[InterruptIndex::Keyboard.as_irq()].set_handler_fn(keyboard_interrupt_handler);
        idt.page_fault.set_handler_fn(page_fault_handler);

        idt
    };
}

pub fn init_idt() {
    IDT.load();
}

extern "x86-interrupt" fn page_fault_handler(
    stack_frame: InterruptStackFrame,
    error_code: PageFaultErrorCode,
) {
    use x86_64::registers::control::Cr2;

    serial_println!("EXCEPTION: PAGE FAULT");
    serial_println!("Accessed Address: {:?}", Cr2::read());
    serial_println!("Error Code: {:?}", error_code);
    serial_println!("{:#?}", stack_frame);
    hlt_loop();
}

extern "x86-interrupt" fn breakpoint_handler(stack_frame: InterruptStackFrame) {
    println!("EXCEPTION: BREAKPOINT\n{:#?}", stack_frame);
}

extern "x86-interrupt" fn double_fault_handler(
    _stack_frame: InterruptStackFrame,
    _error_code: u64,
) -> ! {
    unsafe {
        let port = 0x3F8 as *mut u8;
        for b in b"DOUBLE_FAULT\r\n" {
            port.write_volatile(*b);
        }
    }
    loop {
        x86_64::instructions::hlt();
    }
}

extern "x86-interrupt" fn timer_interrupt_handler(
    _stack_frame: InterruptStackFrame)
{
    TIMER_TICKS.fetch_add(1, Ordering::Relaxed);
    unsafe {
        if let Some(mut pics) = PICS.try_lock() {
            pics.notify_end_of_interrupt(InterruptIndex::Timer.as_irq());
        }
    }
}

extern "x86-interrupt" fn keyboard_interrupt_handler(
    _stack_frame: InterruptStackFrame)
{
    let mut port = Port::new(0x60);
    let scancode: u8 = unsafe { port.read() };

    if let Some(mut buf) = KEYBOARD_BUFFER.try_lock() {
        let _ = buf.push(scancode);
    }

    unsafe {
        PICS.lock()
            .notify_end_of_interrupt(InterruptIndex::Keyboard.as_irq());
    }
}

pub fn get_key() -> Option<u8> {
    KEYBOARD_BUFFER.lock().pop()
}
