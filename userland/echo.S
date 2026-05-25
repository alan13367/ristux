.section .text
.global _start
_start:
    mov $1, %rax
    mov $1, %rdi
    lea message(%rip), %rsi
    mov $(message_end - message), %rdx
    int $0x80

    mov $3, %rax
    xor %rdi, %rdi
    int $0x80

.halt:
    jmp .halt

.section .rodata
message:
    .ascii "echo: hello from real /bin/echo\n"
message_end:
