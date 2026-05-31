.section .text
.global _start
.type _start, @function
_start:
    xor %rbp, %rbp
    and $-16, %rsp

    mov %rdx, environ(%rip)
    call main

    mov %eax, %edi
    call exit

.Lhalt:
    hlt
    jmp .Lhalt

.section .data
.global environ
.type environ, @object
.size environ, 8
environ:
    .quad 0
