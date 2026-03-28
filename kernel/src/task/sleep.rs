use core::{
    pin::Pin,
    sync::atomic::Ordering,
    task::{Context, Poll},
};

use crate::flags::{TIMER_TICKS, TIMER_WAKER};

pub struct Sleep {
    target: usize,
}

impl Sleep {
    pub fn new(ticks: usize) -> Self {
        let now = TIMER_TICKS.load(Ordering::Relaxed);
        Sleep { target: now + ticks }
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
