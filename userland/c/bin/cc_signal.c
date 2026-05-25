#include <signal.h>
#include <stdio.h>
#include <string.h>
#include <unistd.h>

static volatile int saw_signal;

static void on_sigint(int signum) {
    if (signum == SIGINT) {
        const char *msg = "cc_signal: handler\n";
        write(1, msg, strlen(msg));
        saw_signal = 1;
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
    puts("cc_signal: after handler");
    return 0;
}
