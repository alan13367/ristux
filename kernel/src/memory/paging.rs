use core::{fmt, ptr};

use super::frame_allocator::{self, Frame, FRAME_SIZE};

const ENTRY_COUNT: usize = 512;
const PRESENT: u64 = 1 << 0;
const WRITABLE: u64 = 1 << 1;
const USER: u64 = 1 << 2;
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
    pub const USER_WRITABLE: Self = Self(PRESENT | WRITABLE | USER);
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
        boot_root_table() as usize
    );
    self_test();
}

pub fn boot_root_table() -> *mut PageTable {
    ptr::addr_of_mut!(boot_p4_table)
}

pub unsafe fn switch_cr3(p4_phys: usize) {
    unsafe {
        core::arch::asm!("mov cr3, {}", in(reg) p4_phys, options(nostack, preserves_flags));
    }
}

pub unsafe fn map_page(virt: usize, phys: usize, flags: PageFlags) -> Result<(), PagingError> {
    unsafe { map_page_at(boot_root_table(), virt, phys, flags) }
}

pub unsafe fn map_page_at(
    p4: *mut PageTable,
    virt: usize,
    phys: usize,
    flags: PageFlags,
) -> Result<(), PagingError> {
    let p3 = unsafe { next_table_or_create(p4, &mut (*p4).entries[p4_index(virt)], flags)? };
    let p2 = unsafe { next_table_or_create(p4, &mut (*p3).entries[p3_index(virt)], flags)? };
    let p2_entry = unsafe { &mut (*p2).entries[p2_index(virt)] };
    if *p2_entry & HUGE_PAGE != 0 {
        unsafe {
            split_p2_huge_page(p2_entry, flags)?;
        }
    }
    let p1 = unsafe { next_table_or_create(p4, p2_entry, flags)? };
    let entry = unsafe { &mut (*p1).entries[p1_index(virt)] };

    if *entry & PRESENT != 0 {
        if (*entry & ADDR_MASK) as usize == phys {
            return Err(PagingError::AlreadyMapped);
        }
        *entry = phys as u64 | flags.0;
        unsafe {
            flush(virt);
        }
        return Ok(());
    }

    *entry = phys as u64 | flags.0;
    unsafe {
        flush(virt);
    }
    Ok(())
}

pub unsafe fn unmap_page(virt: usize) -> Result<usize, PagingError> {
    unsafe { unmap_page_at(boot_root_table(), virt) }
}

pub unsafe fn unmap_page_at(p4: *mut PageTable, virt: usize) -> Result<usize, PagingError> {
    let p3 = unsafe { next_table(p4, &mut (*p4).entries[p4_index(virt)])? };
    let p2 = unsafe { next_table(p4, &mut (*p3).entries[p3_index(virt)])? };
    let p1 = unsafe { next_table(p4, &mut (*p2).entries[p2_index(virt)])? };
    let entry = unsafe { &mut (*p1).entries[p1_index(virt)] };

    if *entry & PRESENT == 0 {
        return Err(PagingError::NotMapped);
    }

    if *entry & HUGE_PAGE != 0 {
        return Err(PagingError::HugePage);
    }

    let phys = (*entry & ADDR_MASK) as usize;
    *entry = 0;
    unsafe {
        flush(virt);
    }
    Ok(phys)
}

pub unsafe fn clone_p4_table(source: *mut PageTable) -> Result<Frame, PagingError> { unsafe {
    deep_clone_p4_table(source)
}}

unsafe fn deep_clone_p4_table(source: *mut PageTable) -> Result<Frame, PagingError> {
    let frame = frame_allocator::allocate_frame().ok_or(PagingError::OutOfFrames)?;
    let dest = frame.start as *mut PageTable;
    unsafe {
        ptr::write_bytes(dest as *mut u8, 0, FRAME_SIZE);
        for index in 0..512 {
            clone_table_entry(&(*source).entries[index], &mut (*dest).entries[index], 3)?;
        }
    }
    Ok(frame)
}

unsafe fn clone_table_entry(src: &u64, dst: &mut u64, level: u32) -> Result<(), PagingError> { unsafe {
    if *src & PRESENT == 0 {
        return Ok(());
    }

    if level == 0 || *src & HUGE_PAGE != 0 {
        *dst = *src;
        return Ok(());
    }

    let src_table = (*src & ADDR_MASK) as *mut PageTable;
    let new_frame = frame_allocator::allocate_frame().ok_or(PagingError::OutOfFrames)?;
    let dest_table = new_frame.start as *mut PageTable;
    ptr::write_bytes(dest_table as *mut u8, 0, FRAME_SIZE);

    for index in 0..512 {
        clone_table_entry(&(*src_table).entries[index], &mut (*dest_table).entries[index], level - 1)?;
    }

    *dst = new_frame.start as u64 | (*src & !ADDR_MASK);
    Ok(())
}}

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
    let user_frame =
        frame_allocator::allocate_frame().expect("user paging self-test frame allocation failed");
    unsafe {
        map_page(0x4010_0000, user_frame.start, PageFlags::USER_WRITABLE)
            .unwrap_or_else(|err| panic!("user paging map self-test failed: {}", err));
        let unmapped = unmap_page(0x4010_0000)
            .unwrap_or_else(|err| panic!("user paging unmap self-test failed: {}", err));
        if unmapped != user_frame.start {
            panic!("user paging self-test unmapped unexpected frame {:#x}", unmapped);
        }
    }
    frame_allocator::free_frame(user_frame);
    crate::println!("Paging map/unmap self-test passed.");
}

unsafe fn split_p2_huge_page(entry: &mut u64, flags: PageFlags) -> Result<(), PagingError> { unsafe {
    if *entry & HUGE_PAGE == 0 {
        return Ok(());
    }

    let huge_phys = (*entry & ADDR_MASK) as usize;
    let leaf_flags = (*entry & !ADDR_MASK) & !HUGE_PAGE;

    let frame = frame_allocator::allocate_frame().ok_or(PagingError::OutOfFrames)?;
    let p1 = frame.start as *mut PageTable;

    for i in 0..512 {
        let page_phys = huge_phys + i * FRAME_SIZE;
        (*p1).entries[i] = page_phys as u64 | leaf_flags;
    }

    *entry = frame.start as u64 | PRESENT | WRITABLE | (flags.0 & USER);
    Ok(())
}}

unsafe fn next_table_or_create(
    _p4: *mut PageTable,
    entry: &mut u64,
    flags: PageFlags,
) -> Result<*mut PageTable, PagingError> {
    if *entry & HUGE_PAGE != 0 {
        return Err(PagingError::HugePage);
    }

    if *entry & PRESENT == 0 {
        let frame = frame_allocator::allocate_frame().ok_or(PagingError::OutOfFrames)?;
        unsafe {
            ptr::write_bytes(frame.start as *mut u8, 0, FRAME_SIZE);
        }
        *entry = frame.start as u64 | PRESENT | WRITABLE | (flags.0 & USER);
    } else if flags.0 & USER != 0 {
        *entry |= USER;
    }

    Ok((*entry & ADDR_MASK) as *mut PageTable)
}

unsafe fn next_table(_p4: *mut PageTable, entry: &mut u64) -> Result<*mut PageTable, PagingError> {
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

pub const fn align_down(addr: usize, align: usize) -> usize {
    addr & !(align - 1)
}

pub const fn align_up(addr: usize, align: usize) -> usize {
    (addr + align - 1) & !(align - 1)
}

pub const USER_STACK_TOP: usize = 0x7000_2000;
pub const USER_STACK_GUARD: usize = 0x7000_0000;
pub const USER_HEAP_START: usize = 0x6000_0000;
