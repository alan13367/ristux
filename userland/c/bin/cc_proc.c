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

static void write_le64(unsigned char *out, unsigned long value) {
    for (int i = 0; i < 8; i++) {
        out[i] = (unsigned char)((value >> (i * 8)) & 0xff);
    }
}

static unsigned long read_le16(const unsigned char *in) {
    return (unsigned long)in[0] | ((unsigned long)in[1] << 8);
}

static unsigned long read_le32(const unsigned char *in) {
    unsigned long value = 0;
    for (int i = 0; i < 4; i++) {
        value |= (unsigned long)in[i] << (i * 8);
    }
    return value;
}

static unsigned long read_le64(const unsigned char *in) {
    unsigned long value = 0;
    for (int i = 0; i < 8; i++) {
        value |= (unsigned long)in[i] << (i * 8);
    }
    return value;
}

static int patch_first_load_vaddr(unsigned char *image, ssize_t len, unsigned long vaddr) {
    if (len < 64) {
        return -1;
    }
    unsigned long phoff = read_le64(&image[32]);
    unsigned long phentsize = read_le16(&image[54]);
    unsigned long phnum = read_le16(&image[56]);
    if (phentsize < 56) {
        return -1;
    }
    for (unsigned long i = 0; i < phnum; i++) {
        unsigned long off = phoff + i * phentsize;
        if (off + 56 > (unsigned long)len) {
            return -1;
        }
        if (read_le32(&image[off]) == 1) {
            write_le64(&image[off + 16], vaddr);
            write_le64(&image[off + 24], vaddr);
            return 0;
        }
    }
    return -1;
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

static int check_exec_nonexec_entry(void) {
    const char *source = "/bin/true";
    const char *path = "/tmp/cc_proc_bad_entry";
    static unsigned char image[65536];
    ssize_t total = 0;

    int in = open(source, O_RDONLY, 0);
    if (in < 0) {
        puts("cc_proc: exec bad entry source failed");
        return 1;
    }
    for (;;) {
        if ((size_t)total == sizeof(image)) {
            close(in);
            puts("cc_proc: exec bad entry too large");
            return 1;
        }
        ssize_t n = read(in, image + total, sizeof(image) - (size_t)total);
        if (n < 0) {
            close(in);
            puts("cc_proc: exec bad entry read failed");
            return 1;
        }
        if (n == 0) {
            break;
        }
        total += n;
    }
    close(in);
    if (total < 64 || memcmp(image, "\177ELF", 4) != 0) {
        puts("cc_proc: exec bad entry source invalid");
        return 1;
    }
    write_le64(&image[24], 0);

    int fd = open(path, O_CREAT | O_TRUNC | O_WRONLY, 0755);
    if (fd < 0) {
        puts("cc_proc: exec bad entry create failed");
        return 1;
    }
    if (write(fd, image, (size_t)total) != total) {
        close(fd);
        puts("cc_proc: exec bad entry write failed");
        return 1;
    }
    close(fd);
    if (chmod(path, 0755) < 0) {
        puts("cc_proc: exec bad entry chmod failed");
        return 1;
    }

    pid_t child = fork();
    if (child < 0) {
        puts("cc_proc: exec bad entry fork failed");
        return 1;
    }
    if (child == 0) {
        char *argv[] = { (char *)path, NULL };
        char *envp[] = { NULL };
        execve(path, argv, envp);
        _exit(errno == ENOEXEC ? 0 : 105);
    }
    if (wait_for_zero(child, "cc_proc: exec bad entry failed") != 0) {
        return 1;
    }

    puts("cc_proc: exec bad entry ok");
    return 0;
}

static int check_exec_high_segment(void) {
    const char *source = "/bin/true";
    const char *path = "/tmp/cc_proc_high_segment";
    static unsigned char image[65536];
    ssize_t total = 0;

    int in = open(source, O_RDONLY, 0);
    if (in < 0) {
        puts("cc_proc: exec high segment source failed");
        return 1;
    }
    for (;;) {
        if ((size_t)total == sizeof(image)) {
            close(in);
            puts("cc_proc: exec high segment too large");
            return 1;
        }
        ssize_t n = read(in, image + total, sizeof(image) - (size_t)total);
        if (n < 0) {
            close(in);
            puts("cc_proc: exec high segment read failed");
            return 1;
        }
        if (n == 0) {
            break;
        }
        total += n;
    }
    close(in);
    if (total < 64 || memcmp(image, "\177ELF", 4) != 0 ||
        patch_first_load_vaddr(image, total, 0xffffffff80000000UL) != 0) {
        puts("cc_proc: exec high segment source invalid");
        return 1;
    }

    int fd = open(path, O_CREAT | O_TRUNC | O_WRONLY, 0755);
    if (fd < 0) {
        puts("cc_proc: exec high segment create failed");
        return 1;
    }
    if (write(fd, image, (size_t)total) != total) {
        close(fd);
        puts("cc_proc: exec high segment write failed");
        return 1;
    }
    close(fd);
    if (chmod(path, 0755) < 0) {
        puts("cc_proc: exec high segment chmod failed");
        return 1;
    }

    pid_t child = fork();
    if (child < 0) {
        puts("cc_proc: exec high segment fork failed");
        return 1;
    }
    if (child == 0) {
        char *argv[] = { (char *)path, NULL };
        char *envp[] = { NULL };
        execve(path, argv, envp);
        _exit(errno == ENOEXEC ? 0 : 106);
    }
    if (wait_for_zero(child, "cc_proc: exec high segment failed") != 0) {
        return 1;
    }

    puts("cc_proc: exec high segment ok");
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
    if (check_exec_nonexec_entry() != 0) {
        return 1;
    }
    if (check_exec_high_segment() != 0) {
        return 1;
    }
    puts("cc_proc: done");
    return 0;
}
