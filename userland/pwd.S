.section .text
.global _start
_start:
    mov $79, %rax
    lea buffer(%rip), %rdi
    mov $128, %rsi
    syscall
    test %rax, %rax
    js .fail

    lea buffer(%rip), %r8
    xor %rdx, %rdx
.strlen:
    cmpb $0, (%r8,%rdx,1)
    je .write_path
    inc %rdx
    cmp $128, %rdx
    jb .strlen

.write_path:
    mov $1, %rax
    mov $1, %rdi
    lea buffer(%rip), %rsi
    syscall

    mov $1, %rax
    mov $1, %rdi
    lea nl(%rip), %rsi
    mov $1, %rdx
    syscall

    mov $60, %rax
    xor %rdi, %rdi
    syscall

.fail:
    mov $60, %rax
    mov $1, %rdi
    syscall

.halt:
    jmp .halt

.section .rodata
nl:
    .ascii "\n"

.section .bss
.align 16
buffer:
    .skip 128
