use core::{fmt, ptr};

use super::frame_allocator::{self, FRAME_SIZE};

const ENTRY_COUNT: usize = 512;
const PRESENT: u64 = 1 << 0;
const WRITABLE: u64 = 1 << 1;
const HUGE_PAGE: u64 = 1 << 7;
const ADDR_MASK: u64 = 0x000f_ffff_ffff_f000;

#[repr(C, align(4096))]
pub struct PageTable {
    entries: [u64; ENTRY_COUNT],
}

unsafe extern "C" {
    static mut boot_p4_table: PageTable;
}

#[derive(Clone, Copy)]
pub struct PageFlags(u64);

impl PageFlags {
    pub const WRITABLE: Self = Self(PRESENT | WRITABLE);
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum PagingError {
    OutOfFrames,
    AlreadyMapped,
    NotMapped,
    HugePage,
}

impl fmt::Display for PagingError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::OutOfFrames => f.write_str("out of physical frames"),
            Self::AlreadyMapped => f.write_str("page is already mapped"),
            Self::NotMapped => f.write_str("page is not mapped"),
            Self::HugePage => f.write_str("encountered an unsupported huge page"),
        }
    }
}

pub fn init() {
    crate::println!(
        "Active level-4 page table: {:#x}",
        root_table() as usize
    );
    self_test();
}

pub unsafe fn map_page(virt: usize, phys: usize, flags: PageFlags) -> Result<(), PagingError> {
    let p4 = root_table();
    let p3 = unsafe { next_table_or_create(&mut (*p4).entries[p4_index(virt)])? };
    let p2 = unsafe { next_table_or_create(&mut (*p3).entries[p3_index(virt)])? };
    let p1 = unsafe { next_table_or_create(&mut (*p2).entries[p2_index(virt)])? };
    let entry = unsafe { &mut (*p1).entries[p1_index(virt)] };

    if *entry & PRESENT != 0 {
        return Err(PagingError::AlreadyMapped);
    }

    *entry = phys as u64 | flags.0;
    unsafe {
        flush(virt);
    }
    Ok(())
}

pub unsafe fn unmap_page(virt: usize) -> Result<usize, PagingError> {
    let p4 = root_table();
    let p3 = unsafe { next_table(&mut (*p4).entries[p4_index(virt)])? };
    let p2 = unsafe { next_table(&mut (*p3).entries[p3_index(virt)])? };
    let p1 = unsafe { next_table(&mut (*p2).entries[p2_index(virt)])? };
    let entry = unsafe { &mut (*p1).entries[p1_index(virt)] };

    if *entry & PRESENT == 0 {
        return Err(PagingError::NotMapped);
    }

    let phys = (*entry & ADDR_MASK) as usize;
    *entry = 0;
    unsafe {
        flush(virt);
    }
    Ok(phys)
}

fn self_test() {
    const TEST_VIRT: usize = 0x4000_0000;
    const TEST_VALUE: u64 = 0xfeed_face_cafe_beef;

    let frame = frame_allocator::allocate_frame().expect("paging self-test frame allocation failed");

    unsafe {
        map_page(TEST_VIRT, frame.start, PageFlags::WRITABLE)
            .unwrap_or_else(|err| panic!("paging map self-test failed: {}", err));

        let ptr = TEST_VIRT as *mut u64;
        ptr::write_volatile(ptr, TEST_VALUE);
        let read_back = ptr::read_volatile(ptr);
        if read_back != TEST_VALUE {
            panic!("paging self-test read back {:#x}", read_back);
        }

        let unmapped = unmap_page(TEST_VIRT)
            .unwrap_or_else(|err| panic!("paging unmap self-test failed: {}", err));
        if unmapped != frame.start {
            panic!("paging self-test unmapped unexpected frame {:#x}", unmapped);
        }
    }

    frame_allocator::free_frame(frame);
    crate::println!("Paging map/unmap self-test passed.");
}

fn root_table() -> *mut PageTable {
    ptr::addr_of_mut!(boot_p4_table)
}

unsafe fn next_table_or_create(entry: &mut u64) -> Result<*mut PageTable, PagingError> {
    if *entry & HUGE_PAGE != 0 {
        return Err(PagingError::HugePage);
    }

    if *entry & PRESENT == 0 {
        let frame = frame_allocator::allocate_frame().ok_or(PagingError::OutOfFrames)?;
        unsafe {
            ptr::write_bytes(frame.start as *mut u8, 0, FRAME_SIZE);
        }
        *entry = frame.start as u64 | PRESENT | WRITABLE;
    }

    Ok((*entry & ADDR_MASK) as *mut PageTable)
}

unsafe fn next_table(entry: &mut u64) -> Result<*mut PageTable, PagingError> {
    if *entry & PRESENT == 0 {
        return Err(PagingError::NotMapped);
    }

    if *entry & HUGE_PAGE != 0 {
        return Err(PagingError::HugePage);
    }

    Ok((*entry & ADDR_MASK) as *mut PageTable)
}

unsafe fn flush(virt: usize) {
    unsafe {
        core::arch::asm!("invlpg [{}]", in(reg) virt, options(nostack, preserves_flags));
    }
}

const fn p4_index(addr: usize) -> usize {
    (addr >> 39) & 0x1ff
}

const fn p3_index(addr: usize) -> usize {
    (addr >> 30) & 0x1ff
}

const fn p2_index(addr: usize) -> usize {
    (addr >> 21) & 0x1ff
}

const fn p1_index(addr: usize) -> usize {
    (addr >> 12) & 0x1ff
}

