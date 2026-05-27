#include <stdio.h>

#include "message.h"

int main(int argc, char **argv) {
    int i;

    puts(ristux_hello_message());
    printf("argc=%d\n", argc);
    for (i = 1; i < argc; i++) {
        printf("arg[%d]=%s\n", i, argv[i]);
    }
    return 0;
}
