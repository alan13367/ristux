.section .text
.global _start
.type _start, @function
_start:
    xor %rbp, %rbp
    and $-16, %rsp
    call __ristux_start

.Lhalt:
    hlt
    jmp .Lhalt
