CARGO ?= cargo
RUSTC ?= rustc
CLANG ?= clang
GRUB_FILE ?= $(shell command -v grub-file 2>/dev/null || command -v i686-elf-grub-file 2>/dev/null || command -v x86_64-elf-grub-file 2>/dev/null || printf grub-file)
GRUB_MKRESCUE ?= $(shell command -v grub-mkrescue 2>/dev/null || command -v i686-elf-grub-mkrescue 2>/dev/null || command -v x86_64-elf-grub-mkrescue 2>/dev/null || printf grub-mkrescue)
GRUB_MKIMAGE ?= $(shell command -v grub-mkimage 2>/dev/null || command -v i686-elf-grub-mkimage 2>/dev/null || command -v x86_64-elf-grub-mkimage 2>/dev/null || printf grub-mkimage)
GRUB_BIOS_DIR ?= $(shell for d in /usr/lib/grub/i386-pc /usr/local/lib/grub/i386-pc /opt/homebrew/lib/grub/i386-pc $$(find /opt/homebrew/Cellar -path '*/lib/*/grub/i386-pc' -type d 2>/dev/null | sort -r); do if test -f "$$d/boot.img"; then printf '%s' "$$d"; break; fi; done)
QEMU_IMG ?= qemu-img
QEMU ?= qemu-system-x86_64
QEMU_FLAGS ?= -m 256M -smp 4
QEMU_DISPLAY ?= $(shell if $(QEMU) -display help 2>/dev/null | grep -qx cocoa; then printf '%s' '-display cocoa,zoom-to-fit=on'; fi)
QEMU_KEYMAP ?= es
QEMU_WINDOW_BOUNDS ?= 80,80,1360,820
QEMU_WINDOW_TITLE ?= Ristux
RUST_HOST := $(shell $(RUSTC) -vV | sed -n 's/^host: //p')
RUST_LLD ?= $(shell $(RUSTC) --print sysroot)/lib/rustlib/$(RUST_HOST)/bin/rust-lld
HOST_AR ?= $(shell $(RUSTC) --print sysroot)/lib/rustlib/$(RUST_HOST)/bin/llvm-ar

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
VM_DISK_SIZE ?= 1073741824
VM_BLANK_IMAGE := build/ristux-blank.raw
VM_IMAGE := build/ristux-vm.raw
VM_QCOW2_IMAGE := build/ristux-vm.qcow2
USERLAND_RS_TARGET := x86_64-ristux-user
USERLAND_RS_OUT := userland/target/$(USERLAND_RS_TARGET)/release
USERLAND_RS_SRC := \
	userland/Cargo.toml \
	userland/linker.ld \
	$(wildcard userland/src/*.rs) \
	$(wildcard userland/src/bin/*.rs) \
	targets/x86_64-ristux-user.json
USERLAND_RS_BINS := init sh cat echo true false touch mount login ristux_install fdisk mkfs_ext2 id su sleep shutdown ping ip curl_lite loopback_check ssh_banner pty_shell_check sig_demo edit ansi_demo tar pkg ar pkgconf make toolchain cp mv ls mkdir rm chmod kill pwd udp grep printf test ln readlink wc head tail tee sort uniq basename dirname install env cut find xargs sed uname hostname tr date which cmp dd df seq expr yes diff awk patch gzip xz stat chown uptime free ps
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
USER_STTY_OBJ := build/userland/stty.o
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
USER_TCC_ELF := build/userland/tcc.elf
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
USER_LIBC_OBJ := build/userland/libc.o
USER_LIBC_SO := build/userland/libc.so
USER_C_HEADERS := $(shell find userland/c/include -type f -name '*.h' 2>/dev/null | sort)
USER_C_CFLAGS := --target=x86_64-unknown-none-elf -std=c11 -ffreestanding -fno-builtin -fno-stack-protector -fno-pic -mno-red-zone -nostdinc -Iuserland/c/include -Wall -Wextra
USER_C_ASFLAGS := --target=x86_64-unknown-none-elf -x assembler -c
USER_CRT0_OBJ := build/userland/c/crt0.o
USER_CRTI_OBJ := build/userland/c/crti.o
USER_CRTN_OBJ := build/userland/c/crtn.o
USER_C_LIBC_OBJ := build/userland/c/libc.o
USER_C_LIBC_A := build/userland/c/libc.a
USER_CC_HELLO_OBJ := build/userland/c/cc_hello.o
USER_CC_HELLO_ELF := build/userland/cc_hello.elf
USER_CC_NEWLIB_HELLO_ELF := build/userland/cc_newlib_hello.elf
USER_CC_NEWLIB_POSIX_ELF := build/userland/cc_newlib_posix.elf
USER_CC_CRED_OBJ := build/userland/c/cc_cred.o
USER_CC_CRED_ELF := build/userland/cc_cred.elf
USER_CC_PASSWD_OBJ := build/userland/c/cc_passwd.o
USER_CC_PASSWD_ELF := build/userland/cc_passwd.elf
USER_CC_SESSION_OBJ := build/userland/c/cc_session.o
USER_CC_SESSION_ELF := build/userland/cc_session.elf
USER_CC_DEV_OBJ := build/userland/c/cc_dev.o
USER_CC_DEV_ELF := build/userland/cc_dev.elf
USER_CC_DNS_OBJ := build/userland/c/cc_dns.o
USER_CC_DNS_ELF := build/userland/cc_dns.elf
USER_CC_HTTP_OBJ := build/userland/c/cc_http.o
USER_CC_HTTP_ELF := build/userland/cc_http.elf
USER_CC_COW_OBJ := build/userland/c/cc_cow.o
USER_CC_COW_ELF := build/userland/cc_cow.elf
USER_CC_EXT2_OBJ := build/userland/c/cc_ext2.o
USER_CC_EXT2_ELF := build/userland/cc_ext2.elf
USER_CC_FCNTL_OBJ := build/userland/c/cc_fcntl.o
USER_CC_FCNTL_ELF := build/userland/cc_fcntl.elf
USER_CC_FILE_SYNC_OBJ := build/userland/c/cc_file_sync.o
USER_CC_FILE_SYNC_ELF := build/userland/cc_file_sync.elf
USER_CC_MMAP_OBJ := build/userland/c/cc_mmap.o
USER_CC_MMAP_ELF := build/userland/cc_mmap.elf
USER_CC_POLL_OBJ := build/userland/c/cc_poll.o
USER_CC_POLL_ELF := build/userland/cc_poll.elf
USER_CC_SELECT_OBJ := build/userland/c/cc_select.o
USER_CC_SELECT_ELF := build/userland/cc_select.elf
USER_CC_SOCKET_OBJ := build/userland/c/cc_socket.o
USER_CC_SOCKET_ELF := build/userland/cc_socket.elf
USER_CC_TCP_OBJ := build/userland/c/cc_tcp.o
USER_CC_TCP_ELF := build/userland/cc_tcp.elf
USER_CC_UIO_OBJ := build/userland/c/cc_uio.o
USER_CC_UIO_ELF := build/userland/cc_uio.elf
USER_CC_PATH_OBJ := build/userland/c/cc_path.o
USER_CC_PATH_ELF := build/userland/cc_path.elf
USER_CC_FS_OBJ := build/userland/c/cc_fs.o
USER_CC_FS_ELF := build/userland/cc_fs.elf
USER_CC_FUTEX_OBJ := build/userland/c/cc_futex.o
USER_CC_FUTEX_ELF := build/userland/cc_futex.elf
USER_CC_SIGNAL_OBJ := build/userland/c/cc_signal.o
USER_CC_SIGNAL_ELF := build/userland/cc_signal.elf
USER_CC_STACK_OBJ := build/userland/c/cc_stack.o
USER_CC_STACK_ELF := build/userland/cc_stack.elf
USER_CC_SSE_OBJ := build/userland/c/cc_sse.o
USER_CC_SSE_ELF := build/userland/cc_sse.elf
USER_CC_TTY_OBJ := build/userland/c/cc_tty.o
USER_CC_TTY_ELF := build/userland/cc_tty.elf
USER_CC_PTY_OBJ := build/userland/c/cc_pty.o
USER_CC_PTY_ELF := build/userland/cc_pty.elf
USER_CC_LINKS_OBJ := build/userland/c/cc_links.o
USER_CC_LINKS_ELF := build/userland/cc_links.elf
USER_CC_LIBC_COMPAT_OBJ := build/userland/c/cc_libc_compat.o
USER_CC_LIBC_COMPAT_ELF := build/userland/cc_libc_compat.elf
USER_CC_LIBC_HOSTED_OBJ := build/userland/c/cc_libc_hosted.o
USER_CC_LIBC_HOSTED_ELF := build/userland/cc_libc_hosted.elf
USER_CC_PROC_OBJ := build/userland/c/cc_proc.o
USER_CC_PROC_ELF := build/userland/cc_proc.elf
USER_CC_PROCFS_OBJ := build/userland/c/cc_procfs.o
USER_CC_PROCFS_ELF := build/userland/cc_procfs.elf
USER_CC_STATFS_OBJ := build/userland/c/cc_statfs.o
USER_CC_STATFS_ELF := build/userland/cc_statfs.elf
USER_DROPBEAR_ELF := build/userland/dropbear.elf
USER_DBCLIENT_ELF := build/userland/dbclient.elf
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
ROOTFS_NATIVEPKG_DIR := rootfs/testdata/nativepkg
ROOTFS_NATIVEPKG_INPUTS := $(shell find $(ROOTFS_NATIVEPKG_DIR) -type f 2>/dev/null | sort)
ROOTFS_NATIVEPKG_TAR := build/testdata/ristux-hello-0.1.tar
ROOTFS_NATIVEPKG_ARCHIVE := build/testdata/ristux-hello-0.1.tar.gz
TINYCC_PORT_STAMP := build/ports/tinycc/.port-stamp
TINYCC_INCLUDE_DIR := build/ports/tinycc/root/lib/tcc/include
ROOTFS_TINYCC_PROJECT_DIR := rootfs/testdata/tinycc-project
ROOTFS_TINYCC_PROJECT_INPUTS := $(shell find $(ROOTFS_TINYCC_PROJECT_DIR) -type f 2>/dev/null | sort)
ROOTFS_MAKE_IMPLICIT_DIR := rootfs/testdata/make-implicit
ROOTFS_MAKE_IMPLICIT_INPUTS := $(shell find $(ROOTFS_MAKE_IMPLICIT_DIR) -type f 2>/dev/null | sort)
ROOTFS_INPUTS := $(ROOTFS_MANIFEST) rootfs/etc/os-release rootfs/etc/resolv.conf rootfs/usr/lib/pkgconfig/libc.pc rootfs/usr/lib/pkgconfig/ristux.pc rootfs/testdata/ristuxpkg.patch rootfs/testdata/tinycc-hello.c userland/c/linker.ld $(USER_C_HEADERS) $(USER_CRT0_OBJ) $(USER_CRTI_OBJ) $(USER_CRTN_OBJ) $(USER_C_LIBC_A) $(TINYCC_PORT_STAMP) $(ROOTFS_TINYCC_PROJECT_INPUTS) $(ROOTFS_MAKE_IMPLICIT_INPUTS) $(ROOTFS_BASE_PACKAGE_ARCHIVE) $(ROOTFS_GZIP_TESTDATA_ARCHIVE) $(ROOTFS_SOURCEPKG_ARCHIVE) $(ROOTFS_NATIVEPKG_ARCHIVE)

.PHONY: all build rootfs disk dropbear-port newlib-port-check newlib-sysroot check-multiboot iso installer-iso vm-blank vm-image vm-qcow2 run run-headless run-ssh smoke quick quick-% debug test clean

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

$(USER_STTY_OBJ): userland/c/bin/stty.c $(USER_C_HEADERS)
	mkdir -p build/userland
	$(CLANG) $(USER_C_CFLAGS) -c $< -o $@

$(USER_STTY_ELF): $(USER_CRT0_OBJ) $(USER_CRTI_OBJ) $(USER_STTY_OBJ) $(USER_C_LIBC_OBJ) $(USER_CRTN_OBJ) userland/c/linker.ld
	$(RUST_LLD) -flavor gnu -T userland/c/linker.ld -o $@ $(USER_CRT0_OBJ) $(USER_CRTI_OBJ) $(USER_STTY_OBJ) $(USER_C_LIBC_OBJ) $(USER_CRTN_OBJ)

$(USER_LIBC_OBJ): userland/libc.S
	mkdir -p build/userland
	$(CLANG) --target=x86_64-unknown-none-elf -x assembler -c $< -o $@

$(USER_LIBC_SO): $(USER_LIBC_OBJ)
	$(RUST_LLD) -flavor gnu -shared -o $@ $(USER_LIBC_OBJ)

$(USER_CRT0_OBJ): userland/c/crt/crt0.S
	mkdir -p build/userland/c
	$(CLANG) $(USER_C_ASFLAGS) $< -o $@

$(USER_CRTI_OBJ): userland/c/crt/crti.S
	mkdir -p build/userland/c
	$(CLANG) $(USER_C_ASFLAGS) $< -o $@

$(USER_CRTN_OBJ): userland/c/crt/crtn.S
	mkdir -p build/userland/c
	$(CLANG) $(USER_C_ASFLAGS) $< -o $@

$(USER_C_LIBC_OBJ): userland/c/libc/libc.c $(USER_C_HEADERS)
	mkdir -p build/userland/c
	$(CLANG) $(USER_C_CFLAGS) -c $< -o $@

$(USER_C_LIBC_A): $(USER_C_LIBC_OBJ) Makefile
	$(HOST_AR) --format=gnu rcs $@ $<

$(USER_CC_HELLO_OBJ): userland/c/bin/cc_hello.c $(USER_C_HEADERS)
	mkdir -p build/userland/c
	$(CLANG) $(USER_C_CFLAGS) -c $< -o $@

$(USER_CC_HELLO_ELF): $(USER_CRT0_OBJ) $(USER_CRTI_OBJ) $(USER_CC_HELLO_OBJ) $(USER_C_LIBC_OBJ) $(USER_CRTN_OBJ) userland/c/linker.ld
	$(RUST_LLD) -flavor gnu -T userland/c/linker.ld -o $@ $(USER_CRT0_OBJ) $(USER_CRTI_OBJ) $(USER_CC_HELLO_OBJ) $(USER_C_LIBC_OBJ) $(USER_CRTN_OBJ)

$(USER_CC_NEWLIB_HELLO_ELF): scripts/build_newlib_sysroot.sh ports/newlib/ristux/newlib_hello.c ports/newlib/ristux/syscalls.c ports/newlib/ristux/crt0.S ports/newlib/ristux/linker.ld
	NEWLIB_OUTPUT_ELF=$@ scripts/build_newlib_sysroot.sh

$(USER_CC_NEWLIB_POSIX_ELF): scripts/build_newlib_sysroot.sh ports/newlib/ristux/newlib_posix.c ports/newlib/ristux/syscalls.c ports/newlib/ristux/crt0.S ports/newlib/ristux/linker.ld
	NEWLIB_PROBE_SOURCE=ports/newlib/ristux/newlib_posix.c NEWLIB_OUTPUT_ELF=$@ scripts/build_newlib_sysroot.sh

$(USER_CC_CRED_OBJ): userland/c/bin/cc_cred.c $(USER_C_HEADERS)
	mkdir -p build/userland/c
	$(CLANG) $(USER_C_CFLAGS) -c $< -o $@

$(USER_CC_CRED_ELF): $(USER_CRT0_OBJ) $(USER_CRTI_OBJ) $(USER_CC_CRED_OBJ) $(USER_C_LIBC_OBJ) $(USER_CRTN_OBJ) userland/c/linker.ld
	$(RUST_LLD) -flavor gnu -T userland/c/linker.ld -o $@ $(USER_CRT0_OBJ) $(USER_CRTI_OBJ) $(USER_CC_CRED_OBJ) $(USER_C_LIBC_OBJ) $(USER_CRTN_OBJ)

$(USER_CC_PASSWD_OBJ): userland/c/bin/cc_passwd.c $(USER_C_HEADERS)
	mkdir -p build/userland/c
	$(CLANG) $(USER_C_CFLAGS) -c $< -o $@

$(USER_CC_PASSWD_ELF): $(USER_CRT0_OBJ) $(USER_CRTI_OBJ) $(USER_CC_PASSWD_OBJ) $(USER_C_LIBC_OBJ) $(USER_CRTN_OBJ) userland/c/linker.ld
	$(RUST_LLD) -flavor gnu -T userland/c/linker.ld -o $@ $(USER_CRT0_OBJ) $(USER_CRTI_OBJ) $(USER_CC_PASSWD_OBJ) $(USER_C_LIBC_OBJ) $(USER_CRTN_OBJ)

$(USER_CC_SESSION_OBJ): userland/c/bin/cc_session.c $(USER_C_HEADERS)
	mkdir -p build/userland/c
	$(CLANG) $(USER_C_CFLAGS) -c $< -o $@

$(USER_CC_SESSION_ELF): $(USER_CRT0_OBJ) $(USER_CRTI_OBJ) $(USER_CC_SESSION_OBJ) $(USER_C_LIBC_OBJ) $(USER_CRTN_OBJ) userland/c/linker.ld
	$(RUST_LLD) -flavor gnu -T userland/c/linker.ld -o $@ $(USER_CRT0_OBJ) $(USER_CRTI_OBJ) $(USER_CC_SESSION_OBJ) $(USER_C_LIBC_OBJ) $(USER_CRTN_OBJ)

$(USER_CC_DEV_OBJ): userland/c/bin/cc_dev.c $(USER_C_HEADERS)
	mkdir -p build/userland/c
	$(CLANG) $(USER_C_CFLAGS) -c $< -o $@

$(USER_CC_DEV_ELF): $(USER_CRT0_OBJ) $(USER_CRTI_OBJ) $(USER_CC_DEV_OBJ) $(USER_C_LIBC_OBJ) $(USER_CRTN_OBJ) userland/c/linker.ld
	$(RUST_LLD) -flavor gnu -T userland/c/linker.ld -o $@ $(USER_CRT0_OBJ) $(USER_CRTI_OBJ) $(USER_CC_DEV_OBJ) $(USER_C_LIBC_OBJ) $(USER_CRTN_OBJ)

$(USER_CC_DNS_OBJ): userland/c/bin/cc_dns.c $(USER_C_HEADERS)
	mkdir -p build/userland/c
	$(CLANG) $(USER_C_CFLAGS) -c $< -o $@

$(USER_CC_DNS_ELF): $(USER_CRT0_OBJ) $(USER_CRTI_OBJ) $(USER_CC_DNS_OBJ) $(USER_C_LIBC_OBJ) $(USER_CRTN_OBJ) userland/c/linker.ld
	$(RUST_LLD) -flavor gnu -T userland/c/linker.ld -o $@ $(USER_CRT0_OBJ) $(USER_CRTI_OBJ) $(USER_CC_DNS_OBJ) $(USER_C_LIBC_OBJ) $(USER_CRTN_OBJ)

$(USER_CC_HTTP_OBJ): userland/c/bin/cc_http.c $(USER_C_HEADERS)
	mkdir -p build/userland/c
	$(CLANG) $(USER_C_CFLAGS) -c $< -o $@

$(USER_CC_HTTP_ELF): $(USER_CRT0_OBJ) $(USER_CRTI_OBJ) $(USER_CC_HTTP_OBJ) $(USER_C_LIBC_OBJ) $(USER_CRTN_OBJ) userland/c/linker.ld
	$(RUST_LLD) -flavor gnu -T userland/c/linker.ld -o $@ $(USER_CRT0_OBJ) $(USER_CRTI_OBJ) $(USER_CC_HTTP_OBJ) $(USER_C_LIBC_OBJ) $(USER_CRTN_OBJ)

$(USER_CC_COW_OBJ): userland/c/bin/cc_cow.c $(USER_C_HEADERS)
	mkdir -p build/userland/c
	$(CLANG) $(USER_C_CFLAGS) -c $< -o $@

$(USER_CC_COW_ELF): $(USER_CRT0_OBJ) $(USER_CRTI_OBJ) $(USER_CC_COW_OBJ) $(USER_C_LIBC_OBJ) $(USER_CRTN_OBJ) userland/c/linker.ld
	$(RUST_LLD) -flavor gnu -T userland/c/linker.ld -o $@ $(USER_CRT0_OBJ) $(USER_CRTI_OBJ) $(USER_CC_COW_OBJ) $(USER_C_LIBC_OBJ) $(USER_CRTN_OBJ)

$(USER_CC_EXT2_OBJ): userland/c/bin/cc_ext2.c $(USER_C_HEADERS)
	mkdir -p build/userland/c
	$(CLANG) $(USER_C_CFLAGS) -c $< -o $@

$(USER_CC_EXT2_ELF): $(USER_CRT0_OBJ) $(USER_CRTI_OBJ) $(USER_CC_EXT2_OBJ) $(USER_C_LIBC_OBJ) $(USER_CRTN_OBJ) userland/c/linker.ld
	$(RUST_LLD) -flavor gnu -T userland/c/linker.ld -o $@ $(USER_CRT0_OBJ) $(USER_CRTI_OBJ) $(USER_CC_EXT2_OBJ) $(USER_C_LIBC_OBJ) $(USER_CRTN_OBJ)

$(USER_CC_FCNTL_OBJ): userland/c/bin/cc_fcntl.c $(USER_C_HEADERS)
	mkdir -p build/userland/c
	$(CLANG) $(USER_C_CFLAGS) -c $< -o $@

$(USER_CC_FCNTL_ELF): $(USER_CRT0_OBJ) $(USER_CRTI_OBJ) $(USER_CC_FCNTL_OBJ) $(USER_C_LIBC_OBJ) $(USER_CRTN_OBJ) userland/c/linker.ld
	$(RUST_LLD) -flavor gnu -T userland/c/linker.ld -o $@ $(USER_CRT0_OBJ) $(USER_CRTI_OBJ) $(USER_CC_FCNTL_OBJ) $(USER_C_LIBC_OBJ) $(USER_CRTN_OBJ)

$(USER_CC_FILE_SYNC_OBJ): userland/c/bin/cc_file_sync.c $(USER_C_HEADERS)
	mkdir -p build/userland/c
	$(CLANG) $(USER_C_CFLAGS) -c $< -o $@

$(USER_CC_FILE_SYNC_ELF): $(USER_CRT0_OBJ) $(USER_CRTI_OBJ) $(USER_CC_FILE_SYNC_OBJ) $(USER_C_LIBC_OBJ) $(USER_CRTN_OBJ) userland/c/linker.ld
	$(RUST_LLD) -flavor gnu -T userland/c/linker.ld -o $@ $(USER_CRT0_OBJ) $(USER_CRTI_OBJ) $(USER_CC_FILE_SYNC_OBJ) $(USER_C_LIBC_OBJ) $(USER_CRTN_OBJ)

$(USER_CC_MMAP_OBJ): userland/c/bin/cc_mmap.c $(USER_C_HEADERS)
	mkdir -p build/userland/c
	$(CLANG) $(USER_C_CFLAGS) -c $< -o $@

$(USER_CC_MMAP_ELF): $(USER_CRT0_OBJ) $(USER_CRTI_OBJ) $(USER_CC_MMAP_OBJ) $(USER_C_LIBC_OBJ) $(USER_CRTN_OBJ) userland/c/linker.ld
	$(RUST_LLD) -flavor gnu -T userland/c/linker.ld -o $@ $(USER_CRT0_OBJ) $(USER_CRTI_OBJ) $(USER_CC_MMAP_OBJ) $(USER_C_LIBC_OBJ) $(USER_CRTN_OBJ)

$(USER_CC_POLL_OBJ): userland/c/bin/cc_poll.c $(USER_C_HEADERS)
	mkdir -p build/userland/c
	$(CLANG) $(USER_C_CFLAGS) -c $< -o $@

$(USER_CC_POLL_ELF): $(USER_CRT0_OBJ) $(USER_CRTI_OBJ) $(USER_CC_POLL_OBJ) $(USER_C_LIBC_OBJ) $(USER_CRTN_OBJ) userland/c/linker.ld
	$(RUST_LLD) -flavor gnu -T userland/c/linker.ld -o $@ $(USER_CRT0_OBJ) $(USER_CRTI_OBJ) $(USER_CC_POLL_OBJ) $(USER_C_LIBC_OBJ) $(USER_CRTN_OBJ)

$(USER_CC_SELECT_OBJ): userland/c/bin/cc_select.c $(USER_C_HEADERS)
	mkdir -p build/userland/c
	$(CLANG) $(USER_C_CFLAGS) -c $< -o $@

$(USER_CC_SELECT_ELF): $(USER_CRT0_OBJ) $(USER_CRTI_OBJ) $(USER_CC_SELECT_OBJ) $(USER_C_LIBC_OBJ) $(USER_CRTN_OBJ) userland/c/linker.ld
	$(RUST_LLD) -flavor gnu -T userland/c/linker.ld -o $@ $(USER_CRT0_OBJ) $(USER_CRTI_OBJ) $(USER_CC_SELECT_OBJ) $(USER_C_LIBC_OBJ) $(USER_CRTN_OBJ)

$(USER_CC_SOCKET_OBJ): userland/c/bin/cc_socket.c $(USER_C_HEADERS)
	mkdir -p build/userland/c
	$(CLANG) $(USER_C_CFLAGS) -c $< -o $@

$(USER_CC_SOCKET_ELF): $(USER_CRT0_OBJ) $(USER_CRTI_OBJ) $(USER_CC_SOCKET_OBJ) $(USER_C_LIBC_OBJ) $(USER_CRTN_OBJ) userland/c/linker.ld
	$(RUST_LLD) -flavor gnu -T userland/c/linker.ld -o $@ $(USER_CRT0_OBJ) $(USER_CRTI_OBJ) $(USER_CC_SOCKET_OBJ) $(USER_C_LIBC_OBJ) $(USER_CRTN_OBJ)

$(USER_CC_TCP_OBJ): userland/c/bin/cc_tcp.c $(USER_C_HEADERS)
	mkdir -p build/userland/c
	$(CLANG) $(USER_C_CFLAGS) -c $< -o $@

$(USER_CC_TCP_ELF): $(USER_CRT0_OBJ) $(USER_CRTI_OBJ) $(USER_CC_TCP_OBJ) $(USER_C_LIBC_OBJ) $(USER_CRTN_OBJ) userland/c/linker.ld
	$(RUST_LLD) -flavor gnu -T userland/c/linker.ld -o $@ $(USER_CRT0_OBJ) $(USER_CRTI_OBJ) $(USER_CC_TCP_OBJ) $(USER_C_LIBC_OBJ) $(USER_CRTN_OBJ)

$(USER_CC_UIO_OBJ): userland/c/bin/cc_uio.c $(USER_C_HEADERS)
	mkdir -p build/userland/c
	$(CLANG) $(USER_C_CFLAGS) -c $< -o $@

$(USER_CC_UIO_ELF): $(USER_CRT0_OBJ) $(USER_CRTI_OBJ) $(USER_CC_UIO_OBJ) $(USER_C_LIBC_OBJ) $(USER_CRTN_OBJ) userland/c/linker.ld
	$(RUST_LLD) -flavor gnu -T userland/c/linker.ld -o $@ $(USER_CRT0_OBJ) $(USER_CRTI_OBJ) $(USER_CC_UIO_OBJ) $(USER_C_LIBC_OBJ) $(USER_CRTN_OBJ)

$(USER_CC_PATH_OBJ): userland/c/bin/cc_path.c $(USER_C_HEADERS)
	mkdir -p build/userland/c
	$(CLANG) $(USER_C_CFLAGS) -c $< -o $@

$(USER_CC_PATH_ELF): $(USER_CRT0_OBJ) $(USER_CRTI_OBJ) $(USER_CC_PATH_OBJ) $(USER_C_LIBC_OBJ) $(USER_CRTN_OBJ) userland/c/linker.ld
	$(RUST_LLD) -flavor gnu -T userland/c/linker.ld -o $@ $(USER_CRT0_OBJ) $(USER_CRTI_OBJ) $(USER_CC_PATH_OBJ) $(USER_C_LIBC_OBJ) $(USER_CRTN_OBJ)

$(USER_CC_FS_OBJ): userland/c/bin/cc_fs.c $(USER_C_HEADERS)
	mkdir -p build/userland/c
	$(CLANG) $(USER_C_CFLAGS) -c $< -o $@

$(USER_CC_FS_ELF): $(USER_CRT0_OBJ) $(USER_CRTI_OBJ) $(USER_CC_FS_OBJ) $(USER_C_LIBC_OBJ) $(USER_CRTN_OBJ) userland/c/linker.ld
	$(RUST_LLD) -flavor gnu -T userland/c/linker.ld -o $@ $(USER_CRT0_OBJ) $(USER_CRTI_OBJ) $(USER_CC_FS_OBJ) $(USER_C_LIBC_OBJ) $(USER_CRTN_OBJ)

$(USER_CC_FUTEX_OBJ): userland/c/bin/cc_futex.c $(USER_C_HEADERS)
	mkdir -p build/userland/c
	$(CLANG) $(USER_C_CFLAGS) -c $< -o $@

$(USER_CC_FUTEX_ELF): $(USER_CRT0_OBJ) $(USER_CRTI_OBJ) $(USER_CC_FUTEX_OBJ) $(USER_C_LIBC_OBJ) $(USER_CRTN_OBJ) userland/c/linker.ld
	$(RUST_LLD) -flavor gnu -T userland/c/linker.ld -o $@ $(USER_CRT0_OBJ) $(USER_CRTI_OBJ) $(USER_CC_FUTEX_OBJ) $(USER_C_LIBC_OBJ) $(USER_CRTN_OBJ)

$(USER_CC_SIGNAL_OBJ): userland/c/bin/cc_signal.c $(USER_C_HEADERS)
	mkdir -p build/userland/c
	$(CLANG) $(USER_C_CFLAGS) -c $< -o $@

$(USER_CC_SIGNAL_ELF): $(USER_CRT0_OBJ) $(USER_CRTI_OBJ) $(USER_CC_SIGNAL_OBJ) $(USER_C_LIBC_OBJ) $(USER_CRTN_OBJ) userland/c/linker.ld
	$(RUST_LLD) -flavor gnu -T userland/c/linker.ld -o $@ $(USER_CRT0_OBJ) $(USER_CRTI_OBJ) $(USER_CC_SIGNAL_OBJ) $(USER_C_LIBC_OBJ) $(USER_CRTN_OBJ)

$(USER_CC_STACK_OBJ): userland/c/bin/cc_stack.c $(USER_C_HEADERS)
	mkdir -p build/userland/c
	$(CLANG) $(USER_C_CFLAGS) -c $< -o $@

$(USER_CC_STACK_ELF): $(USER_CRT0_OBJ) $(USER_CRTI_OBJ) $(USER_CC_STACK_OBJ) $(USER_C_LIBC_OBJ) $(USER_CRTN_OBJ) userland/c/linker.ld
	$(RUST_LLD) -flavor gnu -T userland/c/linker.ld -o $@ $(USER_CRT0_OBJ) $(USER_CRTI_OBJ) $(USER_CC_STACK_OBJ) $(USER_C_LIBC_OBJ) $(USER_CRTN_OBJ)

$(USER_CC_SSE_OBJ): userland/c/bin/cc_sse.c $(USER_C_HEADERS)
	mkdir -p build/userland/c
	$(CLANG) $(USER_C_CFLAGS) -c $< -o $@

$(USER_CC_SSE_ELF): $(USER_CRT0_OBJ) $(USER_CRTI_OBJ) $(USER_CC_SSE_OBJ) $(USER_C_LIBC_OBJ) $(USER_CRTN_OBJ) userland/c/linker.ld
	$(RUST_LLD) -flavor gnu -T userland/c/linker.ld -o $@ $(USER_CRT0_OBJ) $(USER_CRTI_OBJ) $(USER_CC_SSE_OBJ) $(USER_C_LIBC_OBJ) $(USER_CRTN_OBJ)

$(USER_CC_TTY_OBJ): userland/c/bin/cc_tty.c $(USER_C_HEADERS)
	mkdir -p build/userland/c
	$(CLANG) $(USER_C_CFLAGS) -c $< -o $@

$(USER_CC_TTY_ELF): $(USER_CRT0_OBJ) $(USER_CRTI_OBJ) $(USER_CC_TTY_OBJ) $(USER_C_LIBC_OBJ) $(USER_CRTN_OBJ) userland/c/linker.ld
	$(RUST_LLD) -flavor gnu -T userland/c/linker.ld -o $@ $(USER_CRT0_OBJ) $(USER_CRTI_OBJ) $(USER_CC_TTY_OBJ) $(USER_C_LIBC_OBJ) $(USER_CRTN_OBJ)

$(USER_CC_PTY_OBJ): userland/c/bin/cc_pty.c $(USER_C_HEADERS)
	mkdir -p build/userland/c
	$(CLANG) $(USER_C_CFLAGS) -c $< -o $@

$(USER_CC_PTY_ELF): $(USER_CRT0_OBJ) $(USER_CRTI_OBJ) $(USER_CC_PTY_OBJ) $(USER_C_LIBC_OBJ) $(USER_CRTN_OBJ) userland/c/linker.ld
	$(RUST_LLD) -flavor gnu -T userland/c/linker.ld -o $@ $(USER_CRT0_OBJ) $(USER_CRTI_OBJ) $(USER_CC_PTY_OBJ) $(USER_C_LIBC_OBJ) $(USER_CRTN_OBJ)

$(USER_CC_LINKS_OBJ): userland/c/bin/cc_links.c $(USER_C_HEADERS)
	mkdir -p build/userland/c
	$(CLANG) $(USER_C_CFLAGS) -c $< -o $@

$(USER_CC_LINKS_ELF): $(USER_CRT0_OBJ) $(USER_CRTI_OBJ) $(USER_CC_LINKS_OBJ) $(USER_C_LIBC_OBJ) $(USER_CRTN_OBJ) userland/c/linker.ld
	$(RUST_LLD) -flavor gnu -T userland/c/linker.ld -o $@ $(USER_CRT0_OBJ) $(USER_CRTI_OBJ) $(USER_CC_LINKS_OBJ) $(USER_C_LIBC_OBJ) $(USER_CRTN_OBJ)

$(USER_CC_LIBC_COMPAT_OBJ): userland/c/bin/cc_libc_compat.c $(USER_C_HEADERS)
	mkdir -p build/userland/c
	$(CLANG) $(USER_C_CFLAGS) -c $< -o $@

$(USER_CC_LIBC_COMPAT_ELF): $(USER_CRT0_OBJ) $(USER_CRTI_OBJ) $(USER_CC_LIBC_COMPAT_OBJ) $(USER_C_LIBC_OBJ) $(USER_CRTN_OBJ) userland/c/linker.ld
	$(RUST_LLD) -flavor gnu -T userland/c/linker.ld -o $@ $(USER_CRT0_OBJ) $(USER_CRTI_OBJ) $(USER_CC_LIBC_COMPAT_OBJ) $(USER_C_LIBC_OBJ) $(USER_CRTN_OBJ)

$(USER_CC_LIBC_HOSTED_OBJ): userland/c/bin/cc_libc_hosted.c $(USER_C_HEADERS)
	mkdir -p build/userland/c
	$(CLANG) $(USER_C_CFLAGS) -c $< -o $@

$(USER_CC_LIBC_HOSTED_ELF): $(USER_CRT0_OBJ) $(USER_CRTI_OBJ) $(USER_CC_LIBC_HOSTED_OBJ) $(USER_C_LIBC_OBJ) $(USER_CRTN_OBJ) userland/c/linker.ld
	$(RUST_LLD) -flavor gnu -T userland/c/linker.ld -o $@ $(USER_CRT0_OBJ) $(USER_CRTI_OBJ) $(USER_CC_LIBC_HOSTED_OBJ) $(USER_C_LIBC_OBJ) $(USER_CRTN_OBJ)

$(TINYCC_PORT_STAMP): scripts/build_tinycc_port.sh $(USER_CRT0_OBJ) $(USER_CRTI_OBJ) $(USER_C_LIBC_OBJ) $(USER_CRTN_OBJ) $(USER_C_HEADERS) userland/c/linker.ld
	scripts/build_tinycc_port.sh $(USER_TCC_ELF) $(TINYCC_INCLUDE_DIR)
	touch $@

$(USER_TCC_ELF): $(TINYCC_PORT_STAMP)

$(USER_CC_PROC_OBJ): userland/c/bin/cc_proc.c $(USER_C_HEADERS)
	mkdir -p build/userland/c
	$(CLANG) $(USER_C_CFLAGS) -c $< -o $@

$(USER_CC_PROC_ELF): $(USER_CRT0_OBJ) $(USER_CRTI_OBJ) $(USER_CC_PROC_OBJ) $(USER_C_LIBC_OBJ) $(USER_CRTN_OBJ) userland/c/linker.ld
	$(RUST_LLD) -flavor gnu -T userland/c/linker.ld -o $@ $(USER_CRT0_OBJ) $(USER_CRTI_OBJ) $(USER_CC_PROC_OBJ) $(USER_C_LIBC_OBJ) $(USER_CRTN_OBJ)

$(USER_CC_PROCFS_OBJ): userland/c/bin/cc_procfs.c $(USER_C_HEADERS)
	mkdir -p build/userland/c
	$(CLANG) $(USER_C_CFLAGS) -c $< -o $@

$(USER_CC_PROCFS_ELF): $(USER_CRT0_OBJ) $(USER_CRTI_OBJ) $(USER_CC_PROCFS_OBJ) $(USER_C_LIBC_OBJ) $(USER_CRTN_OBJ) userland/c/linker.ld
	$(RUST_LLD) -flavor gnu -T userland/c/linker.ld -o $@ $(USER_CRT0_OBJ) $(USER_CRTI_OBJ) $(USER_CC_PROCFS_OBJ) $(USER_C_LIBC_OBJ) $(USER_CRTN_OBJ)

$(USER_CC_STATFS_OBJ): userland/c/bin/cc_statfs.c $(USER_C_HEADERS)
	mkdir -p build/userland/c
	$(CLANG) $(USER_C_CFLAGS) -c $< -o $@

$(USER_CC_STATFS_ELF): $(USER_CRT0_OBJ) $(USER_CRTI_OBJ) $(USER_CC_STATFS_OBJ) $(USER_C_LIBC_OBJ) $(USER_CRTN_OBJ) userland/c/linker.ld
	$(RUST_LLD) -flavor gnu -T userland/c/linker.ld -o $@ $(USER_CRT0_OBJ) $(USER_CRTI_OBJ) $(USER_CC_STATFS_OBJ) $(USER_C_LIBC_OBJ) $(USER_CRTN_OBJ)

$(USER_DROPBEAR_ELF) $(USER_DBCLIENT_ELF): $(USER_CRT0_OBJ) $(USER_CRTI_OBJ) $(USER_C_LIBC_OBJ) $(USER_CRTN_OBJ) userland/c/linker.ld scripts/build_dropbear_port.sh ports/dropbear/config.h ports/dropbear/localoptions.h
	scripts/build_dropbear_port.sh $(USER_DROPBEAR_ELF) $(USER_DBCLIENT_ELF)

dropbear-port: $(USER_DROPBEAR_ELF) $(USER_DBCLIENT_ELF)

newlib-port-check:
	scripts/check_newlib_port.sh

newlib-sysroot:
	scripts/build_newlib_sysroot.sh

$(ROOTFS_BUILDER): tools/build_rootfs.rs tools/package_archive.rs
	mkdir -p build
	$(RUSTC) $< -o $@

$(EXT2_DISK_BUILDER): tools/build_ext2_disk.rs tools/package_archive.rs
	mkdir -p build
	$(RUSTC) $< -o $@

$(VM_DISK_BUILDER): tools/build_vm_disk.rs
	mkdir -p build
	$(RUSTC) $< -o $@

$(PACKAGE_TAR_BUILDER): tools/build_package_tar.rs
	mkdir -p build
	$(RUSTC) $< -o $@

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

$(ROOTFS_NATIVEPKG_TAR): $(PACKAGE_TAR_BUILDER) $(ROOTFS_NATIVEPKG_INPUTS)
	mkdir -p build/testdata
	$(PACKAGE_TAR_BUILDER) $@ $(ROOTFS_NATIVEPKG_DIR)

$(ROOTFS_NATIVEPKG_ARCHIVE): $(ROOTFS_NATIVEPKG_TAR)
	gzip -n -c $< > $@

$(ISO_INITRD): $(USER_INIT_ELF) $(USER_SH_ELF) $(USER_CAT_ELF) $(USER_ECHO_ELF) $(USER_TRUE_ELF) $(USER_FALSE_ELF) $(USER_TOUCH_ELF) $(USER_MOUNT_ELF) $(USER_LOGIN_ELF) $(USER_RISTUX_INSTALL_ELF) $(USER_FDISK_ELF) $(USER_MKFS_EXT2_ELF) $(USER_ID_ELF) $(USER_SU_ELF) $(USER_SLEEP_ELF) $(USER_SHUTDOWN_ELF) $(USER_STTY_ELF) $(USER_PING_ELF) $(USER_CURL_LITE_ELF) $(USER_LOOPBACK_CHECK_ELF) $(USER_SSH_BANNER_ELF) $(USER_PTY_SHELL_CHECK_ELF) $(USER_SIG_DEMO_ELF) $(USER_EDIT_ELF) $(USER_ANSI_DEMO_ELF) $(USER_TAR_ELF) $(USER_PKG_ELF) $(USER_AR_ELF) $(USER_PKGCONF_ELF) $(USER_MAKE_ELF) $(USER_TOOLCHAIN_ELF) $(USER_TCC_ELF) $(USER_CP_ELF) $(USER_MV_ELF) $(USER_GREP_ELF) $(USER_PRINTF_ELF) $(USER_TEST_ELF) $(USER_LN_ELF) $(USER_READLINK_ELF) $(USER_WC_ELF) $(USER_HEAD_ELF) $(USER_TAIL_ELF) $(USER_TEE_ELF) $(USER_SORT_ELF) $(USER_UNIQ_ELF) $(USER_BASENAME_ELF) $(USER_DIRNAME_ELF) $(USER_INSTALL_ELF) $(USER_ENV_ELF) $(USER_CUT_ELF) $(USER_FIND_ELF) $(USER_XARGS_ELF) $(USER_SED_ELF) $(USER_UNAME_ELF) $(USER_TR_ELF) $(USER_DATE_ELF) $(USER_WHICH_ELF) $(USER_CMP_ELF) $(USER_DD_ELF) $(USER_SEQ_ELF) $(USER_EXPR_ELF) $(USER_YES_ELF) $(USER_DIFF_ELF) $(USER_AWK_ELF) $(USER_PATCH_ELF) $(USER_GZIP_ELF) $(USER_XZ_ELF) $(USER_STAT_ELF) $(USER_LS_ELF) $(USER_PWD_ELF) $(USER_CHMOD_ELF) $(USER_KILL_ELF) $(USER_MKDIR_ELF) $(USER_RM_ELF) $(USER_UDP_ELF) $(USER_LIBC_SO) $(USER_CC_HELLO_ELF) $(USER_CC_NEWLIB_HELLO_ELF) $(USER_CC_NEWLIB_POSIX_ELF) $(USER_CC_CRED_ELF) $(USER_CC_PASSWD_ELF) $(USER_CC_SESSION_ELF) $(USER_CC_DEV_ELF) $(USER_CC_DNS_ELF) $(USER_CC_HTTP_ELF) $(USER_CC_COW_ELF) $(USER_CC_EXT2_ELF) $(USER_CC_FCNTL_ELF) $(USER_CC_FILE_SYNC_ELF) $(USER_CC_MMAP_ELF) $(USER_CC_POLL_ELF) $(USER_CC_SELECT_ELF) $(USER_CC_SOCKET_ELF) $(USER_CC_TCP_ELF) $(USER_CC_UIO_ELF) $(USER_CC_PATH_ELF) $(USER_CC_FS_ELF) $(USER_CC_FUTEX_ELF) $(USER_CC_SIGNAL_ELF) $(USER_CC_STACK_ELF) $(USER_CC_SSE_ELF) $(USER_CC_TTY_ELF) $(USER_CC_PTY_ELF) $(USER_CC_LINKS_ELF) $(USER_CC_LIBC_COMPAT_ELF) $(USER_CC_LIBC_HOSTED_ELF) $(USER_CC_PROC_ELF) $(USER_CC_PROCFS_ELF) $(USER_CC_STATFS_ELF) $(USER_DROPBEAR_ELF) $(USER_DBCLIENT_ELF) $(ROOTFS_BUILDER) $(ROOTFS_INPUTS)
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

$(DISK_IMAGE): $(ISO_KERNEL) $(ISO_INITRD) $(EXT2_DISK_BUILDER) $(INSTALLED_GRUB_CFG) $(ROOTFS_MANIFEST) $(ROOTFS_INPUTS)
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

$(INSTALLER_INITRD): $(ROOTFS_BUILDER) $(INSTALLER_ROOTFS_MANIFEST) $(DISK_IMAGE) $(GRUB_BOOT_IMG) $(GRUB_CORE_IMG) $(ISO_KERNEL) $(ISO_INITRD) $(ROOTFS_INPUTS)
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

debug: iso
	scripts/debug_qemu.sh

test: smoke

clean:
	$(CARGO) clean
	$(CARGO) clean --manifest-path userland/Cargo.toml
	rm -rf build
	rm -f $(ISO_KERNEL)
	rm -f $(ISO_INITRD)
