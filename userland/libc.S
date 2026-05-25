.section .text

.global write
.type write, @function
write:
    mov $1, %rax
    int $0x80
    ret

.global read
.type read, @function
read:
    mov $2, %rax
    int $0x80
    ret

.global exit
.type exit, @function
exit:
    mov $3, %rax
    int $0x80
.exit_halt:
    jmp .exit_halt

.global time
.type time, @function
time:
    mov $7, %rax
    int $0x80
    ret

.global open
.type open, @function
open:
    mov $8, %rax
    int $0x80
    ret

.global close
.type close, @function
close:
    mov $9, %rax
    int $0x80
    ret

.global getcwd
.type getcwd, @function
getcwd:
    mov $10, %rax
    int $0x80
    ret

.global listdir
.type listdir, @function
listdir:
    mov $11, %rax
    int $0x80
    ret
