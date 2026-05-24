pub mod ramdisk;

pub fn init() {
    ramdisk::init();
    ramdisk::self_test();
}

pub fn stats() -> ramdisk::StorageStats {
    ramdisk::stats()
}

