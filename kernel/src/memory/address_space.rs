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
    user_protections: Vec<(usize, UserProtection)>,
    pub heap_break: usize,
    pub stack_bottom: usize,
    pub stack_top: usize,
    pub mmap_next: usize,
}

pub const USER_MMAP_START: usize = 0x5000_0000;
pub const USER_MMAP_END: usize = 0x5800_0000;

#[derive(Clone, Copy, Eq, PartialEq)]
pub enum UserProtection {
    None,
    ReadOnly,
    ReadExecute,
    ReadWrite,
}

impl UserProtection {
    pub fn page_flags(self) -> PageFlags {
        match self {
            Self::None => PageFlags::user_no_access(),
            Self::ReadOnly => PageFlags::user_readable(false),
            Self::ReadExecute => PageFlags::user_readable(true),
            Self::ReadWrite => PageFlags::user_writable(),
        }
    }

    pub const fn allows_read(self) -> bool {
        matches!(self, Self::ReadOnly | Self::ReadExecute | Self::ReadWrite)
    }

    pub const fn allows_write(self) -> bool {
        matches!(self, Self::ReadWrite)
    }

    pub const fn allows_execute(self) -> bool {
        matches!(self, Self::ReadExecute)
    }
}

#[derive(Clone, Copy, Eq, PartialEq)]
pub enum UserAccess {
    Read,
    Write,
    Execute,
}

impl AddressSpace {
    pub fn new_kernel_clone() -> Result<Self, PagingError> {
        let p4_frame = unsafe { paging::create_p4_table()? };
        Ok(Self {
            p4_frame,
            p4: p4_frame.start as *mut PageTable,
            user_mappings: Vec::new(),
            user_protections: Vec::new(),
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

    fn reserve_user_mapping_entries(&mut self, additional: usize) -> Result<(), PagingError> {
        self.user_mappings
            .try_reserve_exact(additional)
            .map_err(|_| PagingError::OutOfFrames)?;
        self.user_protections
            .try_reserve_exact(additional)
            .map_err(|_| PagingError::OutOfFrames)
    }

    pub unsafe fn map_owned_user_page(
        &mut self,
        virt: usize,
        frame: Frame,
        protection: UserProtection,
    ) -> Result<(), PagingError> {
        self.reserve_user_mapping_entries(1)?;
        unsafe { self.map_user_page(virt, frame.start, protection.page_flags())? };
        self.user_mappings.push((virt, frame));
        self.user_protections.push((virt, protection));
        Ok(())
    }

    pub fn map_zero_page(&mut self, virt: usize) -> Result<(), PagingError> {
        self.map_zero_page_with_protection(virt, UserProtection::ReadWrite)
    }

    pub fn map_zero_page_with_protection(
        &mut self,
        virt: usize,
        protection: UserProtection,
    ) -> Result<(), PagingError> {
        self.reserve_user_mapping_entries(1)?;
        let frame = frame_allocator::allocate_frame().ok_or(PagingError::OutOfFrames)?;
        unsafe {
            ptr::write_bytes(frame.start as *mut u8, 0, FRAME_SIZE);
            if let Err(err) = self.map_user_page(virt, frame.start, protection.page_flags()) {
                frame_allocator::free_frame(frame);
                return Err(err);
            }
        }
        self.user_mappings.push((virt, frame));
        self.user_protections.push((virt, protection));
        Ok(())
    }

    pub fn unmap_user_page(&mut self, virt: usize) -> Result<(), PagingError> {
        let phys = unsafe { paging::unmap_page_at(self.p4, virt)? };
        if let Some(index) = self.user_protections.iter().position(|(v, _)| *v == virt) {
            self.user_protections.swap_remove(index);
        }
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

    pub fn clear_user_pages(&mut self) {
        while let Some((virt, _)) = self.user_mappings.first().copied() {
            let _ = self.unmap_user_page(virt);
        }
        self.user_protections.clear();
        self.heap_break = paging::USER_HEAP_START;
        self.stack_bottom = paging::USER_STACK_GUARD;
        self.stack_top = paging::USER_STACK_TOP;
        self.mmap_next = USER_MMAP_START;
    }

    pub fn destroy(mut self) {
        self.clear_user_pages();
        unsafe {
            paging::free_user_page_tables(self.p4);
        }
        frame_allocator::free_frame(self.p4_frame);
    }

    pub fn mapping_count(&self) -> usize {
        self.user_mappings.len()
    }

    pub fn allows_user(&self, addr: usize, len: usize, access: UserAccess) -> bool {
        if len == 0 {
            return true;
        }
        let Some(end) = addr.checked_add(len) else {
            return false;
        };
        let mut page = paging::align_down(addr, FRAME_SIZE);
        let Some(end_page) = paging::checked_align_up(end, FRAME_SIZE) else {
            return false;
        };
        while page < end_page {
            if !self.page_allows_user(page, access) {
                return false;
            }
            page += FRAME_SIZE;
        }
        true
    }

    pub fn ensure_user_writable(&mut self, addr: usize, len: usize) -> Result<(), PagingError> {
        if len == 0 {
            return Ok(());
        }
        let end = addr.checked_add(len).ok_or(PagingError::NotMapped)?;
        let mut page = paging::align_down(addr, FRAME_SIZE);
        let end_page = paging::checked_align_up(end, FRAME_SIZE).ok_or(PagingError::NotMapped)?;
        while page < end_page {
            self.ensure_user_page_writable(page)?;
            page += FRAME_SIZE;
        }
        Ok(())
    }

    pub fn clone_full_copy(&self) -> Result<Self, PagingError> {
        let mut clone = Self::new_kernel_clone()?;
        if let Err(err) = clone.reserve_user_mapping_entries(self.user_mappings.len()) {
            clone.destroy();
            return Err(err);
        }
        for &(virt, frame) in &self.user_mappings {
            let protection = match self.protection_for_page(virt) {
                Some(protection) => protection,
                None => {
                    clone.destroy();
                    return Err(PagingError::NotMapped);
                }
            };
            unsafe {
                let Some(pte) = paging::get_pte_mut(self.p4, virt) else {
                    clone.destroy();
                    return Err(PagingError::NotMapped);
                };
                let flags = *pte & !paging::ADDR_MASK;
                let mut clone_flags = flags;
                if clone_flags & paging::WRITABLE_FLAG != 0 {
                    clone_flags &= !paging::WRITABLE_FLAG;
                    clone_flags |= paging::COW_FLAG;
                }
                if !super::refcount::try_increment(frame.start) {
                    clone.destroy();
                    return Err(PagingError::RefcountOverflow);
                }
                if let Err(err) =
                    clone.map_user_page(virt, frame.start, PageFlags::from_raw(clone_flags))
                {
                    let _ = super::refcount::decrement(frame.start);
                    clone.destroy();
                    return Err(err);
                }
                clone.user_mappings.push((virt, frame));
                clone.user_protections.push((virt, protection));
                if clone_flags != flags {
                    *pte = (*pte & paging::ADDR_MASK) | clone_flags;
                    paging::flush(virt);
                    crate::smp::send_tlb_shootdown();
                }
            }
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
        protection: UserProtection,
    ) -> Result<usize, PagingError> {
        let len = paging::checked_align_up(len, FRAME_SIZE).ok_or(PagingError::NotMapped)?;
        let base = self.reserve_mmap_addr(hint, len)?;
        let mut mapped = Vec::new();
        let page_count = len / FRAME_SIZE;
        mapped
            .try_reserve_exact(page_count)
            .map_err(|_| PagingError::OutOfFrames)?;
        self.reserve_user_mapping_entries(page_count)?;
        let end = base.checked_add(len).ok_or(PagingError::NotMapped)?;
        for page in (base..end).step_by(FRAME_SIZE) {
            if let Err(err) = self.map_zero_page_with_protection(page, protection) {
                for mapped_page in mapped {
                    let _ = self.unmap_user_page(mapped_page);
                }
                return Err(err);
            }
            mapped.push(page);
        }
        self.mmap_next = end.min(USER_MMAP_END);
        if self.mmap_next >= USER_MMAP_END {
            self.mmap_next = USER_MMAP_START;
        }
        Ok(base)
    }

    pub fn map_fixed(
        &mut self,
        addr: usize,
        len: usize,
        protection: UserProtection,
    ) -> Result<usize, PagingError> {
        let len = paging::checked_align_up(len, FRAME_SIZE).ok_or(PagingError::NotMapped)?;
        if addr % FRAME_SIZE != 0 || len == 0 {
            return Err(PagingError::NotMapped);
        }
        let end = addr.checked_add(len).ok_or(PagingError::NotMapped)?;
        if addr < USER_MMAP_START || end > USER_MMAP_END || addr >= end {
            return Err(PagingError::NotMapped);
        }
        let mut mapped = Vec::new();
        let page_count = len / FRAME_SIZE;
        mapped
            .try_reserve_exact(page_count)
            .map_err(|_| PagingError::OutOfFrames)?;
        self.reserve_user_mapping_entries(page_count)?;
        self.unmap_user_range(addr, len)?;
        for page in (addr..end).step_by(FRAME_SIZE) {
            if let Err(err) = self.map_zero_page_with_protection(page, protection) {
                for mapped_page in mapped {
                    let _ = self.unmap_user_page(mapped_page);
                }
                return Err(err);
            }
            mapped.push(page);
        }
        self.mmap_next = end.min(USER_MMAP_END);
        if self.mmap_next >= USER_MMAP_END {
            self.mmap_next = USER_MMAP_START;
        }
        Ok(addr)
    }

    pub fn unmap_user_range(&mut self, addr: usize, len: usize) -> Result<(), PagingError> {
        let start = paging::align_down(addr, FRAME_SIZE);
        let range_end = addr.checked_add(len).ok_or(PagingError::NotMapped)?;
        let end = paging::checked_align_up(range_end, FRAME_SIZE).ok_or(PagingError::NotMapped)?;
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
        protection: UserProtection,
    ) -> Result<(), PagingError> {
        let start = paging::align_down(addr, FRAME_SIZE);
        let range_end = addr.checked_add(len).ok_or(PagingError::NotMapped)?;
        let end = paging::checked_align_up(range_end, FRAME_SIZE).ok_or(PagingError::NotMapped)?;
        if start >= end {
            return Err(PagingError::NotMapped);
        }
        let mut page = start;
        while page < end {
            if !self.is_user_mapped(page) {
                return Err(PagingError::NotMapped);
            }
            if self.protection_for_page(page).is_none() {
                return Err(PagingError::NotMapped);
            }
            if unsafe { paging::get_pte_mut(self.p4, page) }
                .is_none_or(|pte| *pte & paging::PRESENT_FLAG == 0)
            {
                return Err(PagingError::NotMapped);
            }
            page += FRAME_SIZE;
        }
        page = start;
        while page < end {
            unsafe {
                self.protect_user_page(page, protection)?;
            }
            self.set_page_protection(page, protection)?;
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
        if self.is_user_mapped(page) {
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
        let Some(page_end) = page.checked_add(FRAME_SIZE) else {
            return false;
        };
        page > paging::USER_STACK_GUARD && page_end <= self.stack_top
    }

    pub fn grow_heap(&mut self, new_break: usize) -> Result<(), PagingError> {
        if new_break < paging::USER_HEAP_START {
            return Err(PagingError::NotMapped);
        }
        let aligned =
            paging::checked_align_up(new_break, FRAME_SIZE).ok_or(PagingError::NotMapped)?;
        if aligned > paging::USER_HEAP_END {
            return Err(PagingError::NotMapped);
        }

        let old_aligned =
            paging::checked_align_up(self.heap_break, FRAME_SIZE).ok_or(PagingError::NotMapped)?;
        if aligned < old_aligned {
            let mut page = aligned;
            while page < old_aligned {
                if self.is_user_mapped(page) {
                    self.unmap_user_page(page)?;
                }
                page += FRAME_SIZE;
            }
            self.heap_break = new_break;
            return Ok(());
        }

        let mut page = old_aligned;
        let mut pages_to_map = 0usize;
        while page < aligned {
            if !self.is_user_mapped(page) {
                pages_to_map += 1;
            }
            page += FRAME_SIZE;
        }
        let mut mapped = Vec::new();
        mapped
            .try_reserve_exact(pages_to_map)
            .map_err(|_| PagingError::OutOfFrames)?;
        self.reserve_user_mapping_entries(pages_to_map)?;
        page = old_aligned;
        while page < aligned {
            if !self.is_user_mapped(page) {
                if let Err(err) = self.map_zero_page(page) {
                    for mapped_page in mapped {
                        let _ = self.unmap_user_page(mapped_page);
                    }
                    return Err(err);
                }
                mapped.push(page);
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

    pub fn is_user_mapped(&self, page: usize) -> bool {
        self.user_mappings.iter().any(|(virt, _)| *virt == page)
    }

    unsafe fn protect_user_page(
        &mut self,
        page: usize,
        protection: UserProtection,
    ) -> Result<(), PagingError> {
        let pte = unsafe { paging::get_pte_mut(self.p4, page).ok_or(PagingError::NotMapped)? };
        let phys = *pte & paging::ADDR_MASK;
        let was_cow = *pte & paging::COW_FLAG != 0;
        let shared = super::refcount::get(phys as usize) > 1;
        let mut flags = paging::PRESENT_FLAG;
        match protection {
            UserProtection::None => {
                if paging::nx_enabled() {
                    flags |= paging::NX_FLAG;
                }
                if was_cow {
                    flags |= paging::COW_FLAG;
                }
            }
            UserProtection::ReadOnly | UserProtection::ReadExecute => {
                flags |= paging::USER_FLAG;
                if !protection.allows_execute() && paging::nx_enabled() {
                    flags |= paging::NX_FLAG;
                }
                if was_cow {
                    flags |= paging::COW_FLAG;
                }
            }
            UserProtection::ReadWrite => {
                flags |= paging::USER_FLAG;
                if paging::nx_enabled() {
                    flags |= paging::NX_FLAG;
                }
                if was_cow || shared {
                    flags |= paging::COW_FLAG;
                } else {
                    flags |= paging::WRITABLE_FLAG;
                }
            }
        }
        *pte = phys | flags;
        unsafe {
            paging::flush(page);
        }
        Ok(())
    }

    fn page_allows_user(&self, page: usize, access: UserAccess) -> bool {
        let Some(protection) = self.protection_for_page(page) else {
            return false;
        };
        match access {
            UserAccess::Read if !protection.allows_read() => return false,
            UserAccess::Write if !protection.allows_write() => return false,
            UserAccess::Execute if !protection.allows_execute() => return false,
            _ => {}
        }

        let Some(pte) = (unsafe { paging::get_pte_mut(self.p4, page) }) else {
            return false;
        };
        if *pte & paging::PRESENT_FLAG == 0 || *pte & paging::USER_FLAG == 0 {
            return false;
        }
        match access {
            UserAccess::Read => true,
            UserAccess::Write => *pte & paging::WRITABLE_FLAG != 0 || *pte & paging::COW_FLAG != 0,
            UserAccess::Execute => !paging::nx_enabled() || *pte & paging::NX_FLAG == 0,
        }
    }

    fn ensure_user_page_writable(&mut self, page: usize) -> Result<(), PagingError> {
        if self.protection_for_page(page) != Some(UserProtection::ReadWrite) {
            return Err(PagingError::NotMapped);
        }
        let pte = unsafe { paging::get_pte_mut(self.p4, page).ok_or(PagingError::NotMapped)? };
        if *pte & paging::PRESENT_FLAG == 0 || *pte & paging::USER_FLAG == 0 {
            return Err(PagingError::NotMapped);
        }
        if *pte & paging::WRITABLE_FLAG != 0 {
            return Ok(());
        }
        if *pte & paging::COW_FLAG == 0 {
            return Err(PagingError::NotMapped);
        }
        self.break_cow_page(page)
    }

    fn break_cow_page(&mut self, page: usize) -> Result<(), PagingError> {
        let pte = unsafe { paging::get_pte_mut(self.p4, page).ok_or(PagingError::NotMapped)? };
        let old_frame_phys = (*pte & paging::ADDR_MASK) as usize;
        let mapping_pos = self
            .user_mappings
            .iter()
            .position(|(virt, _)| *virt == page)
            .ok_or(PagingError::NotMapped)?;
        let ref_count = super::refcount::get(old_frame_phys);
        let mut flags = *pte & !paging::ADDR_MASK;
        flags |= paging::WRITABLE_FLAG;
        flags &= !paging::COW_FLAG;

        if ref_count <= 1 {
            *pte = (*pte & paging::ADDR_MASK) | flags;
            unsafe {
                paging::flush(page);
            }
            crate::smp::send_tlb_shootdown();
            return Ok(());
        }

        let new_frame = frame_allocator::allocate_frame().ok_or(PagingError::OutOfFrames)?;
        unsafe {
            ptr::copy_nonoverlapping(page as *const u8, new_frame.start as *mut u8, FRAME_SIZE);
        }
        super::refcount::decrement(old_frame_phys);
        *pte = new_frame.start as u64 | flags;
        unsafe {
            paging::flush(page);
        }
        crate::smp::send_tlb_shootdown();

        self.user_mappings[mapping_pos] = (page, new_frame);
        Ok(())
    }

    fn protection_for_page(&self, page: usize) -> Option<UserProtection> {
        self.user_protections
            .iter()
            .find(|(virt, _)| *virt == page)
            .map(|(_, protection)| *protection)
    }

    fn set_page_protection(
        &mut self,
        page: usize,
        protection: UserProtection,
    ) -> Result<(), PagingError> {
        let entry = self
            .user_protections
            .iter_mut()
            .find(|(virt, _)| *virt == page)
            .ok_or(PagingError::NotMapped)?;
        entry.1 = protection;
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
    assert_user_page_nx(&space_a, VIRT, true, "writable page");
    space_a
        .protect_user_range(VIRT, FRAME_SIZE, UserProtection::ReadOnly)
        .expect("address space read-only protection failed");
    assert_user_page_nx(&space_a, VIRT, true, "read-only page");
    space_a
        .protect_user_range(VIRT, FRAME_SIZE, UserProtection::ReadExecute)
        .expect("address space executable protection failed");
    assert_user_page_nx(&space_a, VIRT, false, "executable page");
    space_a
        .protect_user_range(VIRT, FRAME_SIZE, UserProtection::ReadWrite)
        .expect("address space writable protection restore failed");
    assert_user_page_nx(&space_a, VIRT, true, "restored writable page");
    assert_mprotect_metadata_failure_is_atomic(&mut space_a);

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

fn assert_user_page_nx(space: &AddressSpace, page: usize, expected: bool, label: &str) {
    if !paging::nx_enabled() {
        return;
    }
    let pte = unsafe { paging::get_pte_mut(space.p4, page) }
        .unwrap_or_else(|| panic!("address space NX self-test missing {}", label));
    let actual = *pte & paging::NX_FLAG != 0;
    if actual != expected {
        panic!(
            "address space NX self-test {} expected {}, got {}",
            label, expected, actual
        );
    }
}

fn assert_mprotect_metadata_failure_is_atomic(space: &mut AddressSpace) {
    const BASE: usize = 0x4020_0000;
    let second = BASE + FRAME_SIZE;

    space
        .map_zero_page(BASE)
        .expect("address space mprotect atomic test first map failed");
    space
        .map_zero_page(second)
        .expect("address space mprotect atomic test second map failed");

    let Some(index) = space
        .user_protections
        .iter()
        .position(|(virt, _)| *virt == second)
    else {
        panic!("address space mprotect atomic test missing metadata");
    };
    space.user_protections.swap_remove(index);

    if space
        .protect_user_range(BASE, FRAME_SIZE * 2, UserProtection::ReadOnly)
        .is_ok()
    {
        panic!("address space mprotect atomic test accepted missing metadata");
    }
    if space.protection_for_page(BASE) != Some(UserProtection::ReadWrite) {
        panic!("address space mprotect atomic test changed first page metadata");
    }
    assert_user_page_writable(space, BASE, true, "mprotect atomic first page");
    assert_user_page_writable(space, second, true, "mprotect atomic second page");
}

fn assert_user_page_writable(space: &AddressSpace, page: usize, expected: bool, label: &str) {
    let pte = unsafe { paging::get_pte_mut(space.p4, page) }
        .unwrap_or_else(|| panic!("address space writable self-test missing {}", label));
    let actual = *pte & paging::WRITABLE_FLAG != 0;
    if actual != expected {
        panic!(
            "address space writable self-test {} expected {}, got {}",
            label, expected, actual
        );
    }
}
