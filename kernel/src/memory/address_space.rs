use alloc::vec::Vec;
use core::ptr;

use super::{
    frame_allocator::{self, Frame, FRAME_SIZE},
    paging::{self, PageFlags, PageTable, PagingError},
};

unsafe impl Send for AddressSpace {}

pub struct AddressSpace {
    p4_frame: Frame,
    p4: *mut PageTable,
    user_mappings: Vec<(usize, Frame)>,
    pub heap_break: usize,
    pub stack_bottom: usize,
    pub stack_top: usize,
}

impl AddressSpace {
    pub fn new_kernel_clone() -> Result<Self, PagingError> {
        let boot = paging::boot_root_table();
        let p4_frame = unsafe { paging::clone_p4_table(boot)? };
        Ok(Self {
            p4_frame,
            p4: p4_frame.start as *mut PageTable,
            user_mappings: Vec::new(),
            heap_break: paging::USER_HEAP_START,
            stack_bottom: paging::USER_STACK_GUARD,
            stack_top: paging::USER_STACK_TOP,
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
        let frame =
            frame_allocator::allocate_frame().ok_or(PagingError::OutOfFrames)?;
        unsafe {
            ptr::write_bytes(frame.start as *mut u8, 0, FRAME_SIZE);
            self.map_user_page(virt, frame.start, PageFlags::USER_WRITABLE)?;
        }
        self.user_mappings.push((virt, frame));
        Ok(())
    }

    pub fn unmap_user_page(&mut self, virt: usize) -> Result<(), PagingError> {
        let phys = unsafe { paging::unmap_page_at(self.p4, virt)? };
        if let Some(index) = self
            .user_mappings
            .iter()
            .position(|(v, _)| *v == virt)
        {
            let (_, frame) = self.user_mappings.remove(index);
            if frame.start != phys {
                panic!("address space unmap frame mismatch");
            }
            frame_allocator::free_frame(frame);
        } else {
            frame_allocator::free_frame(Frame { start: phys });
        }
        Ok(())
    }

    pub fn destroy(mut self) {
        while let Some((virt, _)) = self.user_mappings.pop() {
            let _ = self.unmap_user_page(virt);
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
        self.user_mappings
            .iter()
            .any(|(virt, _)| addr >= *virt && end <= virt + FRAME_SIZE)
    }

    pub fn clone_full_copy(&self) -> Result<Self, PagingError> {
        let mut clone = Self::new_kernel_clone()?;
        for &(virt, ref frame) in &self.user_mappings {
            let new_frame =
                frame_allocator::allocate_frame().ok_or(PagingError::OutOfFrames)?;
            unsafe {
                ptr::copy_nonoverlapping(
                    frame.start as *const u8,
                    new_frame.start as *mut u8,
                    FRAME_SIZE,
                );
                clone.map_user_page(virt, new_frame.start, PageFlags::USER_WRITABLE)?;
            }
            clone.user_mappings.push((virt, new_frame));
        }
        clone.heap_break = self.heap_break;
        clone.stack_bottom = self.stack_bottom;
        clone.stack_top = self.stack_top;
        Ok(clone)
    }

    pub fn grow_stack(&mut self, fault_addr: usize) -> Result<(), PagingError> {
        let page = paging::align_down(fault_addr, FRAME_SIZE);
        if page < self.stack_bottom || page + FRAME_SIZE > self.stack_top {
            return Err(PagingError::NotMapped);
        }
        if self
            .user_mappings
            .iter()
            .any(|(virt, _)| *virt == page)
        {
            return Ok(());
        }
        self.map_zero_page(page)?;
        if page < self.stack_bottom + FRAME_SIZE {
            self.stack_bottom = page;
        }
        Ok(())
    }

    pub fn grow_heap(&mut self, new_break: usize) -> Result<(), PagingError> {
        if new_break < paging::USER_HEAP_START {
            return Err(PagingError::NotMapped);
        }
        let aligned = paging::align_up(new_break, FRAME_SIZE);
        let mut page = paging::align_up(self.heap_break, FRAME_SIZE);
        while page < aligned {
            if !self
                .user_mappings
                .iter()
                .any(|(virt, _)| *virt == page)
            {
                self.map_zero_page(page)?;
            }
            page += FRAME_SIZE;
        }
        self.heap_break = new_break;
        Ok(())
    }
}

pub fn self_test() {
    let mut space_a = AddressSpace::new_kernel_clone()
        .expect("address space A creation failed");
    let mut space_b = AddressSpace::new_kernel_clone()
        .expect("address space B creation failed");

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
