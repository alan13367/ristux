pub mod ramdisk;

pub fn init() {
    crate::drivers::virtio_blk::init();
    ramdisk::init();
    ramdisk::self_test();
    mount_hybrid_ext2();
}

fn mount_hybrid_ext2() {
    if crate::drivers::virtio_blk::self_test() {
        crate::println!("VirtIO block self-test passed (ext2 magic 0xEF53).");
    }
    if crate::fs::ext2::self_test().is_ok() {
        crate::println!("Ext2 parser self-test passed.");
    }
    crate::fs::mount_hybrid_ext2();
}

pub fn stats() -> ramdisk::StorageStats {
    ramdisk::stats()
}
