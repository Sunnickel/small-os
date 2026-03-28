use crate::flags::{TIMER_TICKS, TIMER_WAKER};
use core::pin::Pin;
use core::sync::atomic::Ordering;
use core::task::{Context, Poll};

pub struct Sleep {
    target: usize,
}

impl Sleep {
    pub fn new(ticks: usize) -> Self {
        let now = TIMER_TICKS.load(Ordering::Relaxed);
        Sleep {
            target: now + ticks,
        }
    }
}

impl Future for Sleep {
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        if TIMER_TICKS.load(Ordering::Relaxed) >= self.target {
            Poll::Ready(())
        } else {
            TIMER_WAKER.register(cx.waker());
            Poll::Pending
        }
    }
}
