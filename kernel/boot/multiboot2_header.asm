.section .multiboot2_header, "a"
.align 8

.set MULTIBOOT2_MAGIC, 0xe85250d6
.set MULTIBOOT2_ARCHITECTURE_I386, 0
.set MULTIBOOT2_HEADER_LENGTH, multiboot2_header_end - multiboot2_header
.set MULTIBOOT2_CHECKSUM, -(MULTIBOOT2_MAGIC + MULTIBOOT2_ARCHITECTURE_I386 + MULTIBOOT2_HEADER_LENGTH)

multiboot2_header:
    .long MULTIBOOT2_MAGIC
    .long MULTIBOOT2_ARCHITECTURE_I386
    .long MULTIBOOT2_HEADER_LENGTH
    .long MULTIBOOT2_CHECKSUM

    .align 8
    .short 0
    .short 0
    .long 8
multiboot2_header_end:
