.section .text
.global _start
_start:
    mov $10, %rax
    lea buffer(%rip), %rdi
    mov $128, %rsi
    int $0x80
    test %rax, %rax
    js .fail

    mov %rax, %rdx
    mov $1, %rax
    mov $1, %rdi
    lea buffer(%rip), %rsi
    int $0x80

    mov $1, %rax
    mov $1, %rdi
    lea nl(%rip), %rsi
    mov $1, %rdx
    int $0x80

    mov $3, %rax
    xor %rdi, %rdi
    int $0x80

.fail:
    mov $3, %rax
    mov $1, %rdi
    int $0x80

.halt:
    jmp .halt

.section .rodata
nl:
    .ascii "\n"

.section .bss
.align 16
buffer:
    .skip 128
