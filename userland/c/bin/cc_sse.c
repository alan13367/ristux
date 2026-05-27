#include <stdio.h>

static double scale(double value) {
    return value * 2.0 + 0.5;
}

int main(void) {
    volatile double input = 1.25;
    double result = scale(input);
    if (result < 2.99 || result > 3.01) {
        puts("cc_sse: double math failed");
        return 1;
    }

    puts("cc_sse: double math ok");
    return 0;
}
