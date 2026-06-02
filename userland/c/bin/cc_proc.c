#include <errno.h>
#include <fcntl.h>
#include <stdio.h>
#include <string.h>
#include <sys/stat.h>
#include <sys/wait.h>
#include <unistd.h>

static int contains(const char *haystack, const char *needle) {
    size_t needle_len = strlen(needle);
    if (needle_len == 0) {
        return 1;
    }
    for (size_t i = 0; haystack[i] != '\0'; i++) {
        size_t j = 0;
        while (needle[j] != '\0' && haystack[i + j] == needle[j]) {
            j++;
        }
        if (j == needle_len) {
            return 1;
        }
    }
    return 0;
}

static int wait_for_zero(pid_t child, const char *label) {
    int status = 0;
    if (waitpid(child, &status, 0) != child || !WIFEXITED(status) ||
        WEXITSTATUS(status) != 0) {
        puts(label);
        return 1;
    }
    return 0;
}

static int check_exec_vector_limits(void) {
    char *too_many_args[66];
    for (int i = 0; i < 65; i++) {
        too_many_args[i] = "arg";
    }
    too_many_args[65] = NULL;
    char *empty_env[] = { NULL };

    pid_t child = fork();
    if (child < 0) {
        puts("cc_proc: exec argv fork failed");
        return 1;
    }
    if (child == 0) {
        execve("/bin/false", too_many_args, empty_env);
        _exit(errno == E2BIG ? 0 : 100);
    }
    if (wait_for_zero(child, "cc_proc: exec argv limit failed") != 0) {
        return 1;
    }

    char *argv[] = { "/bin/false", NULL };
    char *too_many_env[66];
    for (int i = 0; i < 65; i++) {
        too_many_env[i] = "K=V";
    }
    too_many_env[65] = NULL;

    child = fork();
    if (child < 0) {
        puts("cc_proc: exec env fork failed");
        return 1;
    }
    if (child == 0) {
        execve("/bin/false", argv, too_many_env);
        _exit(errno == E2BIG ? 0 : 101);
    }
    if (wait_for_zero(child, "cc_proc: exec env limit failed") != 0) {
        return 1;
    }

    puts("cc_proc: exec vector limits ok");
    return 0;
}

static int check_exec_unterminated_path(void) {
    pid_t child = fork();
    if (child < 0) {
        puts("cc_proc: exec unterminated fork failed");
        return 1;
    }
    if (child == 0) {
        char path[4096];
        memset(path, 'x', sizeof(path));
        char *argv[] = { path, NULL };
        char *envp[] = { NULL };
        execve(path, argv, envp);
        _exit(errno == EFAULT ? 0 : 102);
    }
    if (wait_for_zero(child, "cc_proc: exec unterminated path failed") != 0) {
        return 1;
    }

    puts("cc_proc: exec unterminated path ok");
    return 0;
}

static int check_exec_shebang_limit_transaction(void) {
    const char *script = "/tmp/cc_proc_shebang";
    const char *body = "#!/bin/false\n";
    int fd = open(script, O_CREAT | O_TRUNC | O_WRONLY, 0755);
    if (fd < 0) {
        puts("cc_proc: exec shebang create failed");
        return 1;
    }
    if (write(fd, body, strlen(body)) != (ssize_t)strlen(body)) {
        close(fd);
        puts("cc_proc: exec shebang write failed");
        return 1;
    }
    close(fd);
    if (chmod(script, 0755) < 0) {
        puts("cc_proc: exec shebang chmod failed");
        return 1;
    }

    char *argv[65];
    argv[0] = (char *)script;
    for (int i = 1; i < 64; i++) {
        argv[i] = "arg";
    }
    argv[64] = NULL;
    char *envp[] = { NULL };

    pid_t child = fork();
    if (child < 0) {
        puts("cc_proc: exec shebang fork failed");
        return 1;
    }
    if (child == 0) {
        execve(script, argv, envp);
        _exit(errno == E2BIG ? 0 : 103);
    }
    if (wait_for_zero(child, "cc_proc: exec shebang limit failed") != 0) {
        return 1;
    }

    puts("cc_proc: exec shebang limit ok");
    return 0;
}

static int check_exec_invalid_image(void) {
    const char *path = "/tmp/cc_proc_bad_elf";
    const char *body = "not an elf\n";
    int fd = open(path, O_CREAT | O_TRUNC | O_WRONLY, 0755);
    if (fd < 0) {
        puts("cc_proc: exec invalid create failed");
        return 1;
    }
    if (write(fd, body, strlen(body)) != (ssize_t)strlen(body)) {
        close(fd);
        puts("cc_proc: exec invalid write failed");
        return 1;
    }
    close(fd);
    if (chmod(path, 0755) < 0) {
        puts("cc_proc: exec invalid chmod failed");
        return 1;
    }

    pid_t child = fork();
    if (child < 0) {
        puts("cc_proc: exec invalid fork failed");
        return 1;
    }
    if (child == 0) {
        char *argv[] = { (char *)path, NULL };
        char *envp[] = { NULL };
        execve(path, argv, envp);
        _exit(errno == ENOEXEC ? 0 : 104);
    }
    if (wait_for_zero(child, "cc_proc: exec invalid image failed") != 0) {
        return 1;
    }

    puts("cc_proc: exec invalid image ok");
    return 0;
}

int main(void) {
    int pipefd[2];
    if (pipe(pipefd) < 0) {
        puts("cc_proc: pipe failed");
        return 1;
    }

    pid_t child = fork();
    if (child < 0) {
        puts("cc_proc: fork failed");
        return 1;
    }

    if (child == 0) {
        close(pipefd[0]);
        if (dup2(pipefd[1], 1) < 0) {
            _exit(120);
        }
        close(pipefd[1]);
        char *argv[] = { "/bin/echo", "child-pipe", NULL };
        char *envp[] = { "CC_PROC=1", NULL };
        execve("/bin/echo", argv, envp);
        _exit(127);
    }

    close(pipefd[1]);
    char buf[64];
    ssize_t n = read(pipefd[0], buf, sizeof(buf) - 1);
    close(pipefd[0]);
    if (n <= 0) {
        puts("cc_proc: read failed");
        return 1;
    }
    buf[n] = '\0';

    int status = 0;
    pid_t waited = waitpid(child, &status, 0);
    if (waited != child) {
        puts("cc_proc: wait failed");
        return 1;
    }
    if (!contains(buf, "child-pipe") || WEXITSTATUS(status) != 0) {
        puts("cc_proc: child failed");
        return 1;
    }

    puts("cc_proc: pipe exec ok");
    puts("cc_proc: wait ok");
    if (check_exec_vector_limits() != 0) {
        return 1;
    }
    if (check_exec_unterminated_path() != 0) {
        return 1;
    }
    if (check_exec_shebang_limit_transaction() != 0) {
        return 1;
    }
    if (check_exec_invalid_image() != 0) {
        return 1;
    }
    puts("cc_proc: done");
    return 0;
}
