.section .text
.global _start
_start:
    cmp $1, %rdi
    jle .fail

    mov $15, %rax
    mov 8(%rsi), %rdi
    int $0x80
    test %rax, %rax
    js .fail

    mov $3, %rax
    xor %rdi, %rdi
    int $0x80

.fail:
    mov $3, %rax
    mov $1, %rdi
    int $0x80

.halt:
    jmp .halt
