.section .text
.global _start
_start:
    cmp $2, %rdi
    jle .fail

    mov 8(%rsi), %r12
    xor %r13, %r13

.mode_loop:
    movzbq (%r12), %rax
    test %rax, %rax
    jz .chmod
    cmp $'0', %rax
    jb .fail
    cmp $'7', %rax
    ja .fail
    sub $'0', %rax
    shl $3, %r13
    add %rax, %r13
    inc %r12
    jmp .mode_loop

.chmod:
    mov $17, %rax
    mov 16(%rsi), %rdi
    mov %r13, %rsi
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
