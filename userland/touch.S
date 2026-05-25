.section .text
.global _start
_start:
    cmp $1, %rdi
    jle .fail

    mov 8(%rsi), %rdi
    mov $14, %rax
    int $0x80
    test %rax, %rax
    js .fail
    mov %rax, %r12

    mov $9, %rax
    mov %r12, %rdi
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
