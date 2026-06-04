CARGO ?= cargo
RUSTC ?= rustc
HOST_TOOL_RUSTFLAGS ?= -O
GRUB_FILE ?= $(shell command -v grub-file 2>/dev/null || command -v i686-elf-grub-file 2>/dev/null || command -v x86_64-elf-grub-file 2>/dev/null || printf grub-file)
GRUB_MKRESCUE ?= $(shell command -v grub-mkrescue 2>/dev/null || command -v i686-elf-grub-mkrescue 2>/dev/null || command -v x86_64-elf-grub-mkrescue 2>/dev/null || printf grub-mkrescue)
GRUB_MKIMAGE ?= $(shell command -v grub-mkimage 2>/dev/null || command -v i686-elf-grub-mkimage 2>/dev/null || command -v x86_64-elf-grub-mkimage 2>/dev/null || printf grub-mkimage)
GRUB_BIOS_DIR ?= $(shell for d in /usr/lib/grub/i386-pc /usr/local/lib/grub/i386-pc /opt/homebrew/lib/grub/i386-pc $$(find /opt/homebrew/Cellar -path '*/lib/*/grub/i386-pc' -type d 2>/dev/null | sort -r); do if test -f "$$d/boot.img"; then printf '%s' "$$d"; break; fi; done)
QEMU_IMG ?= qemu-img
QEMU ?= qemu-system-x86_64
QEMU_FLAGS ?= -m 1024M -smp 4
QEMU_DISPLAY ?= $(shell if $(QEMU) -display help 2>/dev/null | grep -qx cocoa; then printf '%s' '-display cocoa,zoom-to-fit=on'; fi)
QEMU_KEYMAP ?= es
QEMU_WINDOW_BOUNDS ?= 80,80,1360,820
QEMU_WINDOW_TITLE ?= Ristux

TARGET := x86_64-ristux-kernel
KERNEL_NAME := ristux-kernel
KERNEL_ELF := target/$(TARGET)/release/$(KERNEL_NAME)
ISO_DIR := iso
ISO_KERNEL := $(ISO_DIR)/boot/ristux.elf
ISO_INITRD := $(ISO_DIR)/boot/initrd.bin
ISO_IMAGE := build/ristux.iso
DISK_IMAGE := build/disk.img
INSTALLER_ISO_DIR := build/installer-iso
INSTALLER_INITRD := build/installer/initrd.bin
INSTALLER_ISO_IMAGE := build/ristux-installer.iso
INSTALLED_GRUB_CFG := build/install/grub.cfg
INSTALLER_GRUB_CFG := build/installer/grub.cfg
GRUB_BOOT_IMG := build/grub/boot.img
GRUB_CORE_IMG := build/grub/core.img
GRUB_EMBEDDED_CFG := build/grub/embedded.cfg
VM_DISK_SIZE ?= 4294967296
VM_BLANK_IMAGE := build/ristux-blank.raw
VM_IMAGE := build/ristux-vm.raw
VM_QCOW2_IMAGE := build/ristux-vm.qcow2
USERLAND_RS_TARGET := x86_64-unknown-ristux
USERLAND_RS_OUT := userland/target/$(USERLAND_RS_TARGET)/release
USERLAND_RS_SRC := \
	Makefile \
	userland/Cargo.toml \
	userland/.cargo/config.toml \
	userland/linker.ld \
	$(wildcard userland/src/*.rs) \
	$(wildcard userland/src/bin/*.rs) \
	$(wildcard userland/src/bin/probes/*.rs) \
	targets/x86_64-unknown-ristux.json
USERLAND_RS_BINS := init sh cat echo true false touch mount login ristux_install fdisk mkfs_ext2 id su sleep shutdown ping ip curl_lite loopback_check ssh_banner pty_shell_check sig_demo edit ansi_demo tar pkg ar pkgconf make toolchain rustc cargo rustdoc ristux_ld rust_host_probe stty cp mv ls mkdir rm chmod kill pwd udp grep printf test ln readlink wc head tail tee sort uniq basename dirname install env cut find xargs sed uname hostname tr date which cmp dd df seq expr yes diff awk patch gzip xz stat chown uptime free ps cc_hello cc_cred cc_passwd cc_session cc_dev cc_dns cc_http cc_fcntl cc_file_sync cc_futex cc_cow cc_mmap cc_path cc_poll cc_select cc_socket cc_tcp cc_uio cc_stack cc_tty cc_pty cc_fs cc_signal cc_links cc_libc_compat cc_ext2 cc_proc cc_procfs cc_statfs cc_sse cc_libc_hosted
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
USER_RISTUX_INSTALL_ELF := build/userland/ristux_install.elf
USER_FDISK_ELF := build/userland/fdisk.elf
USER_MKFS_EXT2_ELF := build/userland/mkfs_ext2.elf
USER_ID_ELF := build/userland/id.elf
USER_SU_ELF := build/userland/su.elf
USER_SLEEP_ELF := build/userland/sleep.elf
USER_SHUTDOWN_ELF := build/userland/shutdown.elf
USER_STTY_ELF := build/userland/stty.elf
USER_PING_ELF := build/userland/ping.elf
USER_CURL_LITE_ELF := build/userland/curl_lite.elf
USER_LOOPBACK_CHECK_ELF := build/userland/loopback_check.elf
USER_SSH_BANNER_ELF := build/userland/ssh_banner.elf
USER_PTY_SHELL_CHECK_ELF := build/userland/pty_shell_check.elf
USER_SIG_DEMO_ELF := build/userland/sig_demo.elf
USER_EDIT_ELF := build/userland/edit.elf
USER_ANSI_DEMO_ELF := build/userland/ansi_demo.elf
USER_TAR_ELF := build/userland/tar.elf
USER_PKG_ELF := build/userland/pkg.elf
USER_AR_ELF := build/userland/ar.elf
USER_PKGCONF_ELF := build/userland/pkgconf.elf
USER_MAKE_ELF := build/userland/make.elf
USER_TOOLCHAIN_ELF := build/userland/toolchain.elf
USER_CP_ELF := build/userland/cp.elf
USER_MV_ELF := build/userland/mv.elf
USER_GREP_ELF := build/userland/grep.elf
USER_PRINTF_ELF := build/userland/printf.elf
USER_TEST_ELF := build/userland/test.elf
USER_LN_ELF := build/userland/ln.elf
USER_READLINK_ELF := build/userland/readlink.elf
USER_WC_ELF := build/userland/wc.elf
USER_HEAD_ELF := build/userland/head.elf
USER_TAIL_ELF := build/userland/tail.elf
USER_TEE_ELF := build/userland/tee.elf
USER_SORT_ELF := build/userland/sort.elf
USER_UNIQ_ELF := build/userland/uniq.elf
USER_BASENAME_ELF := build/userland/basename.elf
USER_DIRNAME_ELF := build/userland/dirname.elf
USER_INSTALL_ELF := build/userland/install.elf
USER_ENV_ELF := build/userland/env.elf
USER_CUT_ELF := build/userland/cut.elf
USER_FIND_ELF := build/userland/find.elf
USER_XARGS_ELF := build/userland/xargs.elf
USER_SED_ELF := build/userland/sed.elf
USER_UNAME_ELF := build/userland/uname.elf
USER_TR_ELF := build/userland/tr.elf
USER_DATE_ELF := build/userland/date.elf
USER_WHICH_ELF := build/userland/which.elf
USER_CMP_ELF := build/userland/cmp.elf
USER_DD_ELF := build/userland/dd.elf
USER_SEQ_ELF := build/userland/seq.elf
USER_EXPR_ELF := build/userland/expr.elf
USER_YES_ELF := build/userland/yes.elf
USER_DIFF_ELF := build/userland/diff.elf
USER_AWK_ELF := build/userland/awk.elf
USER_PATCH_ELF := build/userland/patch.elf
USER_GZIP_ELF := build/userland/gzip.elf
USER_XZ_ELF := build/userland/xz.elf
USER_STAT_ELF := build/userland/stat.elf
USER_LS_ELF := build/userland/ls.elf
USER_PWD_ELF := build/userland/pwd.elf
USER_CHMOD_ELF := build/userland/chmod.elf
USER_KILL_ELF := build/userland/kill.elf
USER_MKDIR_ELF := build/userland/mkdir.elf
USER_RM_ELF := build/userland/rm.elf
USER_UDP_ELF := build/userland/udp.elf
ROOTFS_BUILDER := build/build_rootfs
EXT2_DISK_BUILDER := build/build_ext2_disk
VM_DISK_BUILDER := build/build_vm_disk
PACKAGE_TAR_BUILDER := build/build_package_tar
ROOTFS_MANIFEST := rootfs/manifest.txt
INSTALLER_ROOTFS_MANIFEST := rootfs/installer-manifest.txt
ROOTFS_BASE_PACKAGE_DIR := rootfs/packages/base-files
ROOTFS_BASE_PACKAGE_INPUTS := $(shell find $(ROOTFS_BASE_PACKAGE_DIR) -type f 2>/dev/null | sort)
ROOTFS_BASE_PACKAGE_TAR := build/packages/base-files.tar
ROOTFS_BASE_PACKAGE_ARCHIVE := build/packages/base-files.tar.gz
ROOTFS_GZIP_TESTDATA_SOURCE := rootfs/testdata/gzip-dynamic.txt
ROOTFS_GZIP_TESTDATA_ARCHIVE := build/testdata/gzip-dynamic.txt.gz
ROOTFS_SOURCEPKG_DIR := rootfs/testdata/sourcepkg
ROOTFS_SOURCEPKG_INPUTS := $(shell find $(ROOTFS_SOURCEPKG_DIR) -type f 2>/dev/null | sort)
ROOTFS_SOURCEPKG_TAR := build/testdata/ristuxpkg-0.1.tar
ROOTFS_SOURCEPKG_ARCHIVE := build/testdata/ristuxpkg-0.1.tar.gz
RUST_SYSROOT_TREE := build/rustlib
RUST_SYSROOT_LIBDIR := $(RUST_SYSROOT_TREE)/x86_64-unknown-ristux/lib
RUST_SYSROOT_STAMP := build/rustlib.stamp
RUST_STD_PROBE_ELF := build/userland/rust_std_probe.elf
RUST_STD_SYSROOT_TREE := build/rust-std-sysroot
RUST_STD_SYSROOT_STAMP := build/rust-std-sysroot.stamp
RUST_OFFICIAL_RUSTC := build/official-rust/bin/rustc
RUST_OVERLAY_TREE := toolchain/rust-overlays/rust-1.96.0
RUST_OVERLAY_INPUTS := $(shell find $(RUST_OVERLAY_TREE) -type f 2>/dev/null)
ROOTFS_INPUTS := $(ROOTFS_MANIFEST) rootfs/etc/os-release rootfs/etc/resolv.conf rootfs/usr/lib/pkgconfig/ristux.pc rootfs/usr/lib/rustlib/rust-1.96.0-manifest.toml rootfs/testdata/ristuxpkg.patch targets/x86_64-unknown-ristux.json $(ROOTFS_BASE_PACKAGE_ARCHIVE) $(ROOTFS_GZIP_TESTDATA_ARCHIVE) $(ROOTFS_SOURCEPKG_ARCHIVE) $(RUST_OFFICIAL_RUSTC) $(RUST_STD_PROBE_ELF) $(RUST_STD_SYSROOT_STAMP) $(RUST_OVERLAY_INPUTS)

.PHONY: all build rootfs disk check-multiboot iso installer-iso vm-blank vm-image vm-qcow2 run run-headless run-ssh smoke quick quick-% rust-std-probe rust-std-probe-current rust-std-probe-current-blocker rust-std-probe-binary rust-std-sysroot rust-official-rustc rust-official-target-probe rust-official-bootstrap-std rust-official-bootstrap-stage2 rust-official-std-probe debug test clean

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

$(RUST_SYSROOT_STAMP): $(USERLAND_RS_STAMP)
	rm -rf $(RUST_SYSROOT_TREE)
	mkdir -p $(RUST_SYSROOT_LIBDIR)
	@for crate in core alloc compiler_builtins; do \
		rlib=$$(ls -t $(USERLAND_RS_OUT)/deps/lib$$crate-*.rlib 2>/dev/null | head -n 1); \
		test -n "$$rlib" || { echo "missing Rust sysroot artifact for $$crate" >&2; exit 1; }; \
		cp "$$rlib" "$(RUST_SYSROOT_LIBDIR)/"; \
		rmeta="$${rlib%.rlib}.rmeta"; \
		test -f "$$rmeta" && cp "$$rmeta" "$(RUST_SYSROOT_LIBDIR)/" || true; \
	done
	touch $@

$(RUST_STD_SYSROOT_STAMP): scripts/probe_rust_std.sh targets/x86_64-unknown-ristux.json userland/src/bin/ristux_ld.rs $(RUST_OVERLAY_INPUTS)
	RISTUX_STD_PROBE_OUTPUT=$(RUST_STD_PROBE_ELF) RISTUX_STD_SYSROOT_OUTPUT=$(RUST_STD_SYSROOT_TREE) scripts/probe_rust_std.sh --expect-std-link-success
	touch $@

$(RUST_STD_PROBE_ELF): $(RUST_STD_SYSROOT_STAMP)
	@test -f $@

$(RUST_OFFICIAL_RUSTC): scripts/probe_rust_bootstrap_stage2.sh scripts/probe_rust_target.sh userland/src/bin/ristux_ld.rs $(RUST_OVERLAY_INPUTS)
	RISTUX_RUSTC_OUTPUT=$@ scripts/probe_rust_bootstrap_stage2.sh

rust-official-rustc: $(RUST_OFFICIAL_RUSTC)

$(USER_INIT_ELF): $(USERLAND_RS_STAMP)
$(USER_SH_ELF): $(USERLAND_RS_STAMP)
$(USER_CAT_ELF): $(USERLAND_RS_STAMP)
$(USER_ECHO_ELF): $(USERLAND_RS_STAMP)
$(USER_TRUE_ELF): $(USERLAND_RS_STAMP)
$(USER_FALSE_ELF): $(USERLAND_RS_STAMP)
$(USER_TOUCH_ELF): $(USERLAND_RS_STAMP)
$(USER_MOUNT_ELF): $(USERLAND_RS_STAMP)
$(USER_LOGIN_ELF): $(USERLAND_RS_STAMP)
$(USER_RISTUX_INSTALL_ELF): $(USERLAND_RS_STAMP)
$(USER_FDISK_ELF): $(USERLAND_RS_STAMP)
$(USER_MKFS_EXT2_ELF): $(USERLAND_RS_STAMP)
$(USER_ID_ELF): $(USERLAND_RS_STAMP)
$(USER_SU_ELF): $(USERLAND_RS_STAMP)
$(USER_SLEEP_ELF): $(USERLAND_RS_STAMP)
$(USER_SHUTDOWN_ELF): $(USERLAND_RS_STAMP)
$(USER_PING_ELF): $(USERLAND_RS_STAMP)
$(USER_CURL_LITE_ELF): $(USERLAND_RS_STAMP)
$(USER_LOOPBACK_CHECK_ELF): $(USERLAND_RS_STAMP)
$(USER_SSH_BANNER_ELF): $(USERLAND_RS_STAMP)
$(USER_PTY_SHELL_CHECK_ELF): $(USERLAND_RS_STAMP)
$(USER_SIG_DEMO_ELF): $(USERLAND_RS_STAMP)
$(USER_EDIT_ELF): $(USERLAND_RS_STAMP)
$(USER_ANSI_DEMO_ELF): $(USERLAND_RS_STAMP)
$(USER_TAR_ELF): $(USERLAND_RS_STAMP)
$(USER_PKG_ELF): $(USERLAND_RS_STAMP)
$(USER_AR_ELF): $(USERLAND_RS_STAMP)
$(USER_PKGCONF_ELF): $(USERLAND_RS_STAMP)
$(USER_MAKE_ELF): $(USERLAND_RS_STAMP)
$(USER_TOOLCHAIN_ELF): $(USERLAND_RS_STAMP)
$(USER_CP_ELF): $(USERLAND_RS_STAMP)
$(USER_MV_ELF): $(USERLAND_RS_STAMP)
$(USER_MKDIR_ELF): $(USERLAND_RS_STAMP)
$(USER_RM_ELF): $(USERLAND_RS_STAMP)
$(USER_CHMOD_ELF): $(USERLAND_RS_STAMP)
$(USER_LS_ELF): $(USERLAND_RS_STAMP)
$(USER_KILL_ELF): $(USERLAND_RS_STAMP)
$(USER_PWD_ELF): $(USERLAND_RS_STAMP)
$(USER_UDP_ELF): $(USERLAND_RS_STAMP)
$(USER_GREP_ELF): $(USERLAND_RS_STAMP)
$(USER_PRINTF_ELF): $(USERLAND_RS_STAMP)
$(USER_TEST_ELF): $(USERLAND_RS_STAMP)
$(USER_LN_ELF): $(USERLAND_RS_STAMP)
$(USER_READLINK_ELF): $(USERLAND_RS_STAMP)
$(USER_WC_ELF): $(USERLAND_RS_STAMP)
$(USER_HEAD_ELF): $(USERLAND_RS_STAMP)
$(USER_TAIL_ELF): $(USERLAND_RS_STAMP)
$(USER_TEE_ELF): $(USERLAND_RS_STAMP)
$(USER_SORT_ELF): $(USERLAND_RS_STAMP)
$(USER_UNIQ_ELF): $(USERLAND_RS_STAMP)
$(USER_BASENAME_ELF): $(USERLAND_RS_STAMP)
$(USER_DIRNAME_ELF): $(USERLAND_RS_STAMP)
$(USER_INSTALL_ELF): $(USERLAND_RS_STAMP)
$(USER_ENV_ELF): $(USERLAND_RS_STAMP)
$(USER_CUT_ELF): $(USERLAND_RS_STAMP)
$(USER_FIND_ELF): $(USERLAND_RS_STAMP)
$(USER_XARGS_ELF): $(USERLAND_RS_STAMP)
$(USER_SED_ELF): $(USERLAND_RS_STAMP)
$(USER_UNAME_ELF): $(USERLAND_RS_STAMP)
$(USER_TR_ELF): $(USERLAND_RS_STAMP)
$(USER_DATE_ELF): $(USERLAND_RS_STAMP)
$(USER_WHICH_ELF): $(USERLAND_RS_STAMP)
$(USER_CMP_ELF): $(USERLAND_RS_STAMP)
$(USER_DD_ELF): $(USERLAND_RS_STAMP)
$(USER_SEQ_ELF): $(USERLAND_RS_STAMP)
$(USER_EXPR_ELF): $(USERLAND_RS_STAMP)
$(USER_YES_ELF): $(USERLAND_RS_STAMP)
$(USER_DIFF_ELF): $(USERLAND_RS_STAMP)
$(USER_AWK_ELF): $(USERLAND_RS_STAMP)
$(USER_PATCH_ELF): $(USERLAND_RS_STAMP)
$(USER_GZIP_ELF): $(USERLAND_RS_STAMP)
$(USER_XZ_ELF): $(USERLAND_RS_STAMP)
$(USER_STAT_ELF): $(USERLAND_RS_STAMP)

$(ROOTFS_BUILDER): tools/build_rootfs.rs tools/package_archive.rs Makefile
	mkdir -p build
	$(RUSTC) $(HOST_TOOL_RUSTFLAGS) $< -o $@

$(EXT2_DISK_BUILDER): tools/build_ext2_disk.rs tools/package_archive.rs Makefile
	mkdir -p build
	$(RUSTC) $(HOST_TOOL_RUSTFLAGS) $< -o $@

$(VM_DISK_BUILDER): tools/build_vm_disk.rs Makefile
	mkdir -p build
	$(RUSTC) $(HOST_TOOL_RUSTFLAGS) $< -o $@

$(PACKAGE_TAR_BUILDER): tools/build_package_tar.rs Makefile
	mkdir -p build
	$(RUSTC) $(HOST_TOOL_RUSTFLAGS) $< -o $@

$(ROOTFS_BASE_PACKAGE_TAR): $(PACKAGE_TAR_BUILDER) $(ROOTFS_BASE_PACKAGE_INPUTS)
	mkdir -p build/packages
	$(PACKAGE_TAR_BUILDER) $@ $(ROOTFS_BASE_PACKAGE_DIR)

$(ROOTFS_BASE_PACKAGE_ARCHIVE): $(ROOTFS_BASE_PACKAGE_TAR)
	gzip -n -c $< > $@

$(ROOTFS_GZIP_TESTDATA_ARCHIVE): $(ROOTFS_GZIP_TESTDATA_SOURCE)
	mkdir -p build/testdata
	gzip -n -c $< > $@

$(ROOTFS_SOURCEPKG_TAR): $(PACKAGE_TAR_BUILDER) $(ROOTFS_SOURCEPKG_INPUTS)
	mkdir -p build/testdata
	$(PACKAGE_TAR_BUILDER) $@ $(ROOTFS_SOURCEPKG_DIR)

$(ROOTFS_SOURCEPKG_ARCHIVE): $(ROOTFS_SOURCEPKG_TAR)
	gzip -n -c $< > $@

$(ISO_INITRD): $(USERLAND_RS_STAMP) $(RUST_SYSROOT_STAMP) $(ROOTFS_BUILDER) $(ROOTFS_INPUTS)
	$(ROOTFS_BUILDER) $(ISO_INITRD) $(ROOTFS_MANIFEST)

rootfs: $(ISO_INITRD)

$(INSTALLED_GRUB_CFG):
	mkdir -p $(@D)
	printf '%s\n' \
	  'set timeout=0' \
	  'set default=0' \
	  'terminal_output console' \
	  '' \
	  'menuentry "ristux" {' \
	  '    multiboot2 /boot/ristux.elf root=/dev/vda1' \
	  '    module2 /boot/initrd.bin initrd' \
	  '    boot' \
	  '}' > $@

$(DISK_IMAGE): $(ISO_KERNEL) $(ISO_INITRD) $(EXT2_DISK_BUILDER) $(INSTALLED_GRUB_CFG) $(ROOTFS_MANIFEST) $(RUST_SYSROOT_STAMP) $(ROOTFS_INPUTS)
	$(EXT2_DISK_BUILDER) $(DISK_IMAGE) $(ROOTFS_MANIFEST) $(ISO_KERNEL) $(ISO_INITRD) $(INSTALLED_GRUB_CFG)

disk: $(DISK_IMAGE)

$(GRUB_BOOT_IMG):
	@test -n "$(GRUB_BIOS_DIR)" || { echo "GRUB i386-pc boot.img not found; set GRUB_BIOS_DIR" >&2; exit 1; }
	mkdir -p $(@D)
	cp "$(GRUB_BIOS_DIR)/boot.img" $@

$(GRUB_EMBEDDED_CFG):
	mkdir -p $(@D)
	printf '%s\n' \
	  'set root=(hd0,msdos1)' \
	  'set prefix=(hd0,msdos1)/boot/grub' \
	  'multiboot2 /boot/ristux.elf root=/dev/vda1' \
	  'module2 /boot/initrd.bin initrd' \
	  'boot' > $@

$(GRUB_CORE_IMG): $(GRUB_EMBEDDED_CFG)
	mkdir -p $(@D)
	$(GRUB_MKIMAGE) -O i386-pc -o $@ -p /boot/grub -c $(GRUB_EMBEDDED_CFG) biosdisk part_msdos ext2 multiboot2

$(INSTALLER_INITRD): $(ROOTFS_BUILDER) $(INSTALLER_ROOTFS_MANIFEST) $(DISK_IMAGE) $(GRUB_BOOT_IMG) $(GRUB_CORE_IMG) $(ISO_KERNEL) $(ISO_INITRD) $(RUST_SYSROOT_STAMP) $(ROOTFS_INPUTS)
	mkdir -p $(@D)
	$(ROOTFS_BUILDER) $@ $(INSTALLER_ROOTFS_MANIFEST)

$(INSTALLER_GRUB_CFG):
	mkdir -p $(@D)
	printf '%s\n' \
	  'set timeout=0' \
	  'set default=0' \
	  'set gfxpayload=text' \
	  'terminal_output console' \
	  '' \
	  'menuentry "Install ristux" {' \
	  '    multiboot2 /boot/ristux.elf ristux.mode=install' \
	  '    module2 /boot/initrd.bin initrd' \
	  '    boot' \
	  '}' > $@

check-multiboot: $(ISO_KERNEL)
	$(GRUB_FILE) --is-x86-multiboot2 $(ISO_KERNEL)

iso: check-multiboot $(ISO_INITRD) $(DISK_IMAGE)
	mkdir -p build
	$(GRUB_MKRESCUE) -o $(ISO_IMAGE) $(ISO_DIR)

installer-iso: check-multiboot $(INSTALLER_INITRD) $(INSTALLER_GRUB_CFG)
	rm -rf $(INSTALLER_ISO_DIR)
	mkdir -p $(INSTALLER_ISO_DIR)/boot/grub
	cp $(ISO_KERNEL) $(INSTALLER_ISO_DIR)/boot/ristux.elf
	cp $(INSTALLER_INITRD) $(INSTALLER_ISO_DIR)/boot/initrd.bin
	cp $(INSTALLER_GRUB_CFG) $(INSTALLER_ISO_DIR)/boot/grub/grub.cfg
	$(GRUB_MKRESCUE) -o $(INSTALLER_ISO_IMAGE) $(INSTALLER_ISO_DIR)

vm-blank:
	mkdir -p build
	dd if=/dev/zero of=$(VM_BLANK_IMAGE) bs=1m count=0 seek=$$(($(VM_DISK_SIZE) / 1048576))

$(VM_IMAGE): $(VM_DISK_BUILDER) $(GRUB_BOOT_IMG) $(GRUB_CORE_IMG) $(DISK_IMAGE)
	$(VM_DISK_BUILDER) $@ $(VM_DISK_SIZE) $(GRUB_BOOT_IMG) $(GRUB_CORE_IMG) $(DISK_IMAGE)

vm-image: $(VM_IMAGE)

vm-qcow2: $(VM_IMAGE)
	$(QEMU_IMG) convert -f raw -O qcow2 $(VM_IMAGE) $(VM_QCOW2_IMAGE)

run: iso disk
	QEMU="$(QEMU)" QEMU_FLAGS="$(QEMU_FLAGS)" QEMU_DISPLAY="$(QEMU_DISPLAY)" QEMU_KEYMAP="$(QEMU_KEYMAP)" QEMU_WINDOW_BOUNDS="$(QEMU_WINDOW_BOUNDS)" QEMU_WINDOW_TITLE="$(QEMU_WINDOW_TITLE)" ISO_IMAGE="$(ISO_IMAGE)" DISK_IMAGE="$(DISK_IMAGE)" scripts/run_qemu_display.sh

run-headless: iso
	scripts/run_qemu.sh --headless

run-ssh: iso disk
	scripts/run_qemu.sh --headless --ssh-forward=10022

smoke:
	scripts/smoke_test.sh

quick:
	scripts/quick_fixture.sh boot

quick-%:
	scripts/quick_fixture.sh $*

rust-std-probe:
	scripts/probe_rust_std.sh

rust-std-probe-current:
	scripts/probe_rust_std.sh --expect-std-link-success

rust-std-probe-current-blocker: rust-std-probe-current

rust-std-probe-binary: $(RUST_STD_PROBE_ELF)

rust-std-sysroot: $(RUST_STD_SYSROOT_STAMP)

rust-official-target-probe: scripts/probe_rust_target.sh $(RUST_OVERLAY_TREE)/rust-src/compiler/rustc_target/src/spec/targets/x86_64_unknown_ristux.rs
	scripts/probe_rust_target.sh

rust-official-bootstrap-std: scripts/probe_rust_bootstrap_std.sh scripts/probe_rust_target.sh $(RUST_OVERLAY_INPUTS)
	scripts/probe_rust_bootstrap_std.sh

rust-official-bootstrap-stage2: scripts/probe_rust_bootstrap_stage2.sh scripts/probe_rust_target.sh userland/src/bin/ristux_ld.rs $(RUST_OVERLAY_INPUTS)
	scripts/probe_rust_bootstrap_stage2.sh

rust-official-std-probe:
	scripts/probe_rust_std.sh --expect-official-stage1-blocker

debug: iso
	scripts/debug_qemu.sh

test: smoke

clean:
	$(CARGO) clean
	$(CARGO) clean --manifest-path userland/Cargo.toml
	rm -rf build
	rm -f $(ISO_KERNEL)
	rm -f $(ISO_INITRD)
