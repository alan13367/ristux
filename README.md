# ristux

`ristux` is a small experimental Rust kernel loaded by GRUB through
Multiboot2.

## Requirements

- Nightly Rust with `rust-src` for the repo-local custom target:

  ```sh
  rustup toolchain install nightly --component rust-src
  ```

- GRUB tools: `grub-file` and `grub-mkrescue`
  - Homebrew packages BIOS-capable GRUB as `i686-elf-grub-file` and
    `i686-elf-grub-mkrescue`; the Makefile detects prefixed and unprefixed
    naming schemes.
- `xorriso`, usually required by `grub-mkrescue`
- `mtools`, required by the Homebrew GRUB rescue image workflow
- QEMU: `qemu-system-x86_64`

On macOS with Homebrew, the non-Rust tools are typically installed with:

```sh
brew install i686-elf-grub xorriso mtools qemu
```

## Build

Build the freestanding kernel ELF:

```sh
cargo build
```

Cargo is configured to use `targets/x86_64-ristux-kernel.json`.

Build the release ELF used for the bootable ISO:

```sh
make build
```

## Check Multiboot2 Compatibility

```sh
make check-multiboot
```

This copies the release kernel to `iso/boot/ristux.elf` and verifies it with:

```sh
grub-file --is-x86-multiboot2 iso/boot/ristux.elf
```

## Build the ISO

```sh
make iso
```

The ISO is written to `build/ristux.iso`.

## Run in QEMU

```sh
make run
```

Equivalent QEMU command:

```sh
qemu-system-x86_64 -cdrom build/ristux.iso -m 256M -no-reboot -no-shutdown
```

The Phase 1/2 kernel enters `kernel_main` and halts safely. It does not print
anything yet; serial or screen output starts in a later roadmap phase.
