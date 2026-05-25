#include <stddef.h>
#include <stdio.h>

static volatile unsigned long sink;

static int touch_stack(int depth) {
    volatile unsigned char slab[1536];
    for (size_t i = 0; i < sizeof(slab); i += 113) {
        slab[i] = (unsigned char)(depth + (int)i);
    }
    sink += slab[(depth * 31) % sizeof(slab)];
    if (depth == 0) {
        return (int)sink;
    }
    int nested = touch_stack(depth - 1);
    sink += slab[(depth * 17) % sizeof(slab)];
    return nested + slab[0];
}

int main(void) {
    int value = touch_stack(48);
    if (sink == 0 && value == 0) {
        puts("cc_stack: stack touch failed");
        return 1;
    }
    puts("cc_stack: growth ok");
    puts("cc_stack: done");
    return 0;
}
