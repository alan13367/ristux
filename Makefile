CARGO ?= cargo
RUSTC ?= rustc
CLANG ?= clang
GRUB_FILE ?= $(shell command -v grub-file 2>/dev/null || command -v i686-elf-grub-file 2>/dev/null || command -v x86_64-elf-grub-file 2>/dev/null || printf grub-file)
GRUB_MKRESCUE ?= $(shell command -v grub-mkrescue 2>/dev/null || command -v i686-elf-grub-mkrescue 2>/dev/null || command -v x86_64-elf-grub-mkrescue 2>/dev/null || printf grub-mkrescue)
QEMU ?= qemu-system-x86_64
QEMU_FLAGS ?= -m 256M -smp 4
RUST_HOST := $(shell $(RUSTC) -vV | sed -n 's/^host: //p')
RUST_LLD ?= $(shell $(RUSTC) --print sysroot)/lib/rustlib/$(RUST_HOST)/bin/rust-lld

TARGET := x86_64-ristux-kernel
KERNEL_NAME := ristux-kernel
KERNEL_ELF := target/$(TARGET)/release/$(KERNEL_NAME)
ISO_DIR := iso
ISO_KERNEL := $(ISO_DIR)/boot/ristux.elf
ISO_INITRD := $(ISO_DIR)/boot/initrd.bin
ISO_IMAGE := build/ristux.iso
DISK_IMAGE := build/disk.img
USERLAND_RS_TARGET := x86_64-ristux-user
USERLAND_RS_OUT := userland/target/$(USERLAND_RS_TARGET)/release
USERLAND_RS_SRC := \
	userland/Cargo.toml \
	userland/linker.ld \
	$(wildcard userland/src/*.rs) \
	$(wildcard userland/src/bin/*.rs) \
	targets/x86_64-ristux-user.json
USERLAND_RS_BINS := init sh cat echo true false touch mount login id su sleep ping curl_lite sig_demo
USERLAND_RS_STAMP := build/userland/.rust-stamp
USER_INIT_ELF := build/userland/init.elf
USER_SH_ELF := build/userland/sh.elf
USER_CAT_ELF := build/userland/cat.elf
USER_ECHO_ELF := build/userland/echo.elf
USER_TRUE_ELF := build/userland/true.elf
USER_FALSE_ELF := build/userland/false.elf
USER_TOUCH_ELF := build/userland/touch.elf
USER_MOUNT_ELF := build/userland/mount.elf
USER_LOGIN_ELF := build/userland/login.elf
USER_ID_ELF := build/userland/id.elf
USER_SU_ELF := build/userland/su.elf
USER_SLEEP_ELF := build/userland/sleep.elf
USER_PING_ELF := build/userland/ping.elf
USER_CURL_LITE_ELF := build/userland/curl_lite.elf
USER_SIG_DEMO_ELF := build/userland/sig_demo.elf
USER_LS_OBJ := build/userland/ls.o
USER_LS_ELF := build/userland/ls.elf
USER_PWD_OBJ := build/userland/pwd.o
USER_PWD_ELF := build/userland/pwd.elf
USER_CHMOD_OBJ := build/userland/chmod.o
USER_CHMOD_ELF := build/userland/chmod.elf
USER_KILL_OBJ := build/userland/kill.o
USER_KILL_ELF := build/userland/kill.elf
USER_MKDIR_OBJ := build/userland/mkdir.o
USER_MKDIR_ELF := build/userland/mkdir.elf
USER_RM_OBJ := build/userland/rm.o
USER_RM_ELF := build/userland/rm.elf
USER_UDP_OBJ := build/userland/udp.o
USER_UDP_ELF := build/userland/udp.elf
USER_LIBC_OBJ := build/userland/libc.o
USER_LIBC_SO := build/userland/libc.so
ROOTFS_BUILDER := build/build_rootfs
EXT2_DISK_BUILDER := build/build_ext2_disk
ROOTFS_MANIFEST := rootfs/manifest.txt
ROOTFS_INPUTS := $(ROOTFS_MANIFEST) rootfs/etc/os-release

.PHONY: all build rootfs disk check-multiboot iso run run-headless smoke debug test clean

all: build

build:
	$(CARGO) build --release

$(ISO_KERNEL): build
	cp $(KERNEL_ELF) $(ISO_KERNEL)

$(USERLAND_RS_STAMP): $(USERLAND_RS_SRC)
	mkdir -p build/userland
	cd userland && $(CARGO) build --release
	@for bin in $(USERLAND_RS_BINS); do \
		cp $(USERLAND_RS_OUT)/$$bin build/userland/$$bin.elf; \
	done
	touch $@

$(USER_INIT_ELF): $(USERLAND_RS_STAMP)
$(USER_SH_ELF): $(USERLAND_RS_STAMP)
$(USER_CAT_ELF): $(USERLAND_RS_STAMP)
$(USER_ECHO_ELF): $(USERLAND_RS_STAMP)
$(USER_TRUE_ELF): $(USERLAND_RS_STAMP)
$(USER_FALSE_ELF): $(USERLAND_RS_STAMP)
$(USER_TOUCH_ELF): $(USERLAND_RS_STAMP)
$(USER_MOUNT_ELF): $(USERLAND_RS_STAMP)
$(USER_LOGIN_ELF): $(USERLAND_RS_STAMP)
$(USER_ID_ELF): $(USERLAND_RS_STAMP)
$(USER_SU_ELF): $(USERLAND_RS_STAMP)
$(USER_SLEEP_ELF): $(USERLAND_RS_STAMP)
$(USER_PING_ELF): $(USERLAND_RS_STAMP)
$(USER_CURL_LITE_ELF): $(USERLAND_RS_STAMP)
$(USER_SIG_DEMO_ELF): $(USERLAND_RS_STAMP)

$(USER_LS_OBJ): userland/ls.S
	mkdir -p build/userland
	$(CLANG) --target=x86_64-unknown-none-elf -x assembler -c $< -o $@

$(USER_LS_ELF): $(USER_LS_OBJ) userland/linker.ld
	$(RUST_LLD) -flavor gnu -T userland/linker.ld -o $@ $(USER_LS_OBJ)

$(USER_PWD_OBJ): userland/pwd.S
	mkdir -p build/userland
	$(CLANG) --target=x86_64-unknown-none-elf -x assembler -c $< -o $@

$(USER_PWD_ELF): $(USER_PWD_OBJ) userland/linker.ld
	$(RUST_LLD) -flavor gnu -T userland/linker.ld -o $@ $(USER_PWD_OBJ)

$(USER_CHMOD_OBJ): userland/chmod.S
	mkdir -p build/userland
	$(CLANG) --target=x86_64-unknown-none-elf -x assembler -c $< -o $@

$(USER_CHMOD_ELF): $(USER_CHMOD_OBJ) userland/linker.ld
	$(RUST_LLD) -flavor gnu -T userland/linker.ld -o $@ $(USER_CHMOD_OBJ)

$(USER_KILL_OBJ): userland/kill.S
	mkdir -p build/userland
	$(CLANG) --target=x86_64-unknown-none-elf -x assembler -c $< -o $@

$(USER_KILL_ELF): $(USER_KILL_OBJ) userland/linker.ld
	$(RUST_LLD) -flavor gnu -T userland/linker.ld -o $@ $(USER_KILL_OBJ)

$(USER_MKDIR_OBJ): userland/mkdir.S
	mkdir -p build/userland
	$(CLANG) --target=x86_64-unknown-none-elf -x assembler -c $< -o $@

$(USER_MKDIR_ELF): $(USER_MKDIR_OBJ) userland/linker.ld
	$(RUST_LLD) -flavor gnu -T userland/linker.ld -o $@ $(USER_MKDIR_OBJ)

$(USER_RM_OBJ): userland/rm.S
	mkdir -p build/userland
	$(CLANG) --target=x86_64-unknown-none-elf -x assembler -c $< -o $@

$(USER_RM_ELF): $(USER_RM_OBJ) userland/linker.ld
	$(RUST_LLD) -flavor gnu -T userland/linker.ld -o $@ $(USER_RM_OBJ)

$(USER_UDP_OBJ): userland/udp.S
	mkdir -p build/userland
	$(CLANG) --target=x86_64-unknown-none-elf -x assembler -c $< -o $@

$(USER_UDP_ELF): $(USER_UDP_OBJ) userland/linker.ld
	$(RUST_LLD) -flavor gnu -T userland/linker.ld -o $@ $(USER_UDP_OBJ)

$(USER_LIBC_OBJ): userland/libc.S
	mkdir -p build/userland
	$(CLANG) --target=x86_64-unknown-none-elf -x assembler -c $< -o $@

$(USER_LIBC_SO): $(USER_LIBC_OBJ)
	$(RUST_LLD) -flavor gnu -shared -o $@ $(USER_LIBC_OBJ)

$(ROOTFS_BUILDER): tools/build_rootfs.rs
	mkdir -p build
	$(RUSTC) $< -o $@

$(EXT2_DISK_BUILDER): tools/build_ext2_disk.rs
	mkdir -p build
	$(RUSTC) $< -o $@

$(ISO_INITRD): $(USER_INIT_ELF) $(USER_SH_ELF) $(USER_CAT_ELF) $(USER_ECHO_ELF) $(USER_TRUE_ELF) $(USER_FALSE_ELF) $(USER_TOUCH_ELF) $(USER_MOUNT_ELF) $(USER_LOGIN_ELF) $(USER_ID_ELF) $(USER_SU_ELF) $(USER_SLEEP_ELF) $(USER_PING_ELF) $(USER_CURL_LITE_ELF) $(USER_SIG_DEMO_ELF) $(USER_LS_ELF) $(USER_PWD_ELF) $(USER_CHMOD_ELF) $(USER_KILL_ELF) $(USER_MKDIR_ELF) $(USER_RM_ELF) $(USER_UDP_ELF) $(USER_LIBC_SO) $(ROOTFS_BUILDER) $(ROOTFS_INPUTS)
	$(ROOTFS_BUILDER) $(ISO_INITRD) $(ROOTFS_MANIFEST)

rootfs: $(ISO_INITRD)

$(DISK_IMAGE): $(ISO_INITRD) $(EXT2_DISK_BUILDER) $(ROOTFS_MANIFEST) $(ROOTFS_INPUTS)
	$(EXT2_DISK_BUILDER) $(DISK_IMAGE) $(ROOTFS_MANIFEST)

disk: $(DISK_IMAGE)

check-multiboot: $(ISO_KERNEL)
	$(GRUB_FILE) --is-x86-multiboot2 $(ISO_KERNEL)

iso: check-multiboot $(ISO_INITRD) $(DISK_IMAGE)
	mkdir -p build
	$(GRUB_MKRESCUE) -o $(ISO_IMAGE) $(ISO_DIR)

run: iso disk
	$(QEMU) -cdrom $(ISO_IMAGE) $(QEMU_FLAGS) -drive file=$(DISK_IMAGE),if=none,id=hd0,format=raw -device virtio-blk-pci,drive=hd0 -no-reboot -no-shutdown

run-headless: iso
	scripts/run_qemu.sh --headless

smoke:
	scripts/smoke_test.sh

debug: iso
	scripts/debug_qemu.sh

test: smoke

clean:
	$(CARGO) clean
	$(CARGO) clean --manifest-path userland/Cargo.toml
	rm -rf build
	rm -f $(ISO_KERNEL)
	rm -f $(ISO_INITRD)
