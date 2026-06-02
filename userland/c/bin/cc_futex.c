#include <errno.h>
#include <linux/futex.h>
#include <stdio.h>
#include <sys/syscall.h>
#include <time.h>
#include <unistd.h>

static int futex_call(int *uaddr, int op, int val, const struct timespec *timeout) {
    return (int)syscall(SYS_futex, uaddr, op, val, timeout, 0, 0);
}

static int check_gettid(void) {
    if (gettid() != getpid() || syscall(SYS_gettid, 0, 0, 0, 0, 0, 0) != getpid()) {
        puts("cc_futex: gettid failed");
        return 1;
    }
    puts("cc_futex: gettid ok");
    return 0;
}

static int check_wait_mismatch(void) {
    int futex_word = 7;
    errno = 0;
    if (futex_call(&futex_word, FUTEX_WAIT_PRIVATE, 8, NULL) != -1 || errno != EAGAIN) {
        puts("cc_futex: mismatch failed");
        return 1;
    }
    puts("cc_futex: mismatch ok");
    return 0;
}

static int check_wait_timeout(void) {
    int futex_word = 11;
    struct timespec timeout = { 0, 1000000 };
    errno = 0;
    if (futex_call(&futex_word, FUTEX_WAIT, 11, &timeout) != -1 || errno != ETIMEDOUT) {
        puts("cc_futex: timeout failed");
        return 1;
    }
    puts("cc_futex: timeout ok");
    return 0;
}

static int check_wake(void) {
    int futex_word = 1;
    errno = 0;
    if (futex_call(&futex_word, FUTEX_WAKE_PRIVATE, 3, NULL) != 3) {
        puts("cc_futex: wake failed");
        return 1;
    }
    puts("cc_futex: wake ok");
    return 0;
}

static int check_nanosleep_invalid(void) {
    struct timespec req = { 0, 1000000000L };
    errno = 0;
    if (nanosleep(&req, NULL) != -1 || errno != EINVAL) {
        puts("cc_futex: nanosleep invalid failed");
        return 1;
    }
    puts("cc_futex: nanosleep invalid ok");
    return 0;
}

int main(void) {
    if (check_gettid() != 0 ||
        check_wait_mismatch() != 0 ||
        check_wait_timeout() != 0 ||
        check_wake() != 0 ||
        check_nanosleep_invalid() != 0) {
        return 1;
    }
    puts("cc_futex: done");
    return 0;
}
