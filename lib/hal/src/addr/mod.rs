use core::ops::Shr;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct PhysAddr(pub u64);

impl PhysAddr {
    pub const fn new(addr: u64) -> Self { Self(addr) }
    pub const fn as_u64(self) -> u64 { self.0 }
    pub const fn low_u32(self) -> u32 { self.0 as u32 }
    pub const fn high_u32(self) -> u32 { (self.0 >> 32) as u32 }
    pub fn as_virt(self, phys_offset: u64) -> *mut u8 { (self.0 + phys_offset) as *mut u8 }
}

impl Shr<i32> for PhysAddr {
    type Output = u32;
    fn shr(self, rhs: i32) -> Self::Output {
        (self.0 >> rhs) as u32
    }
}