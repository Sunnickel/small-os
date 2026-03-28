use crate::interrupts::hardware_interrupt::PIC_1_OFFSET;

#[derive(Debug, Clone, Copy)]
#[repr(u8)]
pub enum InterruptIndex {
    Timer = PIC_1_OFFSET, // raw value is 32
    Keyboard,             // raw value is 33
}

impl InterruptIndex {
    pub fn as_irq(self) -> u8 { self as u8 }
}
