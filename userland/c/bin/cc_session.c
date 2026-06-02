#include <errno.h>
#include <signal.h>
#include <stdio.h>
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

int main(void) {
    if (check_group_leader_rejected() != 0) {
        return 1;
    }
    if (check_child_setsid() != 0) {
        return 1;
    }
    if (check_wait_nohang() != 0) {
        return 1;
    }
    if (check_wait_errors() != 0) {
        return 1;
    }
    puts("cc_session: done");
    return 0;
}
