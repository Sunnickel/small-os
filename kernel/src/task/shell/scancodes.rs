use core::pin::Pin;
use core::task::{Context, Poll};

use crossbeam_queue::ArrayQueue;
use futures_util::stream::Stream;

use crate::flags::{KEYBOARD_WAKER, SCANCODE_QUEUE};

pub struct ScancodeStream;

impl ScancodeStream {
    pub fn new() -> Self {
        SCANCODE_QUEUE
            .try_init_once(|| ArrayQueue::new(256)) // 🔥 bigger buffer
            .expect("SCANCODE_QUEUE already initialized");
        ScancodeStream
    }
}

impl Stream for ScancodeStream {
    type Item = u8;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<u8>> {
        let queue = SCANCODE_QUEUE.get().expect("not initialized");

        // fast path
        if let Some(code) = queue.pop() {
            return Poll::Ready(Some(code));
        }

        KEYBOARD_WAKER.register(cx.waker());

        match queue.pop() {
            Some(code) => {
                KEYBOARD_WAKER.take();
                Poll::Ready(Some(code))
            }
            None => Poll::Pending,
        }
    }
}
