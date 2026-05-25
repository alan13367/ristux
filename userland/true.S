.section .text
.global _start
_start:
    mov $3, %rax
    xor %rdi, %rdi
    int $0x80

.halt:
    jmp .halt
