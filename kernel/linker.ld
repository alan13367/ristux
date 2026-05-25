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
        KEEP(*(.ap_trampoline))
    }

    .bss.boot ALIGN(4K) :
    {
        *(.bss.boot)
    }

    /* Transition VMA to higher-half, keeping LMA contiguous */
    . = . + 0xffffffff80000000;
    __kernel_start = .;

    .text ALIGN(4K) : AT(ADDR(.text) - 0xffffffff80000000)
    {
        *(.text .text.*)
    }

    .rodata ALIGN(4K) : AT(ADDR(.rodata) - 0xffffffff80000000)
    {
        *(.rodata .rodata.*)
    }

    .data ALIGN(4K) : AT(ADDR(.data) - 0xffffffff80000000)
    {
        *(.data .data.*)
    }

    .bss ALIGN(4K) : AT(ADDR(.bss) - 0xffffffff80000000)
    {
        *(COMMON)
        *(.bss .bss.*)
    }

    __kernel_end = .;

    /DISCARD/ :
    {
        *(.eh_frame_hdr)
        *(.eh_frame .eh_frame.*)
        *(.note .note.*)
        *(.comment)
    }
}
