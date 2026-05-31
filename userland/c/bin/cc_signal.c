#include <signal.h>
#include <stdio.h>
#include <string.h>
#include <unistd.h>

static volatile int saw_signal;
static volatile int saw_usr1;

static void on_sigint(int signum) {
    if (signum == SIGINT) {
        const char *msg = "cc_signal: handler\n";
        write(1, msg, strlen(msg));
        saw_signal = 1;
    }
}

static void on_usr1(int signum) {
    if (signum == SIGUSR1) {
        const char *msg = "cc_signal: raise handler\n";
        write(1, msg, strlen(msg));
        saw_usr1 = 1;
    }
}

int main(void) {
    if (signal(SIGINT, on_sigint) == SIG_ERR) {
        puts("cc_signal: signal failed");
        return 1;
    }
    if (kill(getpid(), SIGINT) != 0) {
        puts("cc_signal: kill failed");
        return 1;
    }
    if (!saw_signal) {
        puts("cc_signal: handler not seen");
        return 1;
    }
    if (signal(SIGUSR1, on_usr1) == SIG_ERR) {
        puts("cc_signal: raise signal failed");
        return 1;
    }
    if (raise(SIGUSR1) != 0 || !saw_usr1) {
        puts("cc_signal: raise failed");
        return 1;
    }
    saw_usr1 = 0;
    sigset_t blocked;
    sigset_t oldmask;
    if (sigemptyset(&blocked) < 0 ||
        sigaddset(&blocked, SIGUSR1) < 0 ||
        sigprocmask(SIG_BLOCK, &blocked, &oldmask) < 0) {
        puts("cc_signal: block failed");
        return 1;
    }
    if (raise(SIGUSR1) != 0 || saw_usr1) {
        puts("cc_signal: blocked delivery failed");
        return 1;
    }
    if (sigprocmask(SIG_SETMASK, &oldmask, NULL) < 0 || !saw_usr1) {
        puts("cc_signal: unblock delivery failed");
        return 1;
    }
    puts("cc_signal: mask ok");
    puts("cc_signal: after handler");
    return 0;
}
