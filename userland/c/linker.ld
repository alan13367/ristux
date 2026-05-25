ENTRY(_start)

PHDRS
{
    text   PT_LOAD FLAGS(5);
    rodata PT_LOAD FLAGS(4);
    data   PT_LOAD FLAGS(6);
}

SECTIONS
{
    . = 0x40100000;

    .text ALIGN(4K) : {
        *(.text .text.*)
    } :text

    .init ALIGN(16) : {
        KEEP(*(.init))
    } :text

    .fini ALIGN(16) : {
        KEEP(*(.fini))
    } :text

    . = ALIGN(4K);
    .rodata ALIGN(4K) : {
        *(.rodata .rodata.*)
    } :rodata

    . = ALIGN(4K);
    .data ALIGN(4K) : {
        *(.data .data.*)
        *(.got .got.*)
        *(.got.plt)
    } :data

    .bss ALIGN(4K) : {
        *(COMMON)
        *(.bss .bss.*)
    } :data

    /DISCARD/ : {
        *(.eh_frame .eh_frame.*)
        *(.note .note.*)
        *(.comment)
        *(.gnu.hash)
        *(.hash)
        *(.dynsym)
        *(.dynstr)
        *(.gnu.version*)
        *(.interp)
        *(.dynamic)
        *(.rela.* .rel.*)
        *(.plt .plt.*)
    }
}
