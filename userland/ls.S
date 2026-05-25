.section .text
.global _start
_start:
    cmp $1, %rdi
    jg .use_arg
    lea root(%rip), %r12
    jmp .list

.use_arg:
    mov 8(%rsi), %r12

.list:
    mov $11, %rax
    mov %r12, %rdi
    lea buffer(%rip), %rsi
    mov $512, %rdx
    int $0x80
    test %rax, %rax
    js .fail

    mov %rax, %rdx
    mov $1, %rax
    mov $1, %rdi
    lea buffer(%rip), %rsi
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
root:
    .asciz "/"

.section .bss
.align 16
buffer:
    .skip 512
