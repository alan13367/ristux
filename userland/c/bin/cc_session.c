#include <errno.h>
#include <signal.h>
#include <stdio.h>
#include <string.h>
#include <sys/resource.h>
#include <sys/wait.h>
#include <unistd.h>

static int check_group_leader_rejected(void) {
    if (setpgid(0, 0) < 0) {
        puts("cc_session: setpgid failed");
        return 1;
    }
    errno = 0;
    if (setsid() != -1 || errno != EPERM) {
        puts("cc_session: leader setsid failed");
        return 1;
    }
    puts("cc_session: leader rejection ok");
    return 0;
}

static int check_child_setsid(void) {
    pid_t child = fork();
    if (child < 0) {
        puts("cc_session: fork failed");
        return 1;
    }
    if (child == 0) {
        pid_t pid = getpid();
        pid_t sid = setsid();
        if (sid != pid) {
            _exit(2);
        }
        _exit(0);
    }

    int status = 0;
    if (waitpid(child, &status, 0) != child || WEXITSTATUS(status) != 0) {
        puts("cc_session: child setsid failed");
        return 1;
    }
    puts("cc_session: child setsid ok");
    return 0;
}

static int check_wait_nohang(void) {
    pid_t child = fork();
    if (child < 0) {
        puts("cc_session: nohang fork failed");
        return 1;
    }
    if (child == 0) {
        for (;;) {
        }
    }

    int status = 0;
    pid_t waited = waitpid(child, &status, WNOHANG);
    if (waited != 0) {
        puts("cc_session: nohang wait failed");
        kill(child, SIGTERM);
        waitpid(child, &status, 0);
        return 1;
    }
    if (kill(child, SIGTERM) < 0 ||
        waitpid(child, &status, 0) != child) {
        puts("cc_session: nohang cleanup failed");
        return 1;
    }
    puts("cc_session: wait nohang ok");
    return 0;
}

static void cleanup_wait_child(pid_t child) {
    if (child <= 0) {
        return;
    }
    kill(child, SIGCONT);
    kill(child, SIGTERM);
    waitpid(child, NULL, 0);
}

static int check_setpgid_errors(void) {
    pid_t original = getpgrp();
    errno = 0;
    if (setpgid(-2, 0) != -1 || errno != EINVAL) {
        puts("cc_session: setpgid negative pid failed");
        return 1;
    }
    errno = 0;
    if (setpgid(0, -1) != -1 || errno != EINVAL) {
        puts("cc_session: setpgid negative pgid failed");
        return 1;
    }
    if (getpgrp() != original) {
        puts("cc_session: setpgid invalid changed group");
        return 1;
    }

    int exit_pipe[2];
    if (pipe(exit_pipe) < 0) {
        puts("cc_session: setpgid child pipe failed");
        return 1;
    }
    pid_t child = fork();
    if (child < 0) {
        close(exit_pipe[0]);
        close(exit_pipe[1]);
        puts("cc_session: setpgid child fork failed");
        return 1;
    }
    if (child == 0) {
        close(exit_pipe[1]);
        char done = 0;
        (void)read(exit_pipe[0], &done, 1);
        close(exit_pipe[0]);
        _exit(0);
    }
    close(exit_pipe[0]);
    errno = 0;
    int missing_group_ok = setpgid(child, child + 10000) == -1 && errno == EPERM;
    char done = 'x';
    (void)write(exit_pipe[1], &done, 1);
    close(exit_pipe[1]);
    int status = 0;
    if (waitpid(child, &status, 0) != child || !WIFEXITED(status) ||
        WEXITSTATUS(status) != 0) {
        puts("cc_session: setpgid child cleanup failed");
        return 1;
    }
    if (!missing_group_ok) {
        puts("cc_session: setpgid missing group failed");
        return 1;
    }

    int ready_pipe[2] = { -1, -1 };
    exit_pipe[0] = -1;
    exit_pipe[1] = -1;
    if (pipe(ready_pipe) < 0 || pipe(exit_pipe) < 0) {
        if (ready_pipe[0] >= 0) {
            close(ready_pipe[0]);
            close(ready_pipe[1]);
        }
        if (exit_pipe[0] >= 0) {
            close(exit_pipe[0]);
            close(exit_pipe[1]);
        }
        puts("cc_session: setpgid pipe failed");
        return 1;
    }
    child = fork();
    if (child < 0) {
        close(ready_pipe[0]);
        close(ready_pipe[1]);
        close(exit_pipe[0]);
        close(exit_pipe[1]);
        puts("cc_session: setpgid session fork failed");
        return 1;
    }
    if (child == 0) {
        close(ready_pipe[0]);
        close(exit_pipe[1]);
        if (setsid() < 0) {
            _exit(2);
        }
        char ready = 'x';
        if (write(ready_pipe[1], &ready, 1) != 1) {
            _exit(3);
        }
        close(ready_pipe[1]);
        (void)read(exit_pipe[0], &ready, 1);
        close(exit_pipe[0]);
        _exit(0);
    }
    close(ready_pipe[1]);
    close(exit_pipe[0]);
    char ready = 0;
    if (read(ready_pipe[0], &ready, 1) != 1) {
        close(ready_pipe[0]);
        (void)write(exit_pipe[1], &done, 1);
        close(exit_pipe[1]);
        waitpid(child, NULL, 0);
        puts("cc_session: setpgid session ready failed");
        return 1;
    }
    close(ready_pipe[0]);
    errno = 0;
    int cross_session_ok = setpgid(child, child) == -1 && errno == EPERM;
    (void)write(exit_pipe[1], &done, 1);
    close(exit_pipe[1]);
    status = 0;
    if (waitpid(child, &status, 0) != child || !WIFEXITED(status) ||
        WEXITSTATUS(status) != 0) {
        puts("cc_session: setpgid session cleanup failed");
        return 1;
    }
    if (!cross_session_ok) {
        puts("cc_session: setpgid cross-session failed");
        return 1;
    }

    puts("cc_session: setpgid errors ok");
    return 0;
}

static int check_wait_process_groups(void) {
    pid_t other_group = fork();
    if (other_group < 0) {
        puts("cc_session: wait pgrp fork failed");
        return 1;
    }
    if (other_group == 0) {
        for (;;) {
            getpid();
        }
    }
    if (setpgid(other_group, other_group) < 0) {
        puts("cc_session: wait pgrp setpgid failed");
        cleanup_wait_child(other_group);
        return 1;
    }
    if (kill(other_group, SIGTSTP) < 0) {
        puts("cc_session: wait pgrp stop failed");
        cleanup_wait_child(other_group);
        return 1;
    }

    int status = 0;
    errno = 0;
    if (waitpid(0, &status, WNOHANG | WUNTRACED) != -1 || errno != ECHILD) {
        puts("cc_session: wait current pgrp failed");
        cleanup_wait_child(other_group);
        return 1;
    }

    if (waitpid(-other_group, &status, WUNTRACED) != other_group ||
        !WIFSTOPPED(status) || WSTOPSIG(status) != SIGTSTP) {
        puts("cc_session: wait negative pgrp failed");
        cleanup_wait_child(other_group);
        return 1;
    }
    if (kill(-other_group, SIGCONT) < 0 ||
        kill(-other_group, SIGTERM) < 0 ||
        waitpid(-other_group, &status, 0) != other_group ||
        !WIFSIGNALED(status) || WTERMSIG(status) != SIGTERM) {
        puts("cc_session: wait pgrp cleanup failed");
        cleanup_wait_child(other_group);
        return 1;
    }

    pid_t same_group = fork();
    if (same_group < 0) {
        puts("cc_session: wait same pgrp fork failed");
        return 1;
    }
    if (same_group == 0) {
        _exit(12);
    }
    if (waitpid(0, &status, 0) != same_group || !WIFEXITED(status) ||
        WEXITSTATUS(status) != 12) {
        puts("cc_session: wait same pgrp failed");
        cleanup_wait_child(same_group);
        return 1;
    }

    puts("cc_session: wait pgrp ok");
    return 0;
}

static int check_wait_continued_once(void) {
    pid_t child = fork();
    if (child < 0) {
        puts("cc_session: wait continued fork failed");
        return 1;
    }
    if (child == 0) {
        for (;;) {
            getpid();
        }
    }

    if (kill(child, SIGTSTP) < 0) {
        puts("cc_session: wait continued stop failed");
        cleanup_wait_child(child);
        return 1;
    }

    int status = 0;
    if (waitpid(child, &status, WUNTRACED) != child ||
        !WIFSTOPPED(status) || WSTOPSIG(status) != SIGTSTP) {
        puts("cc_session: wait continued stop wait failed");
        cleanup_wait_child(child);
        return 1;
    }

    if (kill(child, SIGCONT) < 0 ||
        waitpid(child, &status, WCONTINUED) != child ||
        !WIFCONTINUED(status)) {
        puts("cc_session: wait continued failed");
        cleanup_wait_child(child);
        return 1;
    }

    errno = 0;
    if (waitpid(child, &status, WNOHANG | WCONTINUED) != 0) {
        puts("cc_session: wait continued duplicate failed");
        cleanup_wait_child(child);
        return 1;
    }

    if (kill(child, SIGTERM) < 0 ||
        waitpid(child, &status, 0) != child ||
        !WIFSIGNALED(status) || WTERMSIG(status) != SIGTERM) {
        puts("cc_session: wait continued cleanup failed");
        cleanup_wait_child(child);
        return 1;
    }

    puts("cc_session: wait continued ok");
    return 0;
}

static int check_wait_errors(void) {
    int status = 0;
    errno = 0;
    if (waitpid(-1, &status, WNOHANG) != -1 || errno != ECHILD) {
        puts("cc_session: wait nochild failed");
        return 1;
    }
    errno = 0;
    if (waitpid(-1, &status, 0x4000) != -1 || errno != EINVAL) {
        puts("cc_session: wait invalid options failed");
        return 1;
    }
    puts("cc_session: wait errors ok");
    return 0;
}

static int check_wait_bad_status_pointer(void) {
    pid_t child = fork();
    if (child < 0) {
        puts("cc_session: wait bad status fork failed");
        return 1;
    }
    if (child == 0) {
        _exit(7);
    }

    errno = 0;
    if (waitpid(child, (int *)1, 0) != -1 || errno != EFAULT) {
        puts("cc_session: wait bad status pointer failed");
        return 1;
    }

    int status = 0;
    if (waitpid(child, &status, 0) != child || !WIFEXITED(status) ||
        WEXITSTATUS(status) != 7) {
        puts("cc_session: wait bad status retry failed");
        return 1;
    }
    puts("cc_session: wait bad status ok");
    return 0;
}

static int check_wait_rusage(void) {
    pid_t child = fork();
    if (child < 0) {
        puts("cc_session: wait rusage fork failed");
        return 1;
    }
    if (child == 0) {
        _exit(8);
    }

    int status = 0;
    struct rusage usage;
    memset(&usage, 0x5a, sizeof(usage));
    if (wait4(child, &status, 0, &usage) != child || !WIFEXITED(status) ||
        WEXITSTATUS(status) != 8 ||
        usage.ru_utime.tv_sec != 0 ||
        usage.ru_utime.tv_usec != 0 ||
        usage.ru_stime.tv_sec != 0 ||
        usage.ru_stime.tv_usec != 0) {
        puts("cc_session: wait rusage failed");
        return 1;
    }

    child = fork();
    if (child < 0) {
        puts("cc_session: wait bad rusage fork failed");
        return 1;
    }
    if (child == 0) {
        _exit(9);
    }

    status = 0x12345678;
    errno = 0;
    if (wait4(child, &status, 0, (void *)1) != -1 || errno != EFAULT ||
        status != 0x12345678) {
        puts("cc_session: wait bad rusage failed");
        return 1;
    }

    status = 0;
    if (waitpid(child, &status, 0) != child || !WIFEXITED(status) ||
        WEXITSTATUS(status) != 9) {
        puts("cc_session: wait bad rusage retry failed");
        return 1;
    }

    puts("cc_session: wait rusage ok");
    return 0;
}

int main(void) {
    if (check_group_leader_rejected() != 0) {
        return 1;
    }
    if (check_child_setsid() != 0) {
        return 1;
    }
    if (check_setpgid_errors() != 0) {
        return 1;
    }
    if (check_wait_nohang() != 0) {
        return 1;
    }
    if (check_wait_process_groups() != 0) {
        return 1;
    }
    if (check_wait_continued_once() != 0) {
        return 1;
    }
    if (check_wait_errors() != 0) {
        return 1;
    }
    if (check_wait_bad_status_pointer() != 0) {
        return 1;
    }
    if (check_wait_rusage() != 0) {
        return 1;
    }
    puts("cc_session: done");
    return 0;
}
