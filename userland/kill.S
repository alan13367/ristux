.section .text
.global _start
_start:
    cmp $1, %rdi
    jle .fail

    mov 8(%rsi), %r12
    xor %r13, %r13

.pid_loop:
    movzbq (%r12), %rax
    test %rax, %rax
    jz .parse_signal
    cmp $'0', %rax
    jb .fail
    cmp $'9', %rax
    ja .fail
    sub $'0', %rax
    imul $10, %r13, %r13
    add %rax, %r13
    inc %r12
    jmp .pid_loop

.parse_signal:
    mov $15, %r14
    cmp $2, %rdi
    jle .kill
    mov 16(%rsi), %r12
    xor %r14, %r14

.signal_loop:
    movzbq (%r12), %rax
    test %rax, %rax
    jz .kill
    cmp $'0', %rax
    jb .fail
    cmp $'9', %rax
    ja .fail
    sub $'0', %rax
    imul $10, %r14, %r14
    add %rax, %r14
    inc %r12
    jmp .signal_loop

.kill:
    mov $18, %rax
    mov %r13, %rdi
    mov %r14, %rsi
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
