use crate::sync::spinlock::SpinLock;

const BUFFER_LEN: usize = 64;

static SCANCODES: SpinLock<ScancodeBuffer> = SpinLock::new(ScancodeBuffer::new());

struct ScancodeBuffer {
    bytes: [u8; BUFFER_LEN],
    read: usize,
    write: usize,
    len: usize,
}

impl ScancodeBuffer {
    const fn new() -> Self {
        Self {
            bytes: [0; BUFFER_LEN],
            read: 0,
            write: 0,
            len: 0,
        }
    }

    fn push(&mut self, scancode: u8) {
        if self.len == BUFFER_LEN {
            self.read = (self.read + 1) % BUFFER_LEN;
            self.len -= 1;
        }

        self.bytes[self.write] = scancode;
        self.write = (self.write + 1) % BUFFER_LEN;
        self.len += 1;
    }

    #[allow(dead_code)]
    fn pop(&mut self) -> Option<u8> {
        if self.len == 0 {
            return None;
        }

        let scancode = self.bytes[self.read];
        self.read = (self.read + 1) % BUFFER_LEN;
        self.len -= 1;
        Some(scancode)
    }
}

pub fn push_scancode(scancode: u8) {
    SCANCODES.lock().push(scancode);
}

#[allow(dead_code)]
pub fn pop_scancode() -> Option<u8> {
    SCANCODES.lock().pop()
}
