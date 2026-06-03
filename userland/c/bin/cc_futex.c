#include <errno.h>
#include <fcntl.h>
#include <linux/futex.h>
#include <limits.h>
#include <signal.h>
#include <stdio.h>
#include <sys/mman.h>
#include <sys/syscall.h>
#include <sys/wait.h>
#include <time.h>
#include <unistd.h>

static volatile int saw_async_signal;

static void on_async_signal(int signum) {
    if (signum == SIGUSR1) {
        saw_async_signal = 1;
    }
}

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

static int check_wait_timeout_overflow(void) {
    int futex_word = 12;
    struct timespec timeout = { LONG_MAX, 0 };
    errno = 0;
    if (futex_call(&futex_word, FUTEX_WAIT, 12, &timeout) != -1 || errno != EINVAL) {
        puts("cc_futex: timeout overflow failed");
        return 1;
    }
    puts("cc_futex: timeout overflow ok");
    return 0;
}

static int check_wake_empty(void) {
    int futex_word = 1;
    errno = 0;
    if (futex_call(&futex_word, FUTEX_WAKE_PRIVATE, 3, NULL) != 0) {
        puts("cc_futex: wake empty failed");
        return 1;
    }
    puts("cc_futex: wake empty ok");
    return 0;
}

static int check_wake_waiter(void) {
    const char *path = "/tmp/cc_futex_waiter.bin";
    unlink(path);

    int fd = open(path, O_CREAT | O_TRUNC | O_RDWR, 0600);
    if (fd < 0) {
        puts("cc_futex: wake open failed");
        return 1;
    }
    int initial = 1;
    if (write(fd, &initial, sizeof(initial)) != (ssize_t)sizeof(initial)) {
        close(fd);
        unlink(path);
        puts("cc_futex: wake seed failed");
        return 1;
    }

    int *word = mmap(NULL, 4096, PROT_READ | PROT_WRITE, MAP_SHARED, fd, 0);
    close(fd);
    if (word == MAP_FAILED) {
        unlink(path);
        puts("cc_futex: wake mmap failed");
        return 1;
    }
    *word = 1;

    int pipefd[2];
    if (pipe(pipefd) < 0) {
        munmap(word, 4096);
        unlink(path);
        puts("cc_futex: wake pipe failed");
        return 1;
    }

    pid_t child = fork();
    if (child < 0) {
        close(pipefd[0]);
        close(pipefd[1]);
        munmap(word, 4096);
        unlink(path);
        puts("cc_futex: wake fork failed");
        return 1;
    }
    if (child == 0) {
        close(pipefd[0]);
        char ready = 'r';
        if (write(pipefd[1], &ready, 1) != 1) {
            _exit(2);
        }
        close(pipefd[1]);

        struct timespec timeout = { 5, 0 };
        errno = 0;
        int rc = futex_call(word, FUTEX_WAIT, 1, &timeout);
        _exit(rc == 0 ? 0 : 3);
    }

    close(pipefd[1]);
    char ready = 0;
    if (read(pipefd[0], &ready, 1) != 1) {
        close(pipefd[0]);
        kill(child, SIGKILL);
        waitpid(child, NULL, 0);
        munmap(word, 4096);
        unlink(path);
        puts("cc_futex: wake ready failed");
        return 1;
    }
    close(pipefd[0]);

    int woke = 0;
    for (int i = 0; i < 200; i++) {
        errno = 0;
        int rc = futex_call(word, FUTEX_WAKE, 1, NULL);
        if (rc == 1) {
            woke = 1;
            break;
        }
        if (rc < 0) {
            break;
        }
        syscall(SYS_sched_yield);
    }
    if (!woke) {
        kill(child, SIGKILL);
        waitpid(child, NULL, 0);
        munmap(word, 4096);
        unlink(path);
        puts("cc_futex: wake waiter failed");
        return 1;
    }

    int status = 0;
    if (waitpid(child, &status, 0) != child || !WIFEXITED(status) ||
        WEXITSTATUS(status) != 0) {
        munmap(word, 4096);
        unlink(path);
        puts("cc_futex: wake waiter failed");
        return 1;
    }

    munmap(word, 4096);
    unlink(path);
    puts("cc_futex: wake waiter ok");
    return 0;
}

static int check_value_change_without_wake(void) {
    const char *path = "/tmp/cc_futex_no_wake.bin";
    unlink(path);

    int fd = open(path, O_CREAT | O_TRUNC | O_RDWR, 0600);
    if (fd < 0) {
        puts("cc_futex: no wake open failed");
        return 1;
    }
    int initial = 2;
    if (write(fd, &initial, sizeof(initial)) != (ssize_t)sizeof(initial)) {
        close(fd);
        unlink(path);
        puts("cc_futex: no wake seed failed");
        return 1;
    }

    int *word = mmap(NULL, 4096, PROT_READ | PROT_WRITE, MAP_SHARED, fd, 0);
    close(fd);
    if (word == MAP_FAILED) {
        unlink(path);
        puts("cc_futex: no wake mmap failed");
        return 1;
    }
    *word = 2;

    int pipefd[2];
    if (pipe(pipefd) < 0) {
        munmap(word, 4096);
        unlink(path);
        puts("cc_futex: no wake pipe failed");
        return 1;
    }

    pid_t child = fork();
    if (child < 0) {
        close(pipefd[0]);
        close(pipefd[1]);
        munmap(word, 4096);
        unlink(path);
        puts("cc_futex: no wake fork failed");
        return 1;
    }
    if (child == 0) {
        close(pipefd[0]);
        char ready = 'r';
        if (write(pipefd[1], &ready, 1) != 1) {
            _exit(2);
        }
        close(pipefd[1]);

        struct timespec timeout = { 0, 100000000L };
        errno = 0;
        int rc = futex_call(word, FUTEX_WAIT, 2, &timeout);
        _exit(rc == -1 && errno == ETIMEDOUT ? 0 : 3);
    }

    close(pipefd[1]);
    char ready = 0;
    if (read(pipefd[0], &ready, 1) != 1) {
        close(pipefd[0]);
        kill(child, SIGKILL);
        waitpid(child, NULL, 0);
        munmap(word, 4096);
        unlink(path);
        puts("cc_futex: no wake ready failed");
        return 1;
    }
    close(pipefd[0]);

    struct timespec delay = { 0, 20000000L };
    nanosleep(&delay, NULL);
    *word = 3;

    int status = 0;
    if (waitpid(child, &status, 0) != child || !WIFEXITED(status) ||
        WEXITSTATUS(status) != 0) {
        munmap(word, 4096);
        unlink(path);
        puts("cc_futex: no wake changed value failed");
        return 1;
    }

    munmap(word, 4096);
    unlink(path);
    puts("cc_futex: no wake changed value ok");
    return 0;
}

static int check_wait_interrupted_by_signal(void) {
    int pipefd[2];
    if (pipe(pipefd) < 0) {
        puts("cc_futex: signal wait pipe failed");
        return 1;
    }

    pid_t child = fork();
    if (child < 0) {
        close(pipefd[0]);
        close(pipefd[1]);
        puts("cc_futex: signal wait fork failed");
        return 1;
    }
    if (child == 0) {
        close(pipefd[0]);
        saw_async_signal = 0;
        if (signal(SIGUSR1, on_async_signal) == SIG_ERR) {
            _exit(2);
        }
        char ready = 'r';
        if (write(pipefd[1], &ready, 1) != 1) {
            _exit(3);
        }
        close(pipefd[1]);

        int futex_word = 77;
        struct timespec timeout = { 30, 0 };
        errno = 0;
        int rc = futex_call(&futex_word, FUTEX_WAIT, 77, &timeout);
        if (rc != -1) {
            _exit(10);
        }
        if (errno != EINTR) {
            _exit(20);
        }
        if (!saw_async_signal) {
            _exit(30);
        }

        timeout.tv_sec = 0;
        timeout.tv_nsec = 1000000L;
        errno = 0;
        rc = futex_call(&futex_word, FUTEX_WAIT, 78, &timeout);
        if (rc != -1 || errno != EAGAIN) {
            _exit(40);
        }
        _exit(0);
    }

    close(pipefd[1]);
    char ready = 0;
    if (read(pipefd[0], &ready, 1) != 1) {
        close(pipefd[0]);
        kill(child, SIGKILL);
        waitpid(child, NULL, 0);
        puts("cc_futex: signal wait ready failed");
        return 1;
    }
    close(pipefd[0]);

    for (int i = 0; i < 100; i++) {
        syscall(SYS_sched_yield);
    }

    if (kill(child, SIGUSR1) < 0) {
        kill(child, SIGKILL);
        waitpid(child, NULL, 0);
        puts("cc_futex: signal wait send failed");
        return 1;
    }

    int status = 0;
    for (int i = 0; i < 200; i++) {
        pid_t waited = waitpid(child, &status, WNOHANG);
        if (waited == child) {
            if (WIFEXITED(status) && WEXITSTATUS(status) == 0) {
                puts("cc_futex: signal wait ok");
                return 0;
            }
            printf("cc_futex: signal wait child status=%d\n", status);
            return 1;
        }
        if (waited < 0) {
            puts("cc_futex: signal wait wait failed");
            return 1;
        }
        syscall(SYS_sched_yield);
    }

    kill(child, SIGKILL);
    waitpid(child, NULL, 0);
    puts("cc_futex: signal wait timeout");
    return 1;
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

static int check_nanosleep_overflow(void) {
    struct timespec req = { LONG_MAX, 0 };
    errno = 0;
    if (nanosleep(&req, NULL) != -1 || errno != EINVAL) {
        puts("cc_futex: nanosleep overflow failed");
        return 1;
    }
    puts("cc_futex: nanosleep overflow ok");
    return 0;
}

static int check_nanosleep_yields(void) {
    int pipefd[2];
    if (pipe(pipefd) < 0) {
        puts("cc_futex: nanosleep yield pipe failed");
        return 1;
    }

    pid_t child = fork();
    if (child < 0) {
        close(pipefd[0]);
        close(pipefd[1]);
        puts("cc_futex: nanosleep yield fork failed");
        return 1;
    }
    if (child == 0) {
        close(pipefd[0]);
        char ready = 'r';
        if (write(pipefd[1], &ready, 1) != 1) {
            _exit(2);
        }
        struct timespec req = { 0, 200000000L };
        errno = 0;
        if (nanosleep(&req, NULL) != 0) {
            _exit(3);
        }
        close(pipefd[1]);
        _exit(0);
    }

    close(pipefd[1]);
    char ready = 0;
    if (read(pipefd[0], &ready, 1) != 1) {
        close(pipefd[0]);
        kill(child, SIGKILL);
        waitpid(child, NULL, 0);
        puts("cc_futex: nanosleep yield ready failed");
        return 1;
    }
    close(pipefd[0]);

    int status = 0;
    errno = 0;
    if (waitpid(child, &status, WNOHANG) != 0) {
        kill(child, SIGKILL);
        waitpid(child, NULL, 0);
        puts("cc_futex: nanosleep yield wait failed");
        return 1;
    }
    if (waitpid(child, &status, 0) != child || !WIFEXITED(status) ||
        WEXITSTATUS(status) != 0) {
        puts("cc_futex: nanosleep yield child failed");
        return 1;
    }

    puts("cc_futex: nanosleep yield ok");
    return 0;
}

static int check_nanosleep_interrupt_remaining(void) {
    int pipefd[2];
    if (pipe(pipefd) < 0) {
        puts("cc_futex: nanosleep interrupt pipe failed");
        return 1;
    }

    pid_t child = fork();
    if (child < 0) {
        close(pipefd[0]);
        close(pipefd[1]);
        puts("cc_futex: nanosleep interrupt fork failed");
        return 1;
    }
    if (child == 0) {
        close(pipefd[0]);
        saw_async_signal = 0;
        if (signal(SIGUSR1, on_async_signal) == SIG_ERR) {
            _exit(2);
        }
        char ready = 'r';
        if (write(pipefd[1], &ready, 1) != 1) {
            _exit(3);
        }
        close(pipefd[1]);

        struct timespec req = { 30, 0 };
        struct timespec rem = { 0, 0 };
        errno = 0;
        int rc = nanosleep(&req, &rem);
        if (rc != -1) {
            _exit(10);
        }
        if (errno != EINTR) {
            _exit(20);
        }
        if (!saw_async_signal) {
            _exit(30);
        }
        if (rem.tv_sec < 0 || rem.tv_nsec < 0 || rem.tv_nsec >= 1000000000L) {
            _exit(40);
        }
        if (rem.tv_sec == 0 && rem.tv_nsec == 0) {
            _exit(50);
        }
        if (rem.tv_sec > req.tv_sec ||
            (rem.tv_sec == req.tv_sec && rem.tv_nsec > req.tv_nsec)) {
            _exit(60);
        }
        _exit(0);
    }

    close(pipefd[1]);
    char ready = 0;
    if (read(pipefd[0], &ready, 1) != 1) {
        close(pipefd[0]);
        kill(child, SIGKILL);
        waitpid(child, NULL, 0);
        puts("cc_futex: nanosleep interrupt ready failed");
        return 1;
    }
    close(pipefd[0]);

    for (int i = 0; i < 100; i++) {
        syscall(SYS_sched_yield);
    }

    if (kill(child, SIGUSR1) < 0) {
        kill(child, SIGKILL);
        waitpid(child, NULL, 0);
        puts("cc_futex: nanosleep interrupt send failed");
        return 1;
    }

    int status = 0;
    for (int i = 0; i < 200; i++) {
        pid_t waited = waitpid(child, &status, WNOHANG);
        if (waited == child) {
            if (WIFEXITED(status) && WEXITSTATUS(status) == 0) {
                puts("cc_futex: nanosleep interrupt ok");
                return 0;
            }
            printf("cc_futex: nanosleep interrupt child status=%d\n", status);
            return 1;
        }
        if (waited < 0) {
            puts("cc_futex: nanosleep interrupt wait failed");
            return 1;
        }
        syscall(SYS_sched_yield);
    }

    kill(child, SIGKILL);
    waitpid(child, NULL, 0);
    puts("cc_futex: nanosleep interrupt timeout");
    return 1;
}

int main(void) {
    if (check_gettid() != 0 ||
        check_wait_mismatch() != 0 ||
        check_wait_timeout() != 0 ||
        check_wait_timeout_overflow() != 0 ||
        check_wake_empty() != 0 ||
        check_wake_waiter() != 0 ||
        check_value_change_without_wake() != 0 ||
        check_wait_interrupted_by_signal() != 0 ||
        check_nanosleep_invalid() != 0 ||
        check_nanosleep_overflow() != 0 ||
        check_nanosleep_yields() != 0 ||
        check_nanosleep_interrupt_remaining() != 0) {
        return 1;
    }
    puts("cc_futex: done");
    return 0;
}
