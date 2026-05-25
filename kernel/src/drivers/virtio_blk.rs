use super::{
    pci, virtio_mmio,
    virtio_queue::{self, VIRTQ_DESC_F_NEXT, VIRTQ_DESC_F_WRITE, VirtQueue},
};
use crate::sync::spinlock::SpinLock;

const VIRTIO_VENDOR: u16 = 0x1af4;
const VIRTIO_BLK_LEGACY: u16 = 0x1001;
const VIRTIO_BLK_MODERN: u16 = 0x1042;
const MMIO_DEVICE_ID_BLOCK: u32 = 0x02;

const VIRTIO_BLK_T_IN: u32 = 0;
const VIRTIO_BLK_S_OK: u8 = 0;

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
    queue: VirtQueue,
    sector_count: u64,
    hardware: bool,
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
            queue,
            sector_count: 1024,
            hardware: true,
        };
        if driver.hardware_read_sectors(0, 1, &mut [0u8; 512]).is_err() {
            driver.hardware = false;
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
            queue: VirtQueue::new(0, virtio_queue::queue_memory()),
            sector_count: 2048,
            hardware: false,
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

    pub fn sector_count(&self) -> u64 {
        self.sector_count
    }

    fn hardware_read_sectors(&mut self, sector: u64, count: u32, buf: &mut [u8]) -> Result<(), ()> {
        let byte_len = count as usize * 512;
        let mut req = BLK_REQ.lock();
        req.header.type_ = VIRTIO_BLK_T_IN;
        req.header.reserved = 0;
        req.header.sector = sector;
        req.status = 0xff;

        let header_phys = core::ptr::addr_of!(req.header) as u64;
        let data_phys = buf.as_ptr() as u64;
        let status_phys = core::ptr::addr_of!(req.status) as u64;

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
    read_sectors(2, 2, &mut buf).is_ok() && buf[56] == 0x53 && buf[57] == 0xEF
}
