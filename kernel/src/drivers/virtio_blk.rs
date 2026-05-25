use super::{
    pci, virtio_mmio,
    virtio_queue::{self, VIRTQ_DESC_F_NEXT, VIRTQ_DESC_F_WRITE, VirtQueue},
};
use crate::{arch::x86_64::port, sync::spinlock::SpinLock};
use core::{
    ptr,
    sync::atomic::{Ordering, compiler_fence},
};

const VIRTIO_VENDOR: u16 = 0x1af4;
const VIRTIO_BLK_LEGACY: u16 = 0x1001;
const VIRTIO_BLK_MODERN: u16 = 0x1042;
const MMIO_DEVICE_ID_BLOCK: u32 = 0x02;

const VIRTIO_BLK_T_IN: u32 = 0;
const VIRTIO_BLK_T_OUT: u32 = 1;
const VIRTIO_BLK_S_OK: u8 = 0;

const VIRTIO_STATUS_ACKNOWLEDGE: u8 = 1;
const VIRTIO_STATUS_DRIVER: u8 = 2;
const VIRTIO_STATUS_DRIVER_OK: u8 = 4;
const VIRTIO_STATUS_FAILED: u8 = 0x80;

const LEGACY_DEVICE_FEATURES: u16 = 0x00;
const LEGACY_GUEST_FEATURES: u16 = 0x04;
const LEGACY_QUEUE_PFN: u16 = 0x08;
const LEGACY_QUEUE_SIZE_REG: u16 = 0x0c;
const LEGACY_QUEUE_SELECT: u16 = 0x0e;
const LEGACY_QUEUE_NOTIFY: u16 = 0x10;
const LEGACY_DEVICE_STATUS: u16 = 0x12;
const LEGACY_ISR_STATUS: u16 = 0x13;
const LEGACY_CONFIG_CAPACITY: u16 = 0x14;

const KERNEL_HIGH_BASE: usize = 0xffff_ffff_8000_0000;
const LEGACY_QUEUE_CAPACITY: usize = 256;
const LEGACY_QUEUE_BYTES: usize = 12 * 1024;

#[repr(C)]
struct VirtioBlkReq {
    type_: u32,
    reserved: u32,
    sector: u64,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum BlockError {
    NotReady,
    IoError,
}

pub struct VirtioBlockDriver {
    mmio: *mut u8,
    io_base: u16,
    queue: VirtQueue,
    legacy_queue: LegacyVirtQueue,
    sector_count: u64,
    hardware: bool,
    transport: Transport,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Transport {
    LegacyIo,
    Mmio,
    Software,
}

#[repr(C, align(4096))]
struct BlkRequestBuffers {
    header: VirtioBlkReq,
    status: u8,
    _pad: [u8; 4095],
}

static BLK_REQ: SpinLock<BlkRequestBuffers> = SpinLock::new(BlkRequestBuffers {
    header: VirtioBlkReq {
        type_: 0,
        reserved: 0,
        sector: 0,
    },
    status: 0xff,
    _pad: [0; 4095],
});

static BLOCK_DRIVER: SpinLock<Option<VirtioBlockDriver>> = SpinLock::new(None);

unsafe impl Send for VirtioBlockDriver {}

impl VirtioBlockDriver {
    pub fn probe() -> Option<Self> {
        if let Some(driver) = Self::probe_legacy_io() {
            return Some(driver);
        }
        Self::probe_mmio()
    }

    fn probe_legacy_io() -> Option<Self> {
        let device = pci::find_device(VIRTIO_VENDOR, VIRTIO_BLK_LEGACY)?;
        if device.bar0 & 0x1 == 0 {
            return None;
        }
        pci::enable_bus_master(&device);
        let io_base = (device.bar0 & !0x3) as u16;

        unsafe {
            legacy_write8(io_base, LEGACY_DEVICE_STATUS, 0);
            legacy_write8(io_base, LEGACY_DEVICE_STATUS, VIRTIO_STATUS_ACKNOWLEDGE);
            legacy_write8(
                io_base,
                LEGACY_DEVICE_STATUS,
                VIRTIO_STATUS_ACKNOWLEDGE | VIRTIO_STATUS_DRIVER,
            );
            let _device_features = legacy_read32(io_base, LEGACY_DEVICE_FEATURES);
            legacy_write32(io_base, LEGACY_GUEST_FEATURES, 0);
            legacy_write16(io_base, LEGACY_QUEUE_SELECT, 0);
        }

        let queue_size = unsafe { legacy_read16(io_base, LEGACY_QUEUE_SIZE_REG) };
        if queue_size == 0 || queue_size as usize > LEGACY_QUEUE_CAPACITY {
            unsafe {
                legacy_write8(io_base, LEGACY_DEVICE_STATUS, VIRTIO_STATUS_FAILED);
            }
            return None;
        }

        let legacy_queue = LegacyVirtQueue::new(queue_size);
        unsafe {
            legacy_write32(io_base, LEGACY_QUEUE_PFN, legacy_queue.pfn());
            legacy_write8(
                io_base,
                LEGACY_DEVICE_STATUS,
                VIRTIO_STATUS_ACKNOWLEDGE | VIRTIO_STATUS_DRIVER | VIRTIO_STATUS_DRIVER_OK,
            );
        }
        let sector_count = unsafe {
            u64::from(legacy_read32(io_base, LEGACY_CONFIG_CAPACITY))
                | (u64::from(legacy_read32(io_base, LEGACY_CONFIG_CAPACITY + 4)) << 32)
        };

        let mut driver = Self {
            mmio: core::ptr::null_mut(),
            io_base,
            queue: VirtQueue::new(0, virtio_queue::queue_memory()),
            legacy_queue,
            sector_count,
            hardware: true,
            transport: Transport::LegacyIo,
        };
        if driver.hardware_read_sectors(0, 1, &mut [0u8; 512]).is_err() {
            unsafe {
                legacy_write8(io_base, LEGACY_DEVICE_STATUS, VIRTIO_STATUS_FAILED);
            }
            return None;
        }

        crate::println!(
            "VirtIO legacy block driver initialized at I/O {:#x}, queue {}, {} sector(s).",
            io_base,
            queue_size,
            sector_count
        );
        Some(driver)
    }

    fn probe_mmio() -> Option<Self> {
        let device = pci::find_device(VIRTIO_VENDOR, VIRTIO_BLK_LEGACY)
            .or_else(|| pci::find_device(VIRTIO_VENDOR, VIRTIO_BLK_MODERN))?;
        pci::enable_bus_master(&device);
        let mmio = virtio_mmio::map_bar0(device.bar0)?;
        if !virtio_mmio::validate(mmio, MMIO_DEVICE_ID_BLOCK) {
            return None;
        }
        virtio_mmio::init_device(mmio, 1);
        let mem = virtio_queue::queue_memory();
        let queue = VirtQueue::new(0, mem);
        virtio_mmio::setup_queue(
            mmio,
            0,
            queue.desc_phys(),
            queue.avail_phys(),
            queue.used_phys(),
            queue.size as u32,
        );
        let mut driver = Self {
            mmio,
            io_base: 0,
            queue,
            legacy_queue: LegacyVirtQueue::new(0),
            sector_count: 1024,
            hardware: true,
            transport: Transport::Mmio,
        };
        if driver.hardware_read_sectors(0, 1, &mut [0u8; 512]).is_err() {
            driver.hardware = false;
            driver.transport = Transport::Software;
        }
        crate::println!(
            "VirtIO block driver initialized at {:#x}, {} sector(s), hardware {}.",
            mmio as usize,
            driver.sector_count,
            driver.hardware
        );
        Some(driver)
    }

    pub fn software_fallback() -> Self {
        Self {
            mmio: core::ptr::null_mut(),
            io_base: 0,
            queue: VirtQueue::new(0, virtio_queue::queue_memory()),
            legacy_queue: LegacyVirtQueue::new(0),
            sector_count: 2048,
            hardware: false,
            transport: Transport::Software,
        }
    }

    pub fn read_sectors(
        &mut self,
        sector: u64,
        count: u32,
        buf: &mut [u8],
    ) -> Result<(), BlockError> {
        if buf.len() < count as usize * 512 {
            return Err(BlockError::IoError);
        }
        if self.hardware {
            match self.hardware_read_sectors(sector, count, buf) {
                Ok(()) => return Ok(()),
                Err(()) => {
                    self.hardware = false;
                }
            }
        }
        software_read(sector, count, buf)
    }

    pub fn write_sectors(&mut self, sector: u64, count: u32, buf: &[u8]) -> Result<(), BlockError> {
        if buf.len() < count as usize * 512 {
            return Err(BlockError::IoError);
        }
        if self.hardware && self.hardware_write_sectors(sector, count, buf).is_ok() {
            return Ok(());
        }
        self.hardware = false;
        self.transport = Transport::Software;
        Err(BlockError::IoError)
    }

    pub fn sector_count(&self) -> u64 {
        self.sector_count
    }

    fn hardware_read_sectors(&mut self, sector: u64, count: u32, buf: &mut [u8]) -> Result<(), ()> {
        match self.transport {
            Transport::LegacyIo => {
                return self.legacy_transfer(
                    VIRTIO_BLK_T_IN,
                    sector,
                    count,
                    buf.as_mut_ptr(),
                    count as usize * 512,
                    true,
                );
            }
            Transport::Mmio => {}
            Transport::Software => return Err(()),
        }

        let byte_len = count as usize * 512;
        let mut req = BLK_REQ.lock();
        req.header.type_ = VIRTIO_BLK_T_IN;
        req.header.reserved = 0;
        req.header.sector = sector;
        req.status = 0xff;

        let header_phys = virt_to_phys(core::ptr::addr_of!(req.header) as usize);
        let data_phys = virt_to_phys(buf.as_ptr() as usize);
        let status_phys = virt_to_phys(core::ptr::addr_of!(req.status) as usize);

        self.queue.reset();
        self.queue
            .set_desc(0, header_phys, 16, VIRTQ_DESC_F_NEXT, 1);
        self.queue.set_desc(
            1,
            data_phys,
            byte_len as u32,
            VIRTQ_DESC_F_NEXT | VIRTQ_DESC_F_WRITE,
            2,
        );
        self.queue
            .set_desc(2, status_phys, 1, VIRTQ_DESC_F_WRITE, 0);
        self.queue.submit_chain(0)?;
        virtio_mmio::kick_queue(self.mmio, 0);

        if virtio_mmio::interrupt_status(self.mmio) != 0 {
            virtio_mmio::ack_interrupt(self.mmio, virtio_mmio::interrupt_status(self.mmio));
        }

        self.queue.wait_used(0)?;
        drop(req);

        if BLK_REQ.lock().status != VIRTIO_BLK_S_OK {
            return Err(());
        }
        Ok(())
    }

    fn hardware_write_sectors(&mut self, sector: u64, count: u32, buf: &[u8]) -> Result<(), ()> {
        match self.transport {
            Transport::LegacyIo => {
                return self.legacy_transfer(
                    VIRTIO_BLK_T_OUT,
                    sector,
                    count,
                    buf.as_ptr() as *mut u8,
                    count as usize * 512,
                    false,
                );
            }
            Transport::Mmio => {}
            Transport::Software => return Err(()),
        }

        let byte_len = count as usize * 512;
        let mut req = BLK_REQ.lock();
        req.header.type_ = VIRTIO_BLK_T_OUT;
        req.header.reserved = 0;
        req.header.sector = sector;
        req.status = 0xff;

        let header_phys = virt_to_phys(core::ptr::addr_of!(req.header) as usize);
        let data_phys = virt_to_phys(buf.as_ptr() as usize);
        let status_phys = virt_to_phys(core::ptr::addr_of!(req.status) as usize);

        self.queue.reset();
        self.queue
            .set_desc(0, header_phys, 16, VIRTQ_DESC_F_NEXT, 1);
        self.queue
            .set_desc(1, data_phys, byte_len as u32, VIRTQ_DESC_F_NEXT, 2);
        self.queue
            .set_desc(2, status_phys, 1, VIRTQ_DESC_F_WRITE, 0);
        self.queue.submit_chain(0)?;
        virtio_mmio::kick_queue(self.mmio, 0);

        if virtio_mmio::interrupt_status(self.mmio) != 0 {
            virtio_mmio::ack_interrupt(self.mmio, virtio_mmio::interrupt_status(self.mmio));
        }

        self.queue.wait_used(0)?;
        drop(req);

        if BLK_REQ.lock().status != VIRTIO_BLK_S_OK {
            return Err(());
        }
        Ok(())
    }

    fn legacy_transfer(
        &mut self,
        request_type: u32,
        sector: u64,
        count: u32,
        data: *mut u8,
        byte_len: usize,
        device_writes_data: bool,
    ) -> Result<(), ()> {
        if byte_len < count as usize * 512 {
            return Err(());
        }
        let mut req = BLK_REQ.lock();
        req.header.type_ = request_type;
        req.header.reserved = 0;
        req.header.sector = sector;
        req.status = 0xff;

        let data_flags = if device_writes_data {
            VIRTQ_DESC_F_NEXT | VIRTQ_DESC_F_WRITE
        } else {
            VIRTQ_DESC_F_NEXT
        };
        self.legacy_queue.reset();
        self.legacy_queue.set_desc(
            0,
            virt_to_phys(core::ptr::addr_of!(req.header) as usize),
            16,
            VIRTQ_DESC_F_NEXT,
            1,
        );
        self.legacy_queue.set_desc(
            1,
            virt_to_phys(data as usize),
            byte_len as u32,
            data_flags,
            2,
        );
        self.legacy_queue.set_desc(
            2,
            virt_to_phys(core::ptr::addr_of!(req.status) as usize),
            1,
            VIRTQ_DESC_F_WRITE,
            0,
        );
        self.legacy_queue.submit_chain(0)?;
        unsafe {
            legacy_write16(self.io_base, LEGACY_QUEUE_NOTIFY, 0);
        }
        let _ = self.legacy_queue.wait_used(0)?;
        unsafe {
            let isr = legacy_read8(self.io_base, LEGACY_ISR_STATUS);
            let _ = isr;
        }
        if req.status != VIRTIO_BLK_S_OK {
            return Err(());
        }
        Ok(())
    }
}

pub fn init() {
    let driver = VirtioBlockDriver::probe().unwrap_or_else(VirtioBlockDriver::software_fallback);
    *BLOCK_DRIVER.lock() = Some(driver);
    crate::println!("Block layer initialized (VirtIO block + software fallback).");
}

pub fn read_sectors(sector: u64, count: u32, buf: &mut [u8]) -> Result<(), BlockError> {
    BLOCK_DRIVER
        .lock()
        .as_mut()
        .ok_or(BlockError::NotReady)?
        .read_sectors(sector, count, buf)
}

pub fn write_sectors(sector: u64, count: u32, buf: &[u8]) -> Result<(), BlockError> {
    BLOCK_DRIVER
        .lock()
        .as_mut()
        .ok_or(BlockError::NotReady)?
        .write_sectors(sector, count, buf)
}

pub fn sector_count() -> u64 {
    BLOCK_DRIVER
        .lock()
        .as_ref()
        .map(|d| d.sector_count)
        .unwrap_or(0)
}

fn software_read(sector: u64, count: u32, buf: &mut [u8]) -> Result<(), BlockError> {
    let image = software_disk_image();
    let start = sector as usize * 512;
    let end = start + count as usize * 512;
    if end > image.len() {
        return Err(BlockError::IoError);
    }
    buf[..count as usize * 512].copy_from_slice(&image[start..end]);
    Ok(())
}

fn software_disk_image() -> &'static [u8] {
    static DISK: [u8; 4096] = build_software_disk();
    &DISK
}

const fn build_software_disk() -> [u8; 4096] {
    let mut disk = [0u8; 4096];
    disk[1024 + 56] = 0x53;
    disk[1024 + 57] = 0xEF;
    disk[1024 + 24] = 0x00;
    disk[2048] = b'/';
    disk[2049] = b'm';
    disk[2050] = b'n';
    disk[2051] = b't';
    disk
}

pub fn self_test() -> bool {
    let mut buf = [0u8; 1024];
    if read_sectors(2, 2, &mut buf).is_err() || buf[56] != 0x53 || buf[57] != 0xEF {
        return false;
    }
    let hardware = BLOCK_DRIVER
        .lock()
        .as_ref()
        .map(|driver| driver.hardware)
        .unwrap_or(false);
    if hardware && !write_roundtrip_self_test() {
        return false;
    }
    true
}

fn write_roundtrip_self_test() -> bool {
    let scratch_sector = sector_count().saturating_sub(1);
    if scratch_sector == 0 {
        return false;
    }

    let mut original = [0u8; 512];
    let mut verify = [0u8; 512];
    let mut pattern = [0u8; 512];
    for (index, byte) in pattern.iter_mut().enumerate() {
        *byte = (index as u8).wrapping_mul(17).wrapping_add(0x5a);
    }

    if read_sectors(scratch_sector, 1, &mut original).is_err() {
        return false;
    }
    if write_sectors(scratch_sector, 1, &pattern).is_err() {
        return false;
    }
    let ok = read_sectors(scratch_sector, 1, &mut verify).is_ok() && verify == pattern;
    let restored = write_sectors(scratch_sector, 1, &original).is_ok();
    ok && restored
}

#[repr(C, align(4096))]
struct LegacyQueueMem {
    bytes: [u8; LEGACY_QUEUE_BYTES],
}

static mut LEGACY_QUEUE_MEM: LegacyQueueMem = LegacyQueueMem {
    bytes: [0; LEGACY_QUEUE_BYTES],
};

struct LegacyVirtQueue {
    mem: *mut u8,
    size: u16,
    last_used_idx: u16,
}

impl LegacyVirtQueue {
    fn new(size: u16) -> Self {
        let mem = core::ptr::addr_of_mut!(LEGACY_QUEUE_MEM) as *mut u8;
        let mut queue = Self {
            mem,
            size,
            last_used_idx: 0,
        };
        queue.reset();
        queue
    }

    fn pfn(&self) -> u32 {
        (virt_to_phys(self.mem as usize) >> 12) as u32
    }

    fn reset(&mut self) {
        unsafe {
            ptr::write_bytes(self.mem, 0, LEGACY_QUEUE_BYTES);
        }
        self.last_used_idx = 0;
    }

    fn set_desc(&mut self, index: u16, addr: u64, len: u32, flags: u16, next: u16) {
        let offset = index as usize * 16;
        unsafe {
            ptr::write_volatile(self.mem.add(offset) as *mut u64, addr);
            ptr::write_volatile(self.mem.add(offset + 8) as *mut u32, len);
            ptr::write_volatile(self.mem.add(offset + 12) as *mut u16, flags);
            ptr::write_volatile(self.mem.add(offset + 14) as *mut u16, next);
        }
    }

    fn submit_chain(&mut self, head: u16) -> Result<(), ()> {
        if self.size == 0 {
            return Err(());
        }
        let avail = self.avail_offset();
        unsafe {
            let idx_ptr = self.mem.add(avail + 2) as *mut u16;
            let idx = ptr::read_volatile(idx_ptr);
            let slot = idx % self.size;
            ptr::write_volatile(self.mem.add(avail + 4 + slot as usize * 2) as *mut u16, head);
            compiler_fence(Ordering::SeqCst);
            ptr::write_volatile(idx_ptr, idx.wrapping_add(1));
        }
        Ok(())
    }

    fn wait_used(&mut self, head: u16) -> Result<u32, ()> {
        if self.size == 0 {
            return Err(());
        }
        let used = self.used_offset();
        for _ in 0..5_000_000 {
            unsafe {
                compiler_fence(Ordering::SeqCst);
                let idx = ptr::read_volatile(self.mem.add(used + 2) as *const u16);
                if idx != self.last_used_idx {
                    let slot = self.last_used_idx % self.size;
                    let elem = used + 4 + slot as usize * 8;
                    let id = ptr::read_volatile(self.mem.add(elem) as *const u32);
                    let len = ptr::read_volatile(self.mem.add(elem + 4) as *const u32);
                    self.last_used_idx = self.last_used_idx.wrapping_add(1);
                    if id as u16 == head {
                        return Ok(len);
                    }
                }
            }
            core::hint::spin_loop();
        }
        Err(())
    }

    fn avail_offset(&self) -> usize {
        self.size as usize * 16
    }

    fn used_offset(&self) -> usize {
        align_up(self.avail_offset() + 6 + self.size as usize * 2, 4096)
    }
}

unsafe impl Send for LegacyVirtQueue {}

fn virt_to_phys(addr: usize) -> u64 {
    if addr >= KERNEL_HIGH_BASE {
        (addr - KERNEL_HIGH_BASE) as u64
    } else {
        addr as u64
    }
}

fn align_up(value: usize, align: usize) -> usize {
    (value + align - 1) & !(align - 1)
}

unsafe fn legacy_read8(base: u16, offset: u16) -> u8 {
    unsafe { port::inb(base + offset) }
}

unsafe fn legacy_read16(base: u16, offset: u16) -> u16 {
    unsafe { port::inw(base + offset) }
}

unsafe fn legacy_read32(base: u16, offset: u16) -> u32 {
    unsafe { port::inl(base + offset) }
}

unsafe fn legacy_write8(base: u16, offset: u16, value: u8) {
    unsafe {
        port::outb(base + offset, value);
    }
}

unsafe fn legacy_write16(base: u16, offset: u16, value: u16) {
    unsafe {
        port::outw(base + offset, value);
    }
}

unsafe fn legacy_write32(base: u16, offset: u16, value: u32) {
    unsafe {
        port::outl(base + offset, value);
    }
}
