#include <signal.h>
#include <errno.h>
#include <stdio.h>
#include <string.h>
#include <sys/syscall.h>
#include <sys/wait.h>
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

static int check_stop_wait_once(void) {
    pid_t child = fork();
    if (child < 0) {
        puts("cc_signal: stop fork failed");
        return 1;
    }
    if (child == 0) {
        for (;;) {
        }
    }

    if (kill(child, SIGTSTP) < 0) {
        puts("cc_signal: stop send failed");
        kill(child, SIGKILL);
        return 1;
    }

    int status = 0;
    if (waitpid(child, &status, WUNTRACED) != child ||
        !WIFSTOPPED(status) || WSTOPSIG(status) != SIGTSTP) {
        puts("cc_signal: stop wait failed");
        kill(child, SIGKILL);
        waitpid(child, NULL, 0);
        return 1;
    }

    errno = 0;
    if (waitpid(child, &status, WNOHANG | WUNTRACED) != 0) {
        puts("cc_signal: stop duplicate wait failed");
        kill(child, SIGKILL);
        waitpid(child, NULL, 0);
        return 1;
    }

    if (kill(child, SIGCONT) < 0 || kill(child, SIGTERM) < 0 ||
        waitpid(child, &status, 0) != child || !WIFSIGNALED(status) ||
        WTERMSIG(status) != SIGTERM) {
        puts("cc_signal: stop cleanup failed");
        kill(child, SIGKILL);
        waitpid(child, NULL, 0);
        return 1;
    }

    puts("cc_signal: stop wait once ok");
    return 0;
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
    struct sigaction queried;
    if (sigaction(SIGUSR1, NULL, &queried) < 0 ||
        queried.sa_handler != on_usr1) {
        puts("cc_signal: query failed");
        return 1;
    }
    saw_usr1 = 0;
    if (raise(SIGUSR1) != 0 || !saw_usr1) {
        puts("cc_signal: query preserved failed");
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
    sigset_t pending;
    if (sigpending(&pending) < 0 || sigismember(&pending, SIGUSR1) != 1) {
        puts("cc_signal: pending failed");
        return 1;
    }
    if (sigprocmask(SIG_SETMASK, &oldmask, NULL) < 0 || !saw_usr1) {
        puts("cc_signal: unblock delivery failed");
        return 1;
    }
    if (sigpending(&pending) < 0 || sigismember(&pending, SIGUSR1) != 0) {
        puts("cc_signal: pending clear failed");
        return 1;
    }
    puts("cc_signal: mask ok");
    errno = 0;
    if (kill(getpid(), 256) != -1 || errno != EINVAL) {
        puts("cc_signal: invalid signal failed");
        return 1;
    }
    unsigned long fake_frame[20];
    memset(fake_frame, 0, sizeof(fake_frame));
    errno = 0;
    if (syscall(SYS_rt_sigreturn, (long)fake_frame, 0, 0, 0, 0, 0) != -1 ||
        errno != EINVAL) {
        puts("cc_signal: invalid sigreturn failed");
        return 1;
    }
    puts("cc_signal: sigreturn validation ok");

    pid_t parent = getpid();
    pid_t child = fork();
    if (child < 0) {
        puts("cc_signal: permission fork failed");
        return 1;
    }
    if (child == 0) {
        if (setuid(100) < 0) {
            _exit(2);
        }
        errno = 0;
        if (kill(parent, 0) != -1 || errno != EPERM) {
            _exit(3);
        }
        errno = 0;
        if (kill(parent, SIGUSR1) != -1 || errno != EPERM) {
            _exit(4);
        }
        _exit(0);
    }
    int status = 0;
    if (waitpid(child, &status, 0) != child || !WIFEXITED(status) ||
        WEXITSTATUS(status) != 0) {
        puts("cc_signal: permission failed");
        return 1;
    }
    puts("cc_signal: permission ok");

    pid_t fatal = fork();
    if (fatal < 0) {
        puts("cc_signal: default fork failed");
        return 1;
    }
    if (fatal == 0) {
        raise(SIGTERM);
        _exit(42);
    }
    status = 0;
    if (waitpid(fatal, &status, 0) != fatal || !WIFSIGNALED(status) ||
        WTERMSIG(status) != SIGTERM) {
        puts("cc_signal: default disposition failed");
        return 1;
    }
    puts("cc_signal: default disposition ok");

    if (check_stop_wait_once() != 0) {
        return 1;
    }

    puts("cc_signal: after handler");
    return 0;
}
