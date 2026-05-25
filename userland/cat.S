.section .text
.global _start
_start:
    mov %rdi, %r14
    mov %rsi, %r15
    xor %r13, %r13
    cmp $1, %r14
    jle .use_stdin

    mov $8, %rax
    mov 8(%r15), %rdi
    xor %rsi, %rsi
    int $0x80
    test %rax, %rax
    js .fail
    mov %rax, %r12
    mov $1, %r13
    jmp .read_loop

.use_stdin:
    xor %r12, %r12

.read_loop:
    mov $2, %rax
    mov %r12, %rdi
    lea buffer(%rip), %rsi
    mov $128, %rdx
    int $0x80
    test %rax, %rax
    js .fail_close
    jz .done

    mov %rax, %rdx
    mov $1, %rax
    mov $1, %rdi
    lea buffer(%rip), %rsi
    int $0x80
    jmp .read_loop

.done:
    test %r13, %r13
    jz .exit_ok
    mov $9, %rax
    mov %r12, %rdi
    int $0x80

.exit_ok:
    mov $3, %rax
    xor %rdi, %rdi
    int $0x80

.fail_close:
    test %r13, %r13
    jz .fail
    mov $9, %rax
    mov %r12, %rdi
    int $0x80

.fail:
    mov $3, %rax
    mov $1, %rdi
    int $0x80

.halt:
    jmp .halt

.section .bss
.align 16
buffer:
    .skip 128
