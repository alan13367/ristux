use alloc::vec::Vec;
use crate::sync::spinlock::SpinLock;

static REF_COUNTS: SpinLock<Option<Vec<u8>>> = SpinLock::new(None);

pub fn init(max_frame: usize) {
    let mut guard = REF_COUNTS.lock();
    let mut vec = Vec::with_capacity(max_frame);
    vec.resize(max_frame, 1); // default refcount is 1 for allocated frames
    *guard = Some(vec);
}

pub fn increment(phys: usize) {
    let index = phys / 4096;
    let mut guard = REF_COUNTS.lock();
    if let Some(ref mut vec) = *guard {
        if index < vec.len() {
            vec[index] = vec[index].saturating_add(1);
        }
    }
}

pub fn decrement(phys: usize) -> u8 {
    let index = phys / 4096;
    let mut guard = REF_COUNTS.lock();
    if let Some(ref mut vec) = *guard {
        if index < vec.len() {
            if vec[index] > 0 {
                vec[index] -= 1;
            }
            return vec[index];
        }
    }
    0
}

pub fn get(phys: usize) -> u8 {
    let index = phys / 4096;
    let guard = REF_COUNTS.lock();
    if let Some(ref vec) = *guard {
        if index < vec.len() {
            return vec[index];
        }
    }
    1
}

pub fn set(phys: usize, val: u8) {
    let index = phys / 4096;
    let mut guard = REF_COUNTS.lock();
    if let Some(ref mut vec) = *guard {
        if index < vec.len() {
            vec[index] = val;
        }
    }
}
