pub mod alloc;

pub struct ListNode {
    size: usize,
    next: Option<&'static mut ListNode>,
}

impl ListNode {
    const fn new(size: usize) -> Self {
        ListNode { size, next: None }
    }

    fn start_addr(&self) -> usize {
        self as *const Self as usize
    }

    pub(crate) fn end_addr(&self) -> usize {
        self.start_addr() + self.size
    }
}
