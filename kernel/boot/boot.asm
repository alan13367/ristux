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

    mov eax, offset p3_kernel_table
    or eax, 0x3
    mov dword ptr [p4_table + 511 * 8], eax

    mov eax, offset p2_table
    or eax, 0x3
    mov dword ptr [p3_table], eax
    mov dword ptr [p3_kernel_table + 510 * 8], eax

    mov eax, offset p2_table_1g
    or eax, 0x3
    mov dword ptr [p3_table + 1 * 8], eax

    mov eax, offset p2_kernel_table_1g
    or eax, 0x3
    mov dword ptr [p3_kernel_table + 511 * 8], eax

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

    mov ecx, 0

.map_p2_table_1g:
    mov eax, 0x200000
    mul ecx
    add eax, 0x40000000
    adc edx, 0
    or eax, 0x83
    mov dword ptr [p2_table_1g + ecx * 8], eax
    mov dword ptr [p2_table_1g + ecx * 8 + 4], edx
    mov dword ptr [p2_kernel_table_1g + ecx * 8], eax
    mov dword ptr [p2_kernel_table_1g + ecx * 8 + 4], edx

    inc ecx
    cmp ecx, 512
    jne .map_p2_table_1g

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
p3_kernel_table:
    .skip 4096
p2_table:
    .skip 4096
p2_table_1g:
    .skip 4096
p2_kernel_table_1g:
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

.section .ap_trampoline, "a"
.align 16
.global __ap_trampoline_start
.global __ap_trampoline_end

.set AP_TRAMPOLINE_BASE, 0x8000
.set AP_GDT32_CODE, 0x08
.set AP_GDT32_DATA, 0x10
.set AP_GDT64_CODE, 0x18
.set AP_GDT64_DATA, 0x20

.code16
__ap_trampoline_start:
    cli
    mov ax, cs
    mov ds, ax
    lgdt [AP_GDT_DESCRIPTOR_OFFSET]

    mov eax, cr0
    or eax, 1
    mov cr0, eax

    .byte 0x66, 0xea
    .long AP_TRAMPOLINE_BASE + ap_protected - __ap_trampoline_start
    .word AP_GDT32_CODE

.code32
ap_protected:
    mov ax, AP_GDT32_DATA
    mov ds, ax
    mov es, ax
    mov fs, ax
    mov gs, ax
    mov ss, ax

    mov eax, offset boot_p4_table
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

    .byte 0xea
    .long AP_TRAMPOLINE_BASE + ap_long_mode - __ap_trampoline_start
    .word AP_GDT64_CODE

.code64
ap_long_mode:
    mov ax, AP_GDT64_DATA
    mov ds, ax
    mov es, ax
    mov fs, ax
    mov gs, ax
    mov ss, ax

    mov rsp, offset SMP_AP_BOOT_STACK + 4096
    xor rbp, rbp
    movabs rax, offset smp_ap_start
    call rax

.ap_halt:
    hlt
    jmp .ap_halt

.align 8
ap_gdt:
    .quad 0
    .quad 0x00cf9a000000ffff
    .quad 0x00cf92000000ffff
    .quad 0x00209a0000000000
    .quad 0x0000920000000000
ap_gdt_descriptor:
    .word ap_gdt_descriptor - ap_gdt - 1
    .long AP_TRAMPOLINE_BASE + ap_gdt - __ap_trampoline_start

__ap_trampoline_end:

.set AP_GDT_DESCRIPTOR_OFFSET, ap_gdt_descriptor - __ap_trampoline_start

.code64
