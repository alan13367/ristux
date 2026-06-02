use crate::sync::spinlock::SpinLock;
use alloc::boxed::Box;

const CHUNK_SHIFT: usize = 9;
const CHUNK_SIZE: usize = 1 << CHUNK_SHIFT;
const CHUNK_MASK: usize = CHUNK_SIZE - 1;
const ROOT_ENTRIES: usize = 512;

struct RefcountTable {
    root: [Option<Box<[u16; CHUNK_SIZE]>>; ROOT_ENTRIES],
}

impl RefcountTable {
    const fn new() -> Self {
        Self {
            root: [const { None }; ROOT_ENTRIES],
        }
    }

    fn chunk_for(&mut self, frame_index: usize) -> Option<&mut [u16; CHUNK_SIZE]> {
        let root_index = frame_index >> CHUNK_SHIFT;
        if root_index >= ROOT_ENTRIES {
            return None;
        }
        if self.root[root_index].is_none() {
            self.root[root_index] = Some(Box::new([1; CHUNK_SIZE]));
        }
        self.root[root_index].as_deref_mut()
    }

    fn get_chunk(&self, frame_index: usize) -> Option<&[u16; CHUNK_SIZE]> {
        let root_index = frame_index >> CHUNK_SHIFT;
        self.root.get(root_index)?.as_deref()
    }
}

static REF_COUNTS: SpinLock<Option<RefcountTable>> = SpinLock::new(None);

pub fn init() {
    let mut guard = REF_COUNTS.lock();
    *guard = Some(RefcountTable::new());
    crate::println!(
        "Sparse frame refcount table initialized ({} chunks x {} frames).",
        ROOT_ENTRIES,
        CHUNK_SIZE
    );
}

fn frame_index(phys: usize) -> usize {
    phys / 4096
}

pub fn try_increment(phys: usize) -> bool {
    let index = frame_index(phys);
    let mut guard = REF_COUNTS.lock();
    let Some(table) = guard.as_mut() else {
        return true;
    };
    if let Some(chunk) = table.chunk_for(index) {
        let slot = index & CHUNK_MASK;
        let Some(next) = chunk[slot].checked_add(1) else {
            return false;
        };
        chunk[slot] = next;
    }
    true
}

pub fn decrement(phys: usize) -> u16 {
    let index = frame_index(phys);
    let mut guard = REF_COUNTS.lock();
    let Some(table) = guard.as_mut() else {
        return 0;
    };
    if let Some(chunk) = table.chunk_for(index) {
        let slot = index & CHUNK_MASK;
        if chunk[slot] > 0 {
            chunk[slot] -= 1;
        }
        return chunk[slot];
    }
    0
}

pub fn get(phys: usize) -> u16 {
    let index = frame_index(phys);
    let guard = REF_COUNTS.lock();
    let Some(table) = guard.as_ref() else {
        return 1;
    };
    if let Some(chunk) = table.get_chunk(index) {
        return chunk[index & CHUNK_MASK];
    }
    1
}

pub fn set(phys: usize, val: u16) {
    let index = frame_index(phys);
    let mut guard = REF_COUNTS.lock();
    let Some(table) = guard.as_mut() else {
        return;
    };
    if let Some(chunk) = table.chunk_for(index) {
        chunk[index & CHUNK_MASK] = val;
    }
}
