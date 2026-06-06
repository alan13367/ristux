use crate::{
    memory::frame_allocator::{FRAME_SIZE, MAX_PHYSICAL_MEMORY},
    sync::spinlock::SpinLock,
};
use alloc::vec::Vec;

const CHUNK_SHIFT: usize = 9;
const CHUNK_SIZE: usize = 1 << CHUNK_SHIFT;
const CHUNK_MASK: usize = CHUNK_SIZE - 1;
const ROOT_ENTRIES: usize = MAX_PHYSICAL_MEMORY / FRAME_SIZE / CHUNK_SIZE;

struct RefcountTable {
    root: Vec<Option<Vec<u16>>>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RefcountError {
    OutOfMemory,
    Overflow,
    UntrackedFrame,
}

impl RefcountTable {
    fn new() -> Result<Self, RefcountError> {
        let mut root = Vec::new();
        root.try_reserve_exact(ROOT_ENTRIES)
            .map_err(|_| RefcountError::OutOfMemory)?;
        root.resize_with(ROOT_ENTRIES, || None);
        Ok(Self { root })
    }

    fn ensure_chunk(&mut self, frame_index: usize) -> Result<&mut [u16], RefcountError> {
        let root_index = frame_index >> CHUNK_SHIFT;
        if root_index >= ROOT_ENTRIES {
            return Err(RefcountError::UntrackedFrame);
        }
        if self.root[root_index].is_none() {
            let mut chunk = Vec::new();
            chunk
                .try_reserve_exact(CHUNK_SIZE)
                .map_err(|_| RefcountError::OutOfMemory)?;
            chunk.resize(CHUNK_SIZE, 1);
            self.root[root_index] = Some(chunk);
        }
        self.root[root_index]
            .as_deref_mut()
            .ok_or(RefcountError::OutOfMemory)
    }

    fn chunk_mut(&mut self, frame_index: usize) -> Option<&mut [u16]> {
        let root_index = frame_index >> CHUNK_SHIFT;
        self.root.get_mut(root_index)?.as_deref_mut()
    }

    fn get_chunk(&self, frame_index: usize) -> Option<&[u16]> {
        let root_index = frame_index >> CHUNK_SHIFT;
        self.root.get(root_index)?.as_deref()
    }
}

static REF_COUNTS: SpinLock<Option<RefcountTable>> = SpinLock::new(None);

pub fn init() {
    let mut guard = REF_COUNTS.lock();
    *guard = Some(RefcountTable::new().expect("failed to allocate sparse frame refcount root"));
    crate::println!(
        "Sparse frame refcount table initialized ({} chunks x {} frames, {} GiB).",
        ROOT_ENTRIES,
        CHUNK_SIZE,
        MAX_PHYSICAL_MEMORY / (1024 * 1024 * 1024)
    );
}

pub fn self_test() {
    let high_frame = MAX_PHYSICAL_MEMORY - FRAME_SIZE;
    if get(high_frame) != 1
        || try_increment(high_frame).is_err()
        || get(high_frame) != 2
        || decrement(high_frame) != 1
    {
        panic!("sparse frame refcount high-frame self-test failed");
    }
    crate::println!("Sparse frame refcount high-frame self-test passed.");
}

fn frame_index(phys: usize) -> usize {
    phys / FRAME_SIZE
}

pub fn try_increment(phys: usize) -> Result<(), RefcountError> {
    let index = frame_index(phys);
    let mut guard = REF_COUNTS.lock();
    let Some(table) = guard.as_mut() else {
        return Ok(());
    };
    let chunk = table.ensure_chunk(index)?;
    let slot = index & CHUNK_MASK;
    let Some(next) = chunk[slot].checked_add(1) else {
        return Err(RefcountError::Overflow);
    };
    chunk[slot] = next;
    Ok(())
}

pub fn decrement(phys: usize) -> u16 {
    let index = frame_index(phys);
    let mut guard = REF_COUNTS.lock();
    let Some(table) = guard.as_mut() else {
        return 0;
    };
    if let Some(chunk) = table.chunk_mut(index) {
        let slot = index & CHUNK_MASK;
        if chunk[slot] > 0 {
            chunk[slot] -= 1;
        }
        return chunk[slot];
    }
    // Missing chunks represent an implicit refcount of 1.
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
    if let Some(chunk) = table.chunk_mut(index) {
        chunk[index & CHUNK_MASK] = val;
    } else if val != 1 {
        if let Ok(chunk) = table.ensure_chunk(index) {
            chunk[index & CHUNK_MASK] = val;
        }
    }
}
