#include <signal.h>
#include <errno.h>
#include <stdio.h>
#include <string.h>
#include <sys/syscall.h>
#include <sys/wait.h>
#include <unistd.h>

static volatile int saw_signal;
static volatile int saw_usr1;
static volatile int saw_usr2;
static volatile int saw_external_usr1;
static volatile int saw_sigchld;

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

static void on_usr2(int signum) {
    if (signum == SIGUSR2) {
        saw_usr2 = 1;
    }
}

static void on_external_usr1(int signum) {
    if (signum == SIGUSR1) {
        saw_external_usr1 = 1;
    }
}

static void on_sigchld(int signum) {
    if (signum == SIGCHLD) {
        saw_sigchld = 1;
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

static int check_ignored_signals(void) {
    sigset_t blocked;
    sigset_t oldmask;
    sigset_t pending;
    if (signal(SIGQUIT, SIG_IGN) == SIG_ERR ||
        sigemptyset(&blocked) < 0 ||
        sigaddset(&blocked, SIGQUIT) < 0 ||
        sigprocmask(SIG_BLOCK, &blocked, &oldmask) < 0) {
        puts("cc_signal: ignore setup failed");
        return 1;
    }
    if (raise(SIGQUIT) != 0) {
        puts("cc_signal: ignore raise failed");
        return 1;
    }
    if (sigpending(&pending) < 0 || sigismember(&pending, SIGQUIT) != 0) {
        puts("cc_signal: ignore pending failed");
        return 1;
    }
    if (sigprocmask(SIG_SETMASK, &oldmask, NULL) < 0) {
        puts("cc_signal: ignore mask restore failed");
        return 1;
    }
    if (signal(SIGTSTP, SIG_IGN) == SIG_ERR || raise(SIGTSTP) != 0) {
        puts("cc_signal: ignore stop failed");
        return 1;
    }
    puts("cc_signal: ignore ok");
    return 0;
}

static int check_sigprocmask_fault_preserves_oldset(void) {
    sigset_t set;
    sigemptyset(&set);

    sigset_t oldset = 0x5a5a5a5aUL;
    errno = 0;
    if (syscall(SYS_rt_sigprocmask, 99, (long)&set, (long)&oldset,
                sizeof(sigset_t), 0, 0) != -1 ||
        errno != EINVAL || oldset != 0x5a5a5a5aUL) {
        puts("cc_signal: sigprocmask invalid how failed");
        return 1;
    }

    oldset = 0xa5a5a5a5UL;
    errno = 0;
    if (syscall(SYS_rt_sigprocmask, SIG_BLOCK, (long)~0UL, (long)&oldset,
                sizeof(sigset_t), 0, 0) != -1 ||
        errno != EFAULT || oldset != 0xa5a5a5a5UL) {
        puts("cc_signal: sigprocmask fault failed");
        return 1;
    }

    puts("cc_signal: sigprocmask fault ok");
    return 0;
}

static int check_sigkill_uncatchable(void) {
    errno = 0;
    if (signal(SIGKILL, SIG_IGN) != SIG_ERR || errno != EINVAL) {
        puts("cc_signal: sigkill disposition failed");
        return 1;
    }

    pid_t child = fork();
    if (child < 0) {
        puts("cc_signal: sigkill fork failed");
        return 1;
    }
    if (child == 0) {
        raise(SIGKILL);
        _exit(42);
    }
    int status = 0;
    if (waitpid(child, &status, 0) != child || !WIFSIGNALED(status) ||
        WTERMSIG(status) != SIGKILL) {
        puts("cc_signal: sigkill delivery failed");
        return 1;
    }
    puts("cc_signal: sigkill ok");
    return 0;
}

static int check_invalid_raw_handler(void) {
    pid_t child = fork();
    if (child < 0) {
        puts("cc_signal: invalid handler fork failed");
        return 1;
    }
    if (child == 0) {
        void *kernel_handler = (void *)0x1234;
        if (syscall(SYS_rt_sigaction, SIGUSR1, (long)&kernel_handler, 0, 0, 0, 0) < 0) {
            _exit(2);
        }
        raise(SIGUSR1);
        _exit(42);
    }

    int status = 0;
    if (waitpid(child, &status, 0) != child || !WIFSIGNALED(status) ||
        WTERMSIG(status) != SIGUSR1) {
        puts("cc_signal: invalid handler delivery failed");
        return 1;
    }
    puts("cc_signal: invalid handler ok");
    return 0;
}

static int check_sigaction_fault_preserves_handler(void) {
    saw_usr2 = 0;
    if (signal(SIGUSR2, on_usr2) == SIG_ERR) {
        puts("cc_signal: sigaction fault setup failed");
        return 1;
    }
    void *kernel_handler = (void *)SIG_IGN;
    errno = 0;
    if (syscall(SYS_rt_sigaction, SIGUSR2, (long)&kernel_handler, 1, 0, 0, 0) != -1 ||
        errno != EFAULT) {
        puts("cc_signal: sigaction fault failed");
        return 1;
    }
    if (raise(SIGUSR2) != 0 || !saw_usr2) {
        puts("cc_signal: sigaction fault changed handler");
        return 1;
    }
    puts("cc_signal: sigaction fault ok");
    return 0;
}

static int check_sigstop_uncatchable(void) {
    errno = 0;
    if (signal(SIGSTOP, SIG_IGN) != SIG_ERR || errno != EINVAL) {
        puts("cc_signal: sigstop disposition failed");
        return 1;
    }

    pid_t child = fork();
    if (child < 0) {
        puts("cc_signal: sigstop fork failed");
        return 1;
    }
    if (child == 0) {
        sigset_t mask;
        if (sigemptyset(&mask) < 0 ||
            sigaddset(&mask, SIGSTOP) < 0 ||
            sigprocmask(SIG_BLOCK, &mask, NULL) < 0) {
            _exit(2);
        }
        raise(SIGSTOP);
        for (;;) {
            syscall(SYS_sched_yield);
        }
    }

    int status = 0;
    if (waitpid(child, &status, WUNTRACED) != child ||
        !WIFSTOPPED(status) || WSTOPSIG(status) != SIGSTOP) {
        kill(child, SIGKILL);
        waitpid(child, NULL, 0);
        puts("cc_signal: sigstop stop failed");
        return 1;
    }
    if (kill(child, SIGCONT) < 0 || kill(child, SIGTERM) < 0 ||
        waitpid(child, &status, 0) != child || !WIFSIGNALED(status) ||
        WTERMSIG(status) != SIGTERM) {
        kill(child, SIGKILL);
        waitpid(child, NULL, 0);
        puts("cc_signal: sigstop cleanup failed");
        return 1;
    }
    puts("cc_signal: sigstop ok");
    return 0;
}

static int check_external_signal_handler(void) {
    int pipefd[2];
    if (pipe(pipefd) < 0) {
        puts("cc_signal: external pipe failed");
        return 1;
    }
    pid_t child = fork();
    if (child < 0) {
        close(pipefd[0]);
        close(pipefd[1]);
        puts("cc_signal: external fork failed");
        return 1;
    }
    if (child == 0) {
        close(pipefd[0]);
        if (signal(SIGUSR1, on_external_usr1) == SIG_ERR) {
            _exit(2);
        }
        char ready = 'r';
        if (write(pipefd[1], &ready, 1) != 1) {
            _exit(3);
        }
        close(pipefd[1]);
        for (int i = 0; i < 200000 && !saw_external_usr1; i++) {
            syscall(SYS_sched_yield);
        }
        _exit(saw_external_usr1 ? 0 : 4);
    }

    close(pipefd[1]);
    char ready = 0;
    if (read(pipefd[0], &ready, 1) != 1) {
        close(pipefd[0]);
        kill(child, SIGKILL);
        waitpid(child, NULL, 0);
        puts("cc_signal: external ready failed");
        return 1;
    }
    close(pipefd[0]);
    if (kill(child, SIGUSR1) < 0) {
        kill(child, SIGKILL);
        waitpid(child, NULL, 0);
        puts("cc_signal: external send failed");
        return 1;
    }
    int status = 0;
    if (waitpid(child, &status, 0) != child || !WIFEXITED(status) ||
        WEXITSTATUS(status) != 0) {
        puts("cc_signal: external handler failed");
        return 1;
    }
    puts("cc_signal: external handler ok");
    return 0;
}

static int check_sigchld_disposition(void) {
    if (raise(SIGCHLD) != 0 || saw_sigchld) {
        puts("cc_signal: sigchld default failed");
        return 1;
    }
    if (signal(SIGCHLD, on_sigchld) == SIG_ERR) {
        puts("cc_signal: sigchld handler setup failed");
        return 1;
    }
    if (raise(SIGCHLD) != 0 || !saw_sigchld) {
        puts("cc_signal: sigchld handler failed");
        return 1;
    }
    puts("cc_signal: sigchld ok");
    return 0;
}

static int check_additional_signals(void) {
    if (signal(SIGUSR2, on_usr2) == SIG_ERR || raise(SIGUSR2) != 0 ||
        !saw_usr2) {
        puts("cc_signal: sigusr2 failed");
        return 1;
    }

    pid_t child = fork();
    if (child < 0) {
        puts("cc_signal: sigpipe fork failed");
        return 1;
    }
    if (child == 0) {
        raise(SIGPIPE);
        _exit(42);
    }
    int status = 0;
    if (waitpid(child, &status, 0) != child || !WIFSIGNALED(status) ||
        WTERMSIG(status) != SIGPIPE) {
        puts("cc_signal: sigpipe failed");
        return 1;
    }

    puts("cc_signal: extra signals ok");
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
    if (check_sigprocmask_fault_preserves_oldset() != 0) {
        return 1;
    }
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

    if (check_additional_signals() != 0 ||
        check_sigchld_disposition() != 0 ||
        check_external_signal_handler() != 0 ||
        check_invalid_raw_handler() != 0 ||
        check_sigaction_fault_preserves_handler() != 0 ||
        check_sigkill_uncatchable() != 0 ||
        check_sigstop_uncatchable() != 0 ||
        check_stop_wait_once() != 0 ||
        check_ignored_signals() != 0) {
        return 1;
    }

    puts("cc_signal: after handler");
    return 0;
}
