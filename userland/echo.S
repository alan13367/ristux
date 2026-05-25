.section .text
.global _start
_start:
    mov %rdi, %r12
    mov %rsi, %r13
    cmp $1, %r12
    jle .newline

    mov $1, %r14

.arg_loop:
    cmp %r12, %r14
    jge .newline

    mov (%r13,%r14,8), %r15
    xor %rdx, %rdx

.len_loop:
    cmpb $0, (%r15,%rdx,1)
    je .write_arg
    inc %rdx
    jmp .len_loop

.write_arg:
    mov $1, %rax
    mov $1, %rdi
    mov %r15, %rsi
    int $0x80

    inc %r14
    cmp %r12, %r14
    jge .newline

    mov $1, %rax
    mov $1, %rdi
    lea space(%rip), %rsi
    mov $1, %rdx
    int $0x80
    jmp .arg_loop

.newline:
    mov $1, %rax
    mov $1, %rdi
    lea nl(%rip), %rsi
    mov $1, %rdx
    int $0x80

    mov $3, %rax
    xor %rdi, %rdi
    int $0x80

.halt:
    jmp .halt

.section .rodata
space:
    .ascii " "
nl:
    .ascii "\n"
