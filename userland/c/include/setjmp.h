#ifndef _RISTUX_SETJMP_H
#define _RISTUX_SETJMP_H

#include <stdint.h>

typedef struct {
    uint64_t rbx;
    uint64_t rbp;
    uint64_t r12;
    uint64_t r13;
    uint64_t r14;
    uint64_t r15;
    uint64_t rsp;
    uint64_t rip;
} __ristux_jmp_buf;

typedef __ristux_jmp_buf jmp_buf[1];
typedef __ristux_jmp_buf sigjmp_buf[1];

int setjmp(jmp_buf env);
void longjmp(jmp_buf env, int value);

#define sigsetjmp(env, savemask) setjmp(env)
#define siglongjmp(env, value) longjmp(env, value)

#endif
