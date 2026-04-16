use core::sync::atomic::{AtomicU64, Ordering};

static NEXT_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DeviceId(u64);

impl DeviceId {
    pub fn allocate() -> Self { Self(NEXT_ID.fetch_add(1, Ordering::Relaxed)) }

    pub const fn as_u64(self) -> u64 { self.0 }
}

impl core::fmt::Display for DeviceId {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "dev:{}", self.0)
    }
}
