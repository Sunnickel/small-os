pub struct History {
    entries: heapless::Vec<heapless::String<128>, 32>,
    index: usize,
}

impl History {
    pub fn new() -> Self {
        Self {
            entries: heapless::Vec::new(),
            index: 0,
        }
    }

    pub fn push(&mut self, line: &str) {
        if let Ok(s) = heapless::String::try_from(line) {
            if self.entries.push(s).is_ok() {
                self.index = self.entries.len();
            }
        }
    }

    pub fn up(&mut self) -> Option<&str> {
        if self.index > 0 {
            self.index -= 1;
            Some(&self.entries[self.index])
        } else {
            None
        }
    }

    pub fn down(&mut self) -> Option<&str> {
        if self.index + 1 < self.entries.len() {
            self.index += 1;
            Some(&self.entries[self.index])
        } else {
            None
        }
    }
}
