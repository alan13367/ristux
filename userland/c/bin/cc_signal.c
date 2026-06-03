#include <signal.h>
#include <errno.h>
#include <stdio.h>
#include <stdint.h>
#include <string.h>
#include <sys/syscall.h>
#include <sys/wait.h>
#include <time.h>
#include <unistd.h>

static volatile int saw_signal;
static volatile int saw_usr1;
static volatile int saw_usr2;
static volatile int saw_external_usr1;
static volatile int saw_entry_usr1;
static volatile int saw_interrupt_usr1;
static volatile int saw_sigchld;
static volatile int saw_cont;
static volatile int handler_mask_in_usr1;
static volatile int handler_mask_nested_usr2;
static volatile int handler_mask_after_usr2;
static volatile unsigned long entry_spin_sink;

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

static void on_entry_usr1(int signum) {
    if (signum == SIGUSR1) {
        saw_entry_usr1 = 1;
    }
}

static void on_interrupt_usr1(int signum) {
    if (signum == SIGUSR1) {
        saw_interrupt_usr1 = 1;
    }
}

static void on_sigchld(int signum) {
    if (signum == SIGCHLD) {
        saw_sigchld = 1;
    }
}

static void on_cont(int signum) {
    if (signum == SIGCONT) {
        saw_cont = 1;
    }
}

static void on_blocked_usr1(int signum) {
    _exit(signum == SIGUSR1 ? 0 : 5);
}

static void on_handler_mask_usr2(int signum) {
    if (signum == SIGUSR2) {
        if (handler_mask_in_usr1) {
            handler_mask_nested_usr2 = 1;
        } else {
            handler_mask_after_usr2 = 1;
        }
    }
}

static void on_handler_mask_usr1(int signum) {
    if (signum != SIGUSR1) {
        return;
    }
    handler_mask_in_usr1 = 1;
    raise(SIGUSR2);
    handler_mask_in_usr1 = 0;
}

static int check_handler_signal_mask(void) {
    struct sigaction usr1;
    struct sigaction usr2;
    struct sigaction old_usr1;
    struct sigaction old_usr2;
    struct sigaction queried;

    sigemptyset(&usr1.sa_mask);
    sigaddset(&usr1.sa_mask, SIGUSR2);
    usr1.sa_handler = on_handler_mask_usr1;
    usr1.sa_flags = 0;
    sigemptyset(&usr2.sa_mask);
    usr2.sa_handler = on_handler_mask_usr2;
    usr2.sa_flags = 0;

    handler_mask_in_usr1 = 0;
    handler_mask_nested_usr2 = 0;
    handler_mask_after_usr2 = 0;
    if (sigaction(SIGUSR2, &usr2, &old_usr2) < 0 ||
        sigaction(SIGUSR1, &usr1, &old_usr1) < 0 ||
        sigaction(SIGUSR1, NULL, &queried) < 0 ||
        sigismember(&queried.sa_mask, SIGUSR2) != 1) {
        puts("cc_signal: handler mask setup failed");
        return 1;
    }

    if (raise(SIGUSR1) != 0) {
        puts("cc_signal: handler mask raise failed");
        return 1;
    }
    for (int i = 0; i < 16 && !handler_mask_after_usr2; i++) {
        syscall(SYS_sched_yield);
    }
    if (handler_mask_nested_usr2 || !handler_mask_after_usr2) {
        puts("cc_signal: handler mask delivery failed");
        return 1;
    }
    if (sigaction(SIGUSR1, &old_usr1, NULL) < 0 ||
        sigaction(SIGUSR2, &old_usr2, NULL) < 0) {
        puts("cc_signal: handler mask restore failed");
        return 1;
    }
    puts("cc_signal: handler mask ok");
    return 0;
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

static int check_multiple_pending_signals(void) {
    sigset_t blocked;
    sigset_t oldmask;
    sigset_t pending;
    if (signal(SIGUSR1, on_usr1) == SIG_ERR ||
        signal(SIGUSR2, on_usr2) == SIG_ERR ||
        sigemptyset(&blocked) < 0 ||
        sigaddset(&blocked, SIGUSR1) < 0 ||
        sigaddset(&blocked, SIGUSR2) < 0 ||
        sigprocmask(SIG_BLOCK, &blocked, &oldmask) < 0) {
        puts("cc_signal: pending multi setup failed");
        return 1;
    }

    saw_usr1 = 0;
    saw_usr2 = 0;
    if (raise(SIGUSR1) != 0 || raise(SIGUSR2) != 0 || saw_usr1 || saw_usr2) {
        puts("cc_signal: pending multi queue failed");
        return 1;
    }
    if (sigpending(&pending) < 0 ||
        sigismember(&pending, SIGUSR1) != 1 ||
        sigismember(&pending, SIGUSR2) != 1) {
        puts("cc_signal: pending multi bits failed");
        return 1;
    }
    if (sigprocmask(SIG_SETMASK, &oldmask, NULL) < 0) {
        puts("cc_signal: pending multi unblock failed");
        return 1;
    }
    (void)getpid();
    if (!saw_usr1 || !saw_usr2) {
        puts("cc_signal: pending multi delivery failed");
        return 1;
    }
    if (sigpending(&pending) < 0 ||
        sigismember(&pending, SIGUSR1) != 0 ||
        sigismember(&pending, SIGUSR2) != 0) {
        puts("cc_signal: pending multi clear failed");
        return 1;
    }

    puts("cc_signal: pending multi ok");
    return 0;
}

static int check_exec_signal_dispositions(void) {
    pid_t child = fork();
    if (child < 0) {
        puts("cc_signal: exec disposition fork failed");
        return 1;
    }
    if (child == 0) {
        if (signal(SIGUSR1, on_usr1) == SIG_ERR) {
            _exit(2);
        }
        char *argv[] = { "/bin/cc_signal", "exec-reset-probe", NULL };
        char *envp[] = { NULL };
        execve("/bin/cc_signal", argv, envp);
        _exit(3);
    }

    int status = 0;
    if (waitpid(child, &status, 0) != child || !WIFSIGNALED(status) ||
        WTERMSIG(status) != SIGUSR1) {
        puts("cc_signal: exec disposition reset failed");
        return 1;
    }

    child = fork();
    if (child < 0) {
        puts("cc_signal: exec ignore fork failed");
        return 1;
    }
    if (child == 0) {
        if (signal(SIGUSR2, SIG_IGN) == SIG_ERR) {
            _exit(4);
        }
        char *argv[] = { "/bin/cc_signal", "exec-ignore-probe", NULL };
        char *envp[] = { NULL };
        execve("/bin/cc_signal", argv, envp);
        _exit(5);
    }

    status = 0;
    if (waitpid(child, &status, 0) != child || !WIFEXITED(status) ||
        WEXITSTATUS(status) != 0) {
        puts("cc_signal: exec ignore failed");
        return 1;
    }

    puts("cc_signal: exec disposition ok");
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
        struct sigaction raw_action;
        memset(&raw_action, 0, sizeof(raw_action));
        uintptr_t raw_handler = 0x1234;
        memcpy(&raw_action.sa_handler, &raw_handler, sizeof(raw_handler));
        if (syscall(SYS_rt_sigaction, SIGUSR1, (long)&raw_action, 0, 0, 0, 0) < 0) {
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
    struct sigaction raw_action;
    memset(&raw_action, 0, sizeof(raw_action));
    raw_action.sa_handler = SIG_IGN;
    errno = 0;
    if (syscall(SYS_rt_sigaction, SIGUSR2, (long)&raw_action, 1, 0, 0, 0) != -1 ||
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

static int check_external_sigstop(void) {
    pid_t child = fork();
    if (child < 0) {
        puts("cc_signal: external sigstop fork failed");
        return 1;
    }
    if (child == 0) {
        for (;;) {
            syscall(SYS_sched_yield);
        }
    }

    if (kill(child, SIGSTOP) < 0) {
        kill(child, SIGKILL);
        waitpid(child, NULL, 0);
        puts("cc_signal: external sigstop send failed");
        return 1;
    }

    int status = 0;
    if (waitpid(child, &status, WUNTRACED) != child ||
        !WIFSTOPPED(status) || WSTOPSIG(status) != SIGSTOP) {
        kill(child, SIGKILL);
        waitpid(child, NULL, 0);
        puts("cc_signal: external sigstop wait failed");
        return 1;
    }
    if (kill(child, SIGCONT) < 0 || kill(child, SIGTERM) < 0 ||
        waitpid(child, &status, 0) != child || !WIFSIGNALED(status) ||
        WTERMSIG(status) != SIGTERM) {
        kill(child, SIGKILL);
        waitpid(child, NULL, 0);
        puts("cc_signal: external sigstop cleanup failed");
        return 1;
    }

    puts("cc_signal: external sigstop ok");
    return 0;
}

static int check_standard_signal_defaults(void) {
    pid_t child = fork();
    if (child < 0) {
        puts("cc_signal: standard fatal fork failed");
        return 1;
    }
    if (child == 0) {
        raise(SIGALRM);
        _exit(42);
    }
    int status = 0;
    if (waitpid(child, &status, 0) != child || !WIFSIGNALED(status) ||
        WTERMSIG(status) != SIGALRM) {
        puts("cc_signal: standard fatal default failed");
        return 1;
    }

    if (raise(SIGWINCH) != 0) {
        puts("cc_signal: standard ignore default failed");
        return 1;
    }

    child = fork();
    if (child < 0) {
        puts("cc_signal: standard stop fork failed");
        return 1;
    }
    if (child == 0) {
        for (;;) {
            syscall(SYS_sched_yield);
        }
    }
    if (kill(child, SIGTTIN) < 0) {
        kill(child, SIGKILL);
        waitpid(child, NULL, 0);
        puts("cc_signal: standard stop send failed");
        return 1;
    }
    status = 0;
    if (waitpid(child, &status, WUNTRACED) != child ||
        !WIFSTOPPED(status) || WSTOPSIG(status) != SIGTTIN) {
        kill(child, SIGKILL);
        waitpid(child, NULL, 0);
        puts("cc_signal: standard stop default failed");
        return 1;
    }
    if (kill(child, SIGCONT) < 0 || kill(child, SIGTERM) < 0 ||
        waitpid(child, &status, 0) != child || !WIFSIGNALED(status) ||
        WTERMSIG(status) != SIGTERM) {
        kill(child, SIGKILL);
        waitpid(child, NULL, 0);
        puts("cc_signal: standard stop cleanup failed");
        return 1;
    }

    puts("cc_signal: standard defaults ok");
    return 0;
}

static int check_sigcont_handler(void) {
    if (raise(SIGCONT) != 0) {
        puts("cc_signal: sigcont default failed");
        return 1;
    }

    sigset_t blocked;
    sigset_t oldmask;
    sigset_t pending;
    if (sigemptyset(&blocked) < 0 ||
        sigaddset(&blocked, SIGTSTP) < 0 ||
        sigprocmask(SIG_BLOCK, &blocked, &oldmask) < 0) {
        puts("cc_signal: sigcont pending setup failed");
        return 1;
    }
    if (raise(SIGTSTP) != 0 ||
        sigpending(&pending) < 0 ||
        sigismember(&pending, SIGTSTP) != 1) {
        puts("cc_signal: sigcont pending stop failed");
        return 1;
    }
    if (raise(SIGCONT) != 0 ||
        sigpending(&pending) < 0 ||
        sigismember(&pending, SIGTSTP) != 0) {
        puts("cc_signal: sigcont pending clear failed");
        return 1;
    }
    if (sigprocmask(SIG_SETMASK, &oldmask, NULL) < 0) {
        puts("cc_signal: sigcont mask restore failed");
        return 1;
    }

    saw_cont = 0;
    if (signal(SIGCONT, on_cont) == SIG_ERR ||
        raise(SIGCONT) != 0 || !saw_cont) {
        puts("cc_signal: sigcont handler failed");
        return 1;
    }
    if (signal(SIGCONT, SIG_DFL) == SIG_ERR) {
        puts("cc_signal: sigcont restore failed");
        return 1;
    }
    puts("cc_signal: sigcont handler ok");
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

static int check_signal_wakes_blocked_syscall(void) {
    int pipefd[2];
    if (pipe(pipefd) < 0) {
        puts("cc_signal: blocked wake pipe failed");
        return 1;
    }

    pid_t child = fork();
    if (child < 0) {
        close(pipefd[0]);
        close(pipefd[1]);
        puts("cc_signal: blocked wake fork failed");
        return 1;
    }
    if (child == 0) {
        close(pipefd[0]);
        if (signal(SIGUSR1, on_blocked_usr1) == SIG_ERR) {
            _exit(2);
        }
        char ready = 'r';
        if (write(pipefd[1], &ready, 1) != 1) {
            _exit(3);
        }
        close(pipefd[1]);

        struct timespec req = { 5, 0 };
        nanosleep(&req, NULL);
        _exit(4);
    }

    close(pipefd[1]);
    char ready = 0;
    if (read(pipefd[0], &ready, 1) != 1) {
        close(pipefd[0]);
        kill(child, SIGKILL);
        waitpid(child, NULL, 0);
        puts("cc_signal: blocked wake ready failed");
        return 1;
    }
    close(pipefd[0]);

    for (int i = 0; i < 2000; i++) {
        syscall(SYS_sched_yield);
    }

    if (kill(child, SIGUSR1) < 0) {
        kill(child, SIGKILL);
        waitpid(child, NULL, 0);
        puts("cc_signal: blocked wake send failed");
        return 1;
    }

    int status = 0;
    for (int i = 0; i < 200; i++) {
        pid_t waited = waitpid(child, &status, WNOHANG);
        if (waited == child) {
            if (WIFEXITED(status) && WEXITSTATUS(status) == 0) {
                puts("cc_signal: blocked wake ok");
                return 0;
            }
            puts("cc_signal: blocked wake child failed");
            return 1;
        }
        if (waited < 0) {
            puts("cc_signal: blocked wake wait failed");
            return 1;
        }
        syscall(SYS_sched_yield);
    }

    kill(child, SIGKILL);
    waitpid(child, NULL, 0);
    puts("cc_signal: blocked wake timeout failed");
    return 1;
}

static int check_signal_restarts_syscall_entry(void) {
    int ready_pipe[2];
    if (pipe(ready_pipe) < 0) {
        puts("cc_signal: syscall entry pipe failed");
        return 1;
    }

    saw_entry_usr1 = 0;
    pid_t child = fork();
    if (child < 0) {
        close(ready_pipe[0]);
        close(ready_pipe[1]);
        puts("cc_signal: syscall entry fork failed");
        return 1;
    }
    if (child == 0) {
        close(ready_pipe[0]);
        saw_entry_usr1 = 0;
        if (signal(SIGUSR1, on_entry_usr1) == SIG_ERR) {
            _exit(2);
        }
        pid_t self = getpid();
        char ready = 'r';
        if (write(ready_pipe[1], &ready, 1) != 1) {
            _exit(3);
        }
        close(ready_pipe[1]);
        syscall(SYS_sched_yield);

        for (unsigned long i = 0; i < 5000000UL; i++) {
            entry_spin_sink += i;
        }

        pid_t after = getpid();
        if (!saw_entry_usr1) {
            _exit(10);
        }
        _exit(after == self ? 0 : 11);
    }

    close(ready_pipe[1]);
    char ready = 0;
    if (read(ready_pipe[0], &ready, 1) != 1) {
        close(ready_pipe[0]);
        kill(child, SIGKILL);
        waitpid(child, NULL, 0);
        puts("cc_signal: syscall entry ready failed");
        return 1;
    }
    close(ready_pipe[0]);

    if (kill(child, SIGUSR1) < 0) {
        kill(child, SIGKILL);
        waitpid(child, NULL, 0);
        puts("cc_signal: syscall entry send failed");
        return 1;
    }

    int status = 0;
    for (int i = 0; i < 500; i++) {
        pid_t waited = waitpid(child, &status, WNOHANG);
        if (waited == child) {
            if (WIFEXITED(status) && WEXITSTATUS(status) == 0) {
                puts("cc_signal: syscall entry restart ok");
                return 0;
            }
            printf("cc_signal: syscall entry child failed status=%d\n", status);
            return 1;
        }
        if (waited < 0) {
            puts("cc_signal: syscall entry wait failed");
            return 1;
        }
        syscall(SYS_sched_yield);
    }

    kill(child, SIGKILL);
    waitpid(child, NULL, 0);
    puts("cc_signal: syscall entry timeout failed");
    return 1;
}

static int check_signal_interrupts_blocked_read(void) {
    int data_pipe[2];
    int ready_pipe[2];
    if (pipe(data_pipe) < 0 || pipe(ready_pipe) < 0) {
        puts("cc_signal: blocked read pipe failed");
        return 1;
    }

    saw_interrupt_usr1 = 0;
    pid_t child = fork();
    if (child < 0) {
        close(data_pipe[0]);
        close(data_pipe[1]);
        close(ready_pipe[0]);
        close(ready_pipe[1]);
        puts("cc_signal: blocked read fork failed");
        return 1;
    }
    if (child == 0) {
        close(data_pipe[1]);
        close(ready_pipe[0]);
        saw_interrupt_usr1 = 0;
        if (signal(SIGUSR1, on_interrupt_usr1) == SIG_ERR) {
            _exit(2);
        }
        char ready = 'r';
        if (write(ready_pipe[1], &ready, 1) != 1) {
            _exit(3);
        }
        close(ready_pipe[1]);

        char byte = 0;
        errno = 0;
        ssize_t n = read(data_pipe[0], &byte, 1);
        close(data_pipe[0]);
        if (n != -1) {
            _exit(10);
        }
        if (errno != EINTR) {
            _exit(20);
        }
        if (!saw_interrupt_usr1) {
            _exit(30);
        }
        _exit(0);
    }

    close(data_pipe[0]);
    close(ready_pipe[1]);
    char ready = 0;
    if (read(ready_pipe[0], &ready, 1) != 1) {
        close(data_pipe[1]);
        close(ready_pipe[0]);
        kill(child, SIGKILL);
        waitpid(child, NULL, 0);
        puts("cc_signal: blocked read ready failed");
        return 1;
    }
    close(ready_pipe[0]);

    for (int i = 0; i < 2000; i++) {
        syscall(SYS_sched_yield);
    }

    if (kill(child, SIGUSR1) < 0) {
        close(data_pipe[1]);
        kill(child, SIGKILL);
        waitpid(child, NULL, 0);
        puts("cc_signal: blocked read send failed");
        return 1;
    }

    int status = 0;
    for (int i = 0; i < 200; i++) {
        pid_t waited = waitpid(child, &status, WNOHANG);
        if (waited == child) {
            close(data_pipe[1]);
            if (WIFEXITED(status) && WEXITSTATUS(status) == 0) {
                puts("cc_signal: blocked read interrupt ok");
                return 0;
            }
            printf("cc_signal: blocked read child failed status=%d\n", status);
            return 1;
        }
        if (waited < 0) {
            close(data_pipe[1]);
            puts("cc_signal: blocked read wait failed");
            return 1;
        }
        syscall(SYS_sched_yield);
    }

    close(data_pipe[1]);
    kill(child, SIGKILL);
    waitpid(child, NULL, 0);
    puts("cc_signal: blocked read timeout failed");
    return 1;
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

    saw_sigchld = 0;
    pid_t child = fork();
    if (child < 0) {
        puts("cc_signal: sigchld fork failed");
        return 1;
    }
    if (child == 0) {
        _exit(0);
    }

    for (int i = 0; i < 200000 && !saw_sigchld; i++) {
        syscall(SYS_sched_yield);
    }
    int status = 0;
    if (waitpid(child, &status, 0) != child || !WIFEXITED(status) ||
        WEXITSTATUS(status) != 0 || !saw_sigchld) {
        puts("cc_signal: sigchld child failed");
        return 1;
    }

    saw_sigchld = 0;
    child = fork();
    if (child < 0) {
        puts("cc_signal: sigchld stop fork failed");
        return 1;
    }
    if (child == 0) {
        for (;;) {
            syscall(SYS_sched_yield);
        }
    }
    if (kill(child, SIGTSTP) < 0) {
        kill(child, SIGKILL);
        waitpid(child, NULL, 0);
        puts("cc_signal: sigchld stop send failed");
        return 1;
    }
    for (int i = 0; i < 200000 && !saw_sigchld; i++) {
        syscall(SYS_sched_yield);
    }
    status = 0;
    if (waitpid(child, &status, WUNTRACED) != child ||
        !WIFSTOPPED(status) || WSTOPSIG(status) != SIGTSTP ||
        !saw_sigchld) {
        kill(child, SIGKILL);
        waitpid(child, NULL, 0);
        puts("cc_signal: sigchld stop failed");
        return 1;
    }
    if (signal(SIGCHLD, SIG_DFL) == SIG_ERR) {
        puts("cc_signal: sigchld restore failed");
        kill(child, SIGKILL);
        waitpid(child, NULL, 0);
        return 1;
    }
    if (kill(child, SIGCONT) < 0 || kill(child, SIGTERM) < 0 ||
        waitpid(child, &status, 0) != child || !WIFSIGNALED(status) ||
        WTERMSIG(status) != SIGTERM) {
        kill(child, SIGKILL);
        waitpid(child, NULL, 0);
        puts("cc_signal: sigchld stop cleanup failed");
        return 1;
    }
    puts("cc_signal: sigchld stop ok");
    puts("cc_signal: sigchld child ok");
    puts("cc_signal: sigchld ok");
    return 0;
}

static int check_sigchld_no_cldstop(void) {
    struct sigaction act;
    memset(&act, 0, sizeof(act));
    act.sa_handler = on_sigchld;
    act.sa_flags = SA_NOCLDSTOP;
    if (sigemptyset(&act.sa_mask) < 0 ||
        sigaction(SIGCHLD, &act, NULL) < 0) {
        puts("cc_signal: sigchld no-cldstop setup failed");
        return 1;
    }

    struct sigaction queried;
    memset(&queried, 0, sizeof(queried));
    if (sigaction(SIGCHLD, NULL, &queried) < 0 ||
        queried.sa_handler != on_sigchld ||
        queried.sa_flags != SA_NOCLDSTOP) {
        signal(SIGCHLD, SIG_DFL);
        puts("cc_signal: sigchld no-cldstop query failed");
        return 1;
    }

    saw_sigchld = 0;
    pid_t child = fork();
    if (child < 0) {
        signal(SIGCHLD, SIG_DFL);
        puts("cc_signal: sigchld no-cldstop fork failed");
        return 1;
    }
    if (child == 0) {
        for (;;) {
            syscall(SYS_sched_yield);
        }
    }

    if (kill(child, SIGTSTP) < 0) {
        signal(SIGCHLD, SIG_DFL);
        kill(child, SIGKILL);
        waitpid(child, NULL, 0);
        puts("cc_signal: sigchld no-cldstop stop send failed");
        return 1;
    }
    for (int i = 0; i < 200000 && !saw_sigchld; i++) {
        syscall(SYS_sched_yield);
    }

    int status = 0;
    if (waitpid(child, &status, WUNTRACED) != child ||
        !WIFSTOPPED(status) || WSTOPSIG(status) != SIGTSTP ||
        saw_sigchld) {
        signal(SIGCHLD, SIG_DFL);
        kill(child, SIGKILL);
        waitpid(child, NULL, 0);
        puts("cc_signal: sigchld no-cldstop stop failed");
        return 1;
    }

    if (kill(child, SIGCONT) < 0 ||
        waitpid(child, &status, WCONTINUED) != child ||
        !WIFCONTINUED(status) ||
        saw_sigchld) {
        signal(SIGCHLD, SIG_DFL);
        kill(child, SIGKILL);
        waitpid(child, NULL, 0);
        puts("cc_signal: sigchld no-cldstop continue failed");
        return 1;
    }

    if (signal(SIGCHLD, SIG_DFL) == SIG_ERR ||
        kill(child, SIGTERM) < 0 ||
        waitpid(child, &status, 0) != child ||
        !WIFSIGNALED(status) || WTERMSIG(status) != SIGTERM) {
        kill(child, SIGKILL);
        waitpid(child, NULL, 0);
        puts("cc_signal: sigchld no-cldstop cleanup failed");
        return 1;
    }

    puts("cc_signal: sigchld no-cldstop ok");
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

int main(int argc, char **argv) {
    if (argc > 1 && strcmp(argv[1], "exec-reset-probe") == 0) {
        raise(SIGUSR1);
        _exit(7);
    }
    if (argc > 1 && strcmp(argv[1], "exec-ignore-probe") == 0) {
        raise(SIGUSR2);
        _exit(0);
    }

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
    if (check_multiple_pending_signals() != 0) {
        return 1;
    }
    if (check_exec_signal_dispositions() != 0) {
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
        check_sigchld_no_cldstop() != 0 ||
        check_external_signal_handler() != 0 ||
        check_signal_wakes_blocked_syscall() != 0 ||
        check_signal_restarts_syscall_entry() != 0 ||
        check_signal_interrupts_blocked_read() != 0 ||
        check_handler_signal_mask() != 0 ||
        check_invalid_raw_handler() != 0 ||
        check_sigaction_fault_preserves_handler() != 0 ||
        check_sigkill_uncatchable() != 0 ||
        check_sigstop_uncatchable() != 0 ||
        check_external_sigstop() != 0 ||
        check_standard_signal_defaults() != 0 ||
        check_sigcont_handler() != 0 ||
        check_stop_wait_once() != 0 ||
        check_ignored_signals() != 0) {
        return 1;
    }

    puts("cc_signal: after handler");
    return 0;
}
