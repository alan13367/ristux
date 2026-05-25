use alloc::vec::Vec;
use core::ptr;

use super::{
    frame_allocator::{self, FRAME_SIZE, Frame},
    paging::{self, PageFlags, PageTable, PagingError},
};

unsafe impl Send for AddressSpace {}

pub struct AddressSpace {
    p4_frame: Frame,
    pub p4: *mut PageTable,
    pub user_mappings: Vec<(usize, Frame)>,
    pub heap_break: usize,
    pub stack_bottom: usize,
    pub stack_top: usize,
    pub mmap_next: usize,
}

pub const USER_MMAP_START: usize = 0x5000_0000;
pub const USER_MMAP_END: usize = 0x5800_0000;

impl AddressSpace {
    pub fn new_kernel_clone() -> Result<Self, PagingError> {
        let p4_frame = unsafe { paging::create_p4_table()? };
        Ok(Self {
            p4_frame,
            p4: p4_frame.start as *mut PageTable,
            user_mappings: Vec::new(),
            heap_break: paging::USER_HEAP_START,
            stack_bottom: paging::USER_STACK_GUARD,
            stack_top: paging::USER_STACK_TOP,
            mmap_next: USER_MMAP_START,
        })
    }

    pub fn p4_phys(&self) -> usize {
        self.p4_frame.start
    }

    pub fn activate(&self) {
        unsafe {
            paging::switch_cr3(self.p4_phys());
        }
    }

    pub unsafe fn map_user_page(
        &mut self,
        virt: usize,
        phys: usize,
        flags: PageFlags,
    ) -> Result<(), PagingError> {
        unsafe { paging::map_page_at(self.p4, virt, phys, flags) }?;
        Ok(())
    }

    pub unsafe fn map_owned_user_page(
        &mut self,
        virt: usize,
        frame: Frame,
        flags: PageFlags,
    ) -> Result<(), PagingError> {
        unsafe { self.map_user_page(virt, frame.start, flags)? };
        self.user_mappings.push((virt, frame));
        Ok(())
    }

    pub fn map_zero_page(&mut self, virt: usize) -> Result<(), PagingError> {
        self.map_zero_page_with_flags(virt, PageFlags::USER_WRITABLE)
    }

    pub fn map_zero_page_with_flags(
        &mut self,
        virt: usize,
        flags: PageFlags,
    ) -> Result<(), PagingError> {
        let frame = frame_allocator::allocate_frame().ok_or(PagingError::OutOfFrames)?;
        unsafe {
            ptr::write_bytes(frame.start as *mut u8, 0, FRAME_SIZE);
            self.map_user_page(virt, frame.start, flags)?;
        }
        self.user_mappings.push((virt, frame));
        Ok(())
    }

    pub fn unmap_user_page(&mut self, virt: usize) -> Result<(), PagingError> {
        let phys = unsafe { paging::unmap_page_at(self.p4, virt)? };
        if let Some(index) = self.user_mappings.iter().position(|(v, _)| *v == virt) {
            let (_, frame) = self.user_mappings.remove(index);
            if frame.start != phys {
                panic!("address space unmap frame mismatch");
            }
            if super::refcount::decrement(frame.start) == 0 {
                frame_allocator::free_frame(frame);
            }
        } else {
            if super::refcount::decrement(phys) == 0 {
                frame_allocator::free_frame(Frame { start: phys });
            }
        }
        crate::smp::send_tlb_shootdown();
        Ok(())
    }

    pub fn destroy(mut self) {
        while let Some((virt, _)) = self.user_mappings.pop() {
            let _ = self.unmap_user_page(virt);
        }
        unsafe {
            paging::free_user_page_tables(self.p4);
        }
        frame_allocator::free_frame(self.p4_frame);
    }

    pub fn mapping_count(&self) -> usize {
        self.user_mappings.len()
    }

    pub fn allows(&self, addr: usize, len: usize) -> bool {
        if len == 0 {
            return true;
        }
        let Some(end) = addr.checked_add(len) else {
            return false;
        };
        let mut page = paging::align_down(addr, FRAME_SIZE);
        let end_page = paging::align_up(end, FRAME_SIZE);
        while page < end_page {
            if !self.is_user_mapped(page) {
                return false;
            }
            page += FRAME_SIZE;
        }
        true
    }

    pub fn clone_full_copy(&self) -> Result<Self, PagingError> {
        let mut clone = Self::new_kernel_clone()?;
        clone.user_mappings = Vec::with_capacity(self.user_mappings.len());
        for &(virt, ref frame) in &self.user_mappings {
            unsafe {
                if let Some(pte) = paging::get_pte_mut(self.p4, virt) {
                    let mut flags = *pte & !paging::ADDR_MASK;
                    if flags & paging::WRITABLE_FLAG != 0 {
                        flags &= !paging::WRITABLE_FLAG;
                        flags |= paging::COW_FLAG;
                        *pte = (*pte & paging::ADDR_MASK) | flags;
                        paging::flush(virt);
                        crate::smp::send_tlb_shootdown();
                    }
                    clone.map_user_page(virt, frame.start, PageFlags::from_raw(flags))?;
                    super::refcount::increment(frame.start);
                }
            }
            clone.user_mappings.push((virt, *frame));
        }
        clone.heap_break = self.heap_break;
        clone.stack_bottom = self.stack_bottom;
        clone.stack_top = self.stack_top;
        clone.mmap_next = self.mmap_next;
        Ok(clone)
    }

    pub fn map_anonymous(
        &mut self,
        hint: usize,
        len: usize,
        flags: PageFlags,
    ) -> Result<usize, PagingError> {
        let len = paging::align_up(len, FRAME_SIZE);
        let base = self.reserve_mmap_addr(hint, len)?;
        for page in (base..base + len).step_by(FRAME_SIZE) {
            self.map_zero_page_with_flags(page, flags)?;
        }
        self.mmap_next = (base + len).min(USER_MMAP_END);
        if self.mmap_next >= USER_MMAP_END {
            self.mmap_next = USER_MMAP_START;
        }
        Ok(base)
    }

    pub fn unmap_user_range(&mut self, addr: usize, len: usize) -> Result<(), PagingError> {
        let start = paging::align_down(addr, FRAME_SIZE);
        let end = paging::align_up(
            addr.checked_add(len).ok_or(PagingError::NotMapped)?,
            FRAME_SIZE,
        );
        if start < USER_MMAP_START || end > USER_MMAP_END || start >= end {
            return Err(PagingError::NotMapped);
        }
        let mut page = start;
        while page < end {
            if self.is_user_mapped(page) {
                self.unmap_user_page(page)?;
            }
            page += FRAME_SIZE;
        }
        Ok(())
    }

    pub fn protect_user_range(
        &mut self,
        addr: usize,
        len: usize,
        writable: bool,
    ) -> Result<(), PagingError> {
        let start = paging::align_down(addr, FRAME_SIZE);
        let end = paging::align_up(
            addr.checked_add(len).ok_or(PagingError::NotMapped)?,
            FRAME_SIZE,
        );
        if start >= end {
            return Err(PagingError::NotMapped);
        }
        let mut page = start;
        while page < end {
            if !self.is_user_mapped(page) {
                return Err(PagingError::NotMapped);
            }
            unsafe {
                self.protect_user_page(page, writable)?;
            }
            page += FRAME_SIZE;
        }
        crate::smp::send_tlb_shootdown();
        Ok(())
    }

    pub fn grow_stack(&mut self, fault_addr: usize) -> Result<(), PagingError> {
        let page = paging::align_down(fault_addr, FRAME_SIZE);
        if !self.can_grow_stack(fault_addr) {
            return Err(PagingError::NotMapped);
        }
        if self.user_mappings.iter().any(|(virt, _)| *virt == page) {
            return Ok(());
        }
        self.map_zero_page(page)?;
        if page < self.stack_bottom {
            self.stack_bottom = page;
        }
        Ok(())
    }

    pub fn can_grow_stack(&self, fault_addr: usize) -> bool {
        let page = paging::align_down(fault_addr, FRAME_SIZE);
        page > paging::USER_STACK_GUARD && page + FRAME_SIZE <= self.stack_top
    }

    pub fn grow_heap(&mut self, new_break: usize) -> Result<(), PagingError> {
        if new_break < paging::USER_HEAP_START {
            return Err(PagingError::NotMapped);
        }
        let aligned = paging::align_up(new_break, FRAME_SIZE);
        let mut page = paging::align_up(self.heap_break, FRAME_SIZE);
        while page < aligned {
            if !self.user_mappings.iter().any(|(virt, _)| *virt == page) {
                self.map_zero_page(page)?;
            }
            page += FRAME_SIZE;
        }
        self.heap_break = new_break;
        Ok(())
    }

    fn reserve_mmap_addr(&self, hint: usize, len: usize) -> Result<usize, PagingError> {
        if len == 0 || len > USER_MMAP_END - USER_MMAP_START {
            return Err(PagingError::NotMapped);
        }
        if hint != 0 {
            let candidate = paging::align_down(hint, FRAME_SIZE);
            if self.range_available(candidate, len) {
                return Ok(candidate);
            }
        }
        let start = paging::align_up(self.mmap_next, FRAME_SIZE).max(USER_MMAP_START);
        if let Some(candidate) = self.find_free_range(start, USER_MMAP_END, len) {
            return Ok(candidate);
        }
        self.find_free_range(USER_MMAP_START, start, len)
            .ok_or(PagingError::OutOfFrames)
    }

    fn find_free_range(&self, start: usize, end: usize, len: usize) -> Option<usize> {
        let mut candidate = paging::align_up(start, FRAME_SIZE);
        while candidate.checked_add(len)? <= end {
            if self.range_available(candidate, len) {
                return Some(candidate);
            }
            candidate += FRAME_SIZE;
        }
        None
    }

    fn range_available(&self, start: usize, len: usize) -> bool {
        let Some(end) = start.checked_add(len) else {
            return false;
        };
        if start < USER_MMAP_START || end > USER_MMAP_END || start >= end {
            return false;
        }
        let mut page = start;
        while page < end {
            if self.is_user_mapped(page) {
                return false;
            }
            page += FRAME_SIZE;
        }
        true
    }

    fn is_user_mapped(&self, page: usize) -> bool {
        self.user_mappings.iter().any(|(virt, _)| *virt == page)
    }

    unsafe fn protect_user_page(&mut self, page: usize, writable: bool) -> Result<(), PagingError> {
        let pte = unsafe { paging::get_pte_mut(self.p4, page).ok_or(PagingError::NotMapped)? };
        let phys = *pte & paging::ADDR_MASK;
        let was_cow = *pte & paging::COW_FLAG != 0;
        let shared = super::refcount::get(phys as usize) > 1;
        let mut flags = paging::PRESENT_FLAG | paging::USER_FLAG;
        if was_cow || (writable && shared) {
            flags |= paging::COW_FLAG;
        } else if writable {
            flags |= paging::WRITABLE_FLAG;
        }
        *pte = phys | flags;
        unsafe {
            paging::flush(page);
        }
        Ok(())
    }
}

pub fn self_test() {
    let mut space_a = AddressSpace::new_kernel_clone().expect("address space A creation failed");
    let mut space_b = AddressSpace::new_kernel_clone().expect("address space B creation failed");

    const VIRT: usize = 0x4010_0000;
    space_a
        .map_zero_page(VIRT)
        .expect("address space A map failed");
    space_b
        .map_zero_page(VIRT)
        .expect("address space B map failed");

    space_a.activate();
    unsafe {
        ptr::write_volatile(VIRT as *mut u64, 0xaaa1);
    }

    space_b.activate();
    unsafe {
        ptr::write_volatile(VIRT as *mut u64, 0xbbb2);
    }

    space_a.activate();
    let value_a = unsafe { ptr::read_volatile(VIRT as *mut u64) };
    if value_a != 0xaaa1 {
        panic!("address space isolation failed: A read {:#x}", value_a);
    }

    space_b.activate();
    let value_b = unsafe { ptr::read_volatile(VIRT as *mut u64) };
    if value_b != 0xbbb2 {
        panic!("address space isolation failed: B read {:#x}", value_b);
    }

    unsafe {
        paging::switch_cr3(paging::boot_root_table() as usize);
    }

    space_a.destroy();
    space_b.destroy();
    crate::println!("Address space isolation self-test passed.");
}
