use crate::{multiboot::BootInfo, sync::spinlock::SpinLock};

pub const FRAME_SIZE: usize = 4096;
const MAX_PHYSICAL_MEMORY: usize = 16 * 1024 * 1024 * 1024;
const MAX_FRAMES: usize = MAX_PHYSICAL_MEMORY / FRAME_SIZE;
const BITMAP_WORDS: usize = MAX_FRAMES / 64;

unsafe extern "C" {
    static __kernel_start: u8;
    static __kernel_end: u8;
}

static FRAME_ALLOCATOR: SpinLock<FrameAllocator> = SpinLock::new(FrameAllocator::new());

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Frame {
    pub start: usize,
}

#[derive(Clone, Copy)]
pub struct Stats {
    pub total_frames: usize,
    pub free_frames: usize,
}

struct FrameAllocator {
    bitmap: [u64; BITMAP_WORDS],
    max_frame: usize,
    total_frames: usize,
    free_frames: usize,
    initialized: bool,
}

impl FrameAllocator {
    const fn new() -> Self {
        Self {
            bitmap: [u64::MAX; BITMAP_WORDS],
            max_frame: 0,
            total_frames: 0,
            free_frames: 0,
            initialized: false,
        }
    }

    fn init(&mut self, boot_info: &BootInfo) {
        self.bitmap.fill(u64::MAX);
        self.max_frame = 0;
        self.total_frames = 0;
        self.free_frames = 0;

        if let Some(memory_map) = boot_info.memory_map() {
            for entry in memory_map {
                let start = entry.base_addr as usize;
                let end = start.saturating_add(entry.length as usize);

                if entry.is_available() {
                    self.max_frame = self.max_frame.max(frame_index(align_down(end, FRAME_SIZE)));
                    self.mark_range(start, end, false);
                }
            }
        }

        let mut kernel_phys_end = core::ptr::addr_of!(__kernel_end) as usize;
        if kernel_phys_end >= 0xffffffff80000000 {
            kernel_phys_end -= 0xffffffff80000000;
        }

        let (boot_start, boot_end) = boot_info.range();

        self.mark_range(0, 0x100000, true);
        self.mark_range(0x100000, kernel_phys_end, true);
        self.mark_range(boot_start, boot_end, true);

        for module in boot_info.modules() {
            self.mark_range(module.start as usize, module.end as usize, true);
        }

        if let Some(framebuffer) = boot_info.framebuffer() {
            let size = framebuffer.pitch as usize * framebuffer.height as usize;
            self.mark_range(
                framebuffer.addr as usize,
                framebuffer.addr as usize + size,
                true,
            );
        }

        self.initialized = true;
    }

    fn allocate(&mut self) -> Option<Frame> {
        if !self.initialized {
            return None;
        }

        let max_word = words_for_frames(self.max_frame).min(BITMAP_WORDS);
        for word_index in 0..max_word {
            if self.bitmap[word_index] == u64::MAX {
                continue;
            }

            let bit = (!self.bitmap[word_index]).trailing_zeros() as usize;
            let index = word_index * 64 + bit;
            if index >= self.max_frame {
                return None;
            }

            self.set_used(index);
            return Some(Frame {
                start: index * FRAME_SIZE,
            });
        }

        None
    }

    fn free(&mut self, frame: Frame) {
        let index = frame_index(frame.start);
        if index < self.max_frame {
            self.set_free(index);
        }
    }

    fn stats(&self) -> Stats {
        Stats {
            total_frames: self.total_frames,
            free_frames: self.free_frames,
        }
    }

    fn mark_range(&mut self, start: usize, end: usize, used: bool) {
        let start = align_down(start, FRAME_SIZE);
        let end = align_up(end, FRAME_SIZE);

        for addr in (start..end).step_by(FRAME_SIZE) {
            let index = frame_index(addr);
            if index >= MAX_FRAMES {
                break;
            }

            if used {
                self.set_used(index);
            } else {
                self.set_available(index);
            }
        }
    }

    fn set_used(&mut self, index: usize) {
        let word = index / 64;
        let bit = index % 64;
        let mask = 1u64 << bit;
        if word < BITMAP_WORDS && self.bitmap[word] & mask == 0 {
            self.bitmap[word] |= mask;
            self.free_frames = self.free_frames.saturating_sub(1);
        }
    }

    fn set_available(&mut self, index: usize) {
        let word = index / 64;
        let bit = index % 64;
        let mask = 1u64 << bit;
        if word < BITMAP_WORDS && self.bitmap[word] & mask != 0 {
            self.bitmap[word] &= !mask;
            self.total_frames += 1;
            self.free_frames += 1;
        }
    }

    fn set_free(&mut self, index: usize) {
        let word = index / 64;
        let bit = index % 64;
        let mask = 1u64 << bit;
        if word < BITMAP_WORDS && self.bitmap[word] & mask != 0 {
            self.bitmap[word] &= !mask;
            self.free_frames += 1;
        }
    }
}

pub fn init(boot_info: &BootInfo) {
    let mut allocator = FRAME_ALLOCATOR.lock();
    allocator.init(boot_info);
    let stats = allocator.stats();

    crate::println!(
        "Physical frame allocator: {} total frames, {} free frames",
        stats.total_frames,
        stats.free_frames
    );
}

pub fn allocate_frame() -> Option<Frame> {
    let frame = FRAME_ALLOCATOR.lock().allocate();
    if let Some(ref f) = frame {
        super::refcount::set(f.start, 1);
    }
    frame
}

pub fn free_frame(frame: Frame) {
    FRAME_ALLOCATOR.lock().free(frame);
}

pub fn stats() -> Stats {
    FRAME_ALLOCATOR.lock().stats()
}

pub fn max_frame() -> usize {
    FRAME_ALLOCATOR.lock().max_frame
}

#[allow(dead_code)]
pub fn reserve_range(start: usize, end: usize) {
    FRAME_ALLOCATOR.lock().mark_range(start, end, true);
}

pub fn self_test() {
    let before = stats();
    let frame = allocate_frame().expect("physical frame allocation failed");
    crate::println!("Allocated test frame at {:#x}", frame.start);
    free_frame(frame);
    let after = stats();

    if before.free_frames != after.free_frames {
        panic!("frame allocator free count changed during self-test");
    }

    crate::println!("Physical frame allocator self-test passed.");
}

const fn frame_index(addr: usize) -> usize {
    addr / FRAME_SIZE
}

const fn words_for_frames(frames: usize) -> usize {
    frames.div_ceil(64)
}

const fn align_down(value: usize, align: usize) -> usize {
    value & !(align - 1)
}

const fn align_up(value: usize, align: usize) -> usize {
    (value + align - 1) & !(align - 1)
}
