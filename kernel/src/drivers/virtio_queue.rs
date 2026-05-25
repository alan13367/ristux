use core::mem;

/// Guest-memory VirtIO virtqueue with descriptor, available, and used rings.
#[repr(C, align(4096))]
pub struct VirtQueueMem {
    pub descriptors: [VirtqDesc; QUEUE_SIZE],
    pub avail: VirtqAvail,
    pub used: VirtqUsed,
}

const QUEUE_SIZE: usize = 16;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct VirtqDesc {
    pub addr: u64,
    pub len: u32,
    pub flags: u16,
    pub next: u16,
}

#[repr(C)]
pub struct VirtqAvail {
    pub flags: u16,
    pub idx: u16,
    pub ring: [u16; QUEUE_SIZE],
    pub used_event: u16,
}

#[repr(C)]
pub struct VirtqUsed {
    pub flags: u16,
    pub idx: u16,
    pub ring: [VirtqUsedElem; QUEUE_SIZE],
    pub avail_event: u16,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct VirtqUsedElem {
    pub id: u32,
    pub len: u32,
}

pub const VIRTQ_DESC_F_NEXT: u16 = 1;
pub const VIRTQ_DESC_F_WRITE: u16 = 2;

pub struct VirtQueue {
    pub index: u32,
    pub mem: *mut VirtQueueMem,
    pub size: u16,
    pub last_used_idx: u16,
}

impl VirtQueue {
    pub fn new(index: u32, mem: *mut VirtQueueMem) -> Self {
        unsafe {
            (*mem).avail.idx = 0;
            (*mem).used.idx = 0;
        }
        Self {
            index,
            mem,
            size: QUEUE_SIZE as u16,
            last_used_idx: 0,
        }
    }

    pub fn phys_addr(&self) -> u64 {
        self.mem as u64
    }

    pub fn desc_phys(&self) -> u64 {
        self.phys_addr()
    }

    pub fn avail_phys(&self) -> u64 {
        unsafe {
            self.phys_addr().wrapping_add(
                (core::ptr::from_ref(&(*self.mem).avail) as u64).wrapping_sub(self.phys_addr()),
            )
        }
    }

    pub fn used_phys(&self) -> u64 {
        unsafe {
            self.phys_addr().wrapping_add(
                (core::ptr::from_ref(&(*self.mem).used) as u64).wrapping_sub(self.phys_addr()),
            )
        }
    }

    pub fn submit_chain(&mut self, head: u16) -> Result<(), ()> {
        unsafe {
            let avail = &mut (*self.mem).avail;
            let slot = (avail.idx as usize) % QUEUE_SIZE;
            avail.ring[slot] = head;
            avail.idx = avail.idx.wrapping_add(1);
        }
        Ok(())
    }

    pub fn set_desc(&mut self, index: u16, addr: u64, len: u32, flags: u16, next: u16) {
        unsafe {
            let desc = &mut (*self.mem).descriptors[index as usize];
            desc.addr = addr;
            desc.len = len;
            desc.flags = flags;
            desc.next = next;
        }
    }

    pub fn wait_used(&mut self, head: u16) -> Result<u32, ()> {
        for _ in 0..1_000_000 {
            unsafe {
                let used = &(*self.mem).used;
                if used.idx != self.last_used_idx {
                    let slot = (self.last_used_idx as usize) % QUEUE_SIZE;
                    let elem = used.ring[slot];
                    self.last_used_idx = self.last_used_idx.wrapping_add(1);
                    if elem.id as u16 == head {
                        return Ok(elem.len);
                    }
                }
            }
            core::hint::spin_loop();
        }
        Err(())
    }

    pub fn reset(&mut self) {
        unsafe {
            (*self.mem).avail.idx = 0;
            (*self.mem).used.idx = 0;
        }
        self.last_used_idx = 0;
    }
}

static mut QUEUE_MEM: VirtQueueMem = VirtQueueMem {
    descriptors: [VirtqDesc {
        addr: 0,
        len: 0,
        flags: 0,
        next: 0,
    }; QUEUE_SIZE],
    avail: VirtqAvail {
        flags: 0,
        idx: 0,
        ring: [0; QUEUE_SIZE],
        used_event: 0,
    },
    used: VirtqUsed {
        flags: 0,
        idx: 0,
        ring: [VirtqUsedElem { id: 0, len: 0 }; QUEUE_SIZE],
        avail_event: 0,
    },
};

pub fn queue_memory() -> *mut VirtQueueMem {
    unsafe { core::ptr::addr_of_mut!(QUEUE_MEM) }
}

pub fn self_test() -> bool {
    let mem = queue_memory();
    let mut queue = VirtQueue::new(0, mem);
    queue.set_desc(0, 0x1000, 512, 0, 0);
    queue.submit_chain(0).is_ok() && mem::size_of::<VirtqDesc>() == 16
}
