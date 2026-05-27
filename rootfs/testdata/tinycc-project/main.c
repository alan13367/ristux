extern int puts(const char *);

#include "util.h"

int main(void) {
    puts(message());
    return value() == 42 ? 0 : 1;
}
