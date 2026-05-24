ENTRY(_start)
OUTPUT_FORMAT(elf64-x86-64)

KERNEL_LOAD_ADDR = 1M;

SECTIONS
{
    . = KERNEL_LOAD_ADDR;

    .boot ALIGN(8) :
    {
        KEEP(*(.multiboot2_header))
        *(.text.boot)
        *(.rodata.boot)
    }

    .text ALIGN(4K) :
    {
        *(.text .text.*)
    }

    .rodata ALIGN(4K) :
    {
        *(.rodata .rodata.*)
    }

    .data ALIGN(4K) :
    {
        *(.data .data.*)
    }

    .bss ALIGN(4K) :
    {
        *(COMMON)
        *(.bss .bss.*)
    }

    /DISCARD/ :
    {
        *(.eh_frame_hdr)
        *(.eh_frame .eh_frame.*)
        *(.note .note.*)
        *(.comment)
    }
}
