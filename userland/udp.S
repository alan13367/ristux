.section .text
.global _start
_start:
    mov $19, %rax
    mov $9000, %rdi
    int $0x80
    test %rax, %rax
    js .fail
    mov %rax, %r12

    mov $20, %rax
    mov %r12, %rdi
    mov $0x0a000202, %rsi
    mov $9001, %rdx
    lea payload(%rip), %r10
    mov $6, %r8
    int $0x80
    test %rax, %rax
    js .fail

    mov $21, %rax
    mov %r12, %rdi
    lea buffer(%rip), %rsi
    mov $64, %rdx
    int $0x80
    test %rax, %rax
    jle .fail
    mov %rax, %r13

    mov $1, %rax
    mov $1, %rdi
    lea prefix(%rip), %rsi
    mov $10, %rdx
    int $0x80

    mov $1, %rax
    mov $1, %rdi
    lea buffer(%rip), %rsi
    mov %r13, %rdx
    int $0x80

    mov $1, %rax
    mov $1, %rdi
    lea newline(%rip), %rsi
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
payload:
    .ascii "ristux"
prefix:
    .ascii "udp recv: "
newline:
    .ascii "\n"

.section .bss
.align 16
buffer:
    .skip 64
