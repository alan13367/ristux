.section .text.boot, "ax"
.global _start
.type _start, @function

.code32
_start:
    cli
    mov dword ptr [multiboot_magic], eax
    mov dword ptr [multiboot_info_addr], ebx
    mov esp, offset boot_stack_top

    call setup_page_tables
    call enable_long_mode

    lgdt [gdt64_pointer]

    .byte 0xea
    .long long_mode_start
    .word GDT64_CODE

setup_page_tables:
    mov eax, offset p3_table
    or eax, 0x3
    mov dword ptr [p4_table], eax

    mov eax, offset p2_table
    or eax, 0x3
    mov dword ptr [p3_table], eax

    mov ecx, 0

.map_p2_table:
    mov eax, 0x200000
    mul ecx
    or eax, 0x83
    mov dword ptr [p2_table + ecx * 8], eax
    mov dword ptr [p2_table + ecx * 8 + 4], edx

    inc ecx
    cmp ecx, 512
    jne .map_p2_table

    ret

enable_long_mode:
    mov eax, offset p4_table
    mov cr3, eax

    mov eax, cr4
    or eax, 1 << 5
    mov cr4, eax

    mov ecx, 0xc0000080
    rdmsr
    or eax, 1 << 8
    wrmsr

    mov eax, cr0
    or eax, 1 << 31
    mov cr0, eax

    ret

.code64
long_mode_start:
    mov ax, 0x10
    mov ds, ax
    mov es, ax
    mov fs, ax
    mov gs, ax
    mov ss, ax

    mov rsp, offset boot_stack_top
    xor rbp, rbp

    mov edi, dword ptr [multiboot_magic]
    mov esi, dword ptr [multiboot_info_addr]
    call kernel_main

.halt:
    hlt
    jmp .halt

.section .rodata.boot, "a"
.align 8
gdt64:
    .quad 0
.set GDT64_CODE, . - gdt64
    .quad 0x00209a0000000000
.set GDT64_DATA, . - gdt64
    .quad 0x0000920000000000
gdt64_pointer:
    .word . - gdt64 - 1
    .quad gdt64

.section .bss.boot, "aw", @nobits
.align 4096
.global boot_p4_table
boot_p4_table:
p4_table:
    .skip 4096
p3_table:
    .skip 4096
p2_table:
    .skip 4096

.align 16
multiboot_magic:
    .skip 4
multiboot_info_addr:
    .skip 4

.align 16
boot_stack_bottom:
    .skip 16384
boot_stack_top:
