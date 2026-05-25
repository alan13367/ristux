.section .text
.global _start
_start:
    mov $3, %rax
    mov $1, %rdi
    int $0x80

.halt:
    jmp .halt
