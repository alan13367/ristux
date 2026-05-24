use alloc::collections::VecDeque;

pub struct Pipe {
    buffer: VecDeque<u8>,
    capacity: usize,
}

impl Pipe {
    pub fn new(capacity: usize) -> Self {
        Self {
            buffer: VecDeque::new(),
            capacity,
        }
    }

    pub fn write(&mut self, bytes: &[u8]) -> usize {
        let mut written = 0;
        for byte in bytes {
            if self.buffer.len() == self.capacity {
                break;
            }
            self.buffer.push_back(*byte);
            written += 1;
        }
        written
    }

    pub fn read(&mut self, output: &mut [u8]) -> usize {
        let mut read = 0;
        for byte in output {
            let Some(value) = self.buffer.pop_front() else {
                break;
            };
            *byte = value;
            read += 1;
        }
        read
    }
}

pub fn self_test() {
    let mut pipe = Pipe::new(64);
    let written = pipe.write(b"hello pipe");
    let mut output = [0; 10];
    let read = pipe.read(&mut output);
    if written != 10 || read != 10 || &output != b"hello pipe" {
        panic!("pipe self-test failed");
    }
    crate::println!("Pipe self-test passed.");
}

