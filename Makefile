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
USER_INIT_OBJ := build/userland/init.o
USER_INIT_ELF := build/userland/init.elf
USER_CAT_OBJ := build/userland/cat.o
USER_CAT_ELF := build/userland/cat.elf
USER_ECHO_OBJ := build/userland/echo.o
USER_ECHO_ELF := build/userland/echo.elf
USER_TRUE_OBJ := build/userland/true.o
USER_TRUE_ELF := build/userland/true.elf
USER_FALSE_OBJ := build/userland/false.o
USER_FALSE_ELF := build/userland/false.elf
USER_LS_OBJ := build/userland/ls.o
USER_LS_ELF := build/userland/ls.elf
USER_PWD_OBJ := build/userland/pwd.o
USER_PWD_ELF := build/userland/pwd.elf
USER_CHMOD_OBJ := build/userland/chmod.o
USER_CHMOD_ELF := build/userland/chmod.elf
USER_KILL_OBJ := build/userland/kill.o
USER_KILL_ELF := build/userland/kill.elf
USER_TOUCH_OBJ := build/userland/touch.o
USER_TOUCH_ELF := build/userland/touch.elf
USER_MKDIR_OBJ := build/userland/mkdir.o
USER_MKDIR_ELF := build/userland/mkdir.elf
USER_RM_OBJ := build/userland/rm.o
USER_RM_ELF := build/userland/rm.elf
USER_UDP_OBJ := build/userland/udp.o
USER_UDP_ELF := build/userland/udp.elf
USER_LIBC_OBJ := build/userland/libc.o
USER_LIBC_SO := build/userland/libc.so
ROOTFS_BUILDER := build/build_rootfs
ROOTFS_MANIFEST := rootfs/manifest.txt
ROOTFS_INPUTS := $(ROOTFS_MANIFEST) rootfs/etc/os-release

.PHONY: all build rootfs check-multiboot iso run run-headless smoke debug test clean

all: build

build:
	$(CARGO) build --release

$(ISO_KERNEL): build
	cp $(KERNEL_ELF) $(ISO_KERNEL)

$(USER_INIT_OBJ): userland/init.S
	mkdir -p build/userland
	$(CLANG) --target=x86_64-unknown-none-elf -x assembler -c $< -o $@

$(USER_INIT_ELF): $(USER_INIT_OBJ) userland/linker.ld
	$(RUST_LLD) -flavor gnu -T userland/linker.ld -o $@ $(USER_INIT_OBJ)

$(USER_CAT_OBJ): userland/cat.S
	mkdir -p build/userland
	$(CLANG) --target=x86_64-unknown-none-elf -x assembler -c $< -o $@

$(USER_CAT_ELF): $(USER_CAT_OBJ) userland/linker.ld
	$(RUST_LLD) -flavor gnu -T userland/linker.ld -o $@ $(USER_CAT_OBJ)

$(USER_ECHO_OBJ): userland/echo.S
	mkdir -p build/userland
	$(CLANG) --target=x86_64-unknown-none-elf -x assembler -c $< -o $@

$(USER_ECHO_ELF): $(USER_ECHO_OBJ) userland/linker.ld
	$(RUST_LLD) -flavor gnu -T userland/linker.ld -o $@ $(USER_ECHO_OBJ)

$(USER_TRUE_OBJ): userland/true.S
	mkdir -p build/userland
	$(CLANG) --target=x86_64-unknown-none-elf -x assembler -c $< -o $@

$(USER_TRUE_ELF): $(USER_TRUE_OBJ) userland/linker.ld
	$(RUST_LLD) -flavor gnu -T userland/linker.ld -o $@ $(USER_TRUE_OBJ)

$(USER_FALSE_OBJ): userland/false.S
	mkdir -p build/userland
	$(CLANG) --target=x86_64-unknown-none-elf -x assembler -c $< -o $@

$(USER_FALSE_ELF): $(USER_FALSE_OBJ) userland/linker.ld
	$(RUST_LLD) -flavor gnu -T userland/linker.ld -o $@ $(USER_FALSE_OBJ)

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

$(USER_TOUCH_OBJ): userland/touch.S
	mkdir -p build/userland
	$(CLANG) --target=x86_64-unknown-none-elf -x assembler -c $< -o $@

$(USER_TOUCH_ELF): $(USER_TOUCH_OBJ) userland/linker.ld
	$(RUST_LLD) -flavor gnu -T userland/linker.ld -o $@ $(USER_TOUCH_OBJ)

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

$(ISO_INITRD): $(USER_INIT_ELF) $(USER_CAT_ELF) $(USER_ECHO_ELF) $(USER_TRUE_ELF) $(USER_FALSE_ELF) $(USER_LS_ELF) $(USER_PWD_ELF) $(USER_CHMOD_ELF) $(USER_KILL_ELF) $(USER_TOUCH_ELF) $(USER_MKDIR_ELF) $(USER_RM_ELF) $(USER_UDP_ELF) $(USER_LIBC_SO) $(ROOTFS_BUILDER) $(ROOTFS_INPUTS)
	$(ROOTFS_BUILDER) $(ISO_INITRD) $(ROOTFS_MANIFEST)

rootfs: $(ISO_INITRD)

check-multiboot: $(ISO_KERNEL)
	$(GRUB_FILE) --is-x86-multiboot2 $(ISO_KERNEL)

iso: check-multiboot $(ISO_INITRD)
	mkdir -p build
	$(GRUB_MKRESCUE) -o $(ISO_IMAGE) $(ISO_DIR)

run: iso
	$(QEMU) -cdrom $(ISO_IMAGE) $(QEMU_FLAGS) -no-reboot -no-shutdown

run-headless: iso
	scripts/run_qemu.sh --headless

smoke:
	scripts/smoke_test.sh

debug: iso
	scripts/debug_qemu.sh

test: smoke

clean:
	$(CARGO) clean
	rm -rf build
	rm -f $(ISO_KERNEL)
	rm -f $(ISO_INITRD)
