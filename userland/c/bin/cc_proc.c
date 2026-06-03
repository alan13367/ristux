#include <errno.h>
#include <fcntl.h>
#include <signal.h>
#include <stdio.h>
#include <string.h>
#include <sys/mman.h>
#include <sys/stat.h>
#include <sys/syscall.h>
#include <sys/wait.h>
#include <unistd.h>

#define CLONE_VM 0x00000100UL
#define CLONE_FS 0x00000200UL
#define CLONE_FILES 0x00000400UL
#define CLONE_SIGHAND 0x00000800UL
#define CLONE_THREAD 0x00010000UL
#define CLONE_SETTLS 0x00080000UL
#define CLONE_PARENT_SETTID 0x00100000UL
#define CLONE_CHILD_CLEARTID 0x00200000UL
#define CLONE_CHILD_SETTID 0x01000000UL
#define TLS_SENTINEL 0x1122334455667788UL

static const char elf_rodata_probe[] = "rodata-permission-probe";
static int elf_data_probe_value = 7;
static unsigned char elf_data_exec_probe[] = { 0xc3 };

static void elf_text_probe(void) {
}

static unsigned long read_fs_zero(void) {
    unsigned long value = 0;
    __asm__ volatile("movq %%fs:0, %0" : "=r"(value));
    return value;
}

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

static int check_elf_runtime_permissions(void) {
    int zero_fd = open("/dev/zero", O_RDONLY, 0);
    int null_fd = open("/dev/null", O_WRONLY, 0);
    if (zero_fd < 0 || null_fd < 0) {
        close(zero_fd);
        close(null_fd);
        puts("cc_proc: elf permission device open failed");
        return 1;
    }

    elf_data_probe_value = 42;
    elf_data_exec_probe[0] = 0xc3;
    if (elf_data_probe_value != 42 || elf_data_exec_probe[0] != 0xc3) {
        close(zero_fd);
        close(null_fd);
        puts("cc_proc: elf data write failed");
        return 1;
    }
    if (write(null_fd, elf_rodata_probe, 1) != 1 ||
        write(null_fd, (const void *)elf_text_probe, 1) != 1 ||
        write(null_fd, &elf_data_probe_value, 1) != 1) {
        close(zero_fd);
        close(null_fd);
        puts("cc_proc: elf readable segments failed");
        return 1;
    }

    errno = 0;
    if (read(zero_fd, (void *)elf_rodata_probe, 1) != -1 || errno != EFAULT) {
        close(zero_fd);
        close(null_fd);
        printf("cc_proc: elf rodata write errno=%d\n", errno);
        return 1;
    }
    errno = 0;
    if (read(zero_fd, (void *)elf_text_probe, 1) != -1 || errno != EFAULT) {
        close(zero_fd);
        close(null_fd);
        printf("cc_proc: elf text write errno=%d\n", errno);
        return 1;
    }
    close(zero_fd);
    close(null_fd);
    puts("cc_proc: elf permissions ok");
    return 0;
}

static void trigger_unmapped_user_write(void) {
    volatile unsigned long *ptr = (volatile unsigned long *)0x59000000UL;
    *ptr = 0x5e6f;
}

static void trigger_divide_error(void) {
    __asm__ volatile(
        "xor %%edx, %%edx\n"
        "mov $1, %%eax\n"
        "xor %%ecx, %%ecx\n"
        "div %%ecx\n"
        :
        :
        : "rax", "rcx", "rdx");
}

static void trigger_invalid_opcode(void) {
    __asm__ volatile("ud2");
}

static void trigger_general_protection(void) {
    __asm__ volatile("cli");
}

static int expect_user_fault_signal(const char *label, void (*trigger)(void), int signal) {
    pid_t child = fork();
    if (child < 0) {
        printf("cc_proc: %s fork failed\n", label);
        return 1;
    }
    if (child == 0) {
        trigger();
        _exit(111);
    }

    int status = 0;
    if (waitpid(child, &status, 0) != child || !WIFSIGNALED(status) ||
        WTERMSIG(status) != signal) {
        printf("cc_proc: %s containment failed\n", label);
        return 1;
    }
    return 0;
}

static int check_user_fault_containment(void) {
    if (expect_user_fault_signal("user page fault", trigger_unmapped_user_write, SIGSEGV) != 0 ||
        expect_user_fault_signal("user divide error", trigger_divide_error, SIGFPE) != 0 ||
        expect_user_fault_signal("user invalid opcode", trigger_invalid_opcode, SIGILL) != 0 ||
        expect_user_fault_signal("user gpf", trigger_general_protection, SIGSEGV) != 0) {
        return 1;
    }
    puts("cc_proc: user fault containment ok");
    return 0;
}

static void write_le64(unsigned char *out, unsigned long value) {
    for (int i = 0; i < 8; i++) {
        out[i] = (unsigned char)((value >> (i * 8)) & 0xff);
    }
}

static void write_le32(unsigned char *out, unsigned long value) {
    for (int i = 0; i < 4; i++) {
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

static int patch_second_load_vaddr_to_first(unsigned char *image, ssize_t len) {
    if (len < 64) {
        return -1;
    }
    unsigned long phoff = read_le64(&image[32]);
    unsigned long phentsize = read_le16(&image[54]);
    unsigned long phnum = read_le16(&image[56]);
    if (phentsize < 56) {
        return -1;
    }

    unsigned long first_vaddr = 0;
    unsigned long loads = 0;
    for (unsigned long i = 0; i < phnum; i++) {
        unsigned long off = phoff + i * phentsize;
        if (off + 56 > (unsigned long)len) {
            return -1;
        }
        if (read_le32(&image[off]) != 1) {
            continue;
        }
        if (loads == 0) {
            first_vaddr = read_le64(&image[off + 16]);
        } else {
            write_le64(&image[off + 16], first_vaddr);
            write_le64(&image[off + 24], first_vaddr);
            return 0;
        }
        loads++;
    }
    return -1;
}

static int patch_first_load_flags(unsigned char *image, ssize_t len, unsigned long flags) {
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
            write_le32(&image[off + 4], flags);
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

static int check_exec_long_strings(void) {
    static char long_arg[4097];
    static char long_env[4097];

    memset(long_arg, 'a', sizeof(long_arg) - 1);
    long_arg[sizeof(long_arg) - 1] = '\0';
    long_env[0] = 'K';
    long_env[1] = '=';
    memset(long_env + 2, 'b', sizeof(long_env) - 3);
    long_env[sizeof(long_env) - 1] = '\0';

    char *empty_env[] = { NULL };
    char *long_argv[] = { "/bin/false", long_arg, NULL };
    pid_t child = fork();
    if (child < 0) {
        puts("cc_proc: exec long argv fork failed");
        return 1;
    }
    if (child == 0) {
        execve("/bin/false", long_argv, empty_env);
        _exit(errno == E2BIG ? 0 : 104);
    }
    if (wait_for_zero(child, "cc_proc: exec long argv failed") != 0) {
        return 1;
    }

    char *argv[] = { "/bin/false", NULL };
    char *long_envp[] = { long_env, NULL };
    child = fork();
    if (child < 0) {
        puts("cc_proc: exec long env fork failed");
        return 1;
    }
    if (child == 0) {
        execve("/bin/false", argv, long_envp);
        _exit(errno == E2BIG ? 0 : 105);
    }
    if (wait_for_zero(child, "cc_proc: exec long env failed") != 0) {
        return 1;
    }

    puts("cc_proc: exec long strings ok");
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
        _exit(errno == ENAMETOOLONG ? 0 : 102);
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

static int check_exec_reserved_segment(void) {
    const char *source = "/bin/true";
    const char *path = "/tmp/cc_proc_reserved_segment";
    static unsigned char image[65536];
    ssize_t total = 0;

    int in = open(source, O_RDONLY, 0);
    if (in < 0) {
        puts("cc_proc: exec reserved segment source failed");
        return 1;
    }
    for (;;) {
        if ((size_t)total == sizeof(image)) {
            close(in);
            puts("cc_proc: exec reserved segment too large");
            return 1;
        }
        ssize_t n = read(in, image + total, sizeof(image) - (size_t)total);
        if (n < 0) {
            close(in);
            puts("cc_proc: exec reserved segment read failed");
            return 1;
        }
        if (n == 0) {
            break;
        }
        total += n;
    }
    close(in);
    if (total < 64 || memcmp(image, "\177ELF", 4) != 0 ||
        patch_first_load_vaddr(image, total, 0x60000000UL) != 0) {
        puts("cc_proc: exec reserved segment source invalid");
        return 1;
    }

    int fd = open(path, O_CREAT | O_TRUNC | O_WRONLY, 0755);
    if (fd < 0) {
        puts("cc_proc: exec reserved segment create failed");
        return 1;
    }
    if (write(fd, image, (size_t)total) != total) {
        close(fd);
        puts("cc_proc: exec reserved segment write failed");
        return 1;
    }
    close(fd);
    if (chmod(path, 0755) < 0) {
        puts("cc_proc: exec reserved segment chmod failed");
        return 1;
    }

    pid_t child = fork();
    if (child < 0) {
        puts("cc_proc: exec reserved segment fork failed");
        return 1;
    }
    if (child == 0) {
        char *argv[] = { (char *)path, NULL };
        char *envp[] = { NULL };
        execve(path, argv, envp);
        _exit(errno == ENOEXEC ? 0 : 107);
    }
    if (wait_for_zero(child, "cc_proc: exec reserved segment failed") != 0) {
        return 1;
    }

    puts("cc_proc: exec reserved segment ok");
    return 0;
}

static int check_exec_overlapping_segment(void) {
    const char *source = "/bin/true";
    const char *path = "/tmp/cc_proc_overlap_segment";
    static unsigned char image[65536];
    ssize_t total = 0;

    int in = open(source, O_RDONLY, 0);
    if (in < 0) {
        puts("cc_proc: exec overlap segment source failed");
        return 1;
    }
    for (;;) {
        if ((size_t)total == sizeof(image)) {
            close(in);
            puts("cc_proc: exec overlap segment too large");
            return 1;
        }
        ssize_t n = read(in, image + total, sizeof(image) - (size_t)total);
        if (n < 0) {
            close(in);
            puts("cc_proc: exec overlap segment read failed");
            return 1;
        }
        if (n == 0) {
            break;
        }
        total += n;
    }
    close(in);
    if (total < 64 || memcmp(image, "\177ELF", 4) != 0 ||
        patch_second_load_vaddr_to_first(image, total) != 0) {
        puts("cc_proc: exec overlap segment source invalid");
        return 1;
    }

    int fd = open(path, O_CREAT | O_TRUNC | O_WRONLY, 0755);
    if (fd < 0) {
        puts("cc_proc: exec overlap segment create failed");
        return 1;
    }
    if (write(fd, image, (size_t)total) != total) {
        close(fd);
        puts("cc_proc: exec overlap segment write failed");
        return 1;
    }
    close(fd);
    if (chmod(path, 0755) < 0) {
        puts("cc_proc: exec overlap segment chmod failed");
        return 1;
    }

    pid_t child = fork();
    if (child < 0) {
        puts("cc_proc: exec overlap segment fork failed");
        return 1;
    }
    if (child == 0) {
        char *argv[] = { (char *)path, NULL };
        char *envp[] = { NULL };
        execve(path, argv, envp);
        _exit(errno == ENOEXEC ? 0 : 109);
    }
    if (wait_for_zero(child, "cc_proc: exec overlap segment failed") != 0) {
        return 1;
    }

    puts("cc_proc: exec overlap segment ok");
    return 0;
}

static int check_exec_wx_segment(void) {
    const char *source = "/bin/true";
    const char *path = "/tmp/cc_proc_wx_segment";
    static unsigned char image[65536];
    ssize_t total = 0;

    int in = open(source, O_RDONLY, 0);
    if (in < 0) {
        puts("cc_proc: exec wx segment source failed");
        return 1;
    }
    for (;;) {
        if ((size_t)total == sizeof(image)) {
            close(in);
            puts("cc_proc: exec wx segment too large");
            return 1;
        }
        ssize_t n = read(in, image + total, sizeof(image) - (size_t)total);
        if (n < 0) {
            close(in);
            puts("cc_proc: exec wx segment read failed");
            return 1;
        }
        if (n == 0) {
            break;
        }
        total += n;
    }
    close(in);
    if (total < 64 || memcmp(image, "\177ELF", 4) != 0 ||
        patch_first_load_flags(image, total, 0x7) != 0) {
        puts("cc_proc: exec wx segment source invalid");
        return 1;
    }

    int fd = open(path, O_CREAT | O_TRUNC | O_WRONLY, 0755);
    if (fd < 0) {
        puts("cc_proc: exec wx segment create failed");
        return 1;
    }
    if (write(fd, image, (size_t)total) != total) {
        close(fd);
        puts("cc_proc: exec wx segment write failed");
        return 1;
    }
    close(fd);
    if (chmod(path, 0755) < 0) {
        puts("cc_proc: exec wx segment chmod failed");
        return 1;
    }

    pid_t child = fork();
    if (child < 0) {
        puts("cc_proc: exec wx segment fork failed");
        return 1;
    }
    if (child == 0) {
        char *argv[] = { (char *)path, NULL };
        char *envp[] = { NULL };
        execve(path, argv, envp);
        _exit(errno == ENOEXEC ? 0 : 108);
    }
    if (wait_for_zero(child, "cc_proc: exec wx segment failed") != 0) {
        return 1;
    }

    puts("cc_proc: exec wx segment ok");
    return 0;
}

static int check_clone_sigchld(void) {
    errno = 0;
    unsigned long shared_flags[] = {
        CLONE_VM,
        CLONE_FS,
        CLONE_FILES,
        CLONE_SIGHAND,
        CLONE_THREAD,
        CLONE_VM | CLONE_THREAD | CLONE_SIGHAND,
        CLONE_PARENT_SETTID,
        CLONE_CHILD_CLEARTID,
        CLONE_CHILD_SETTID,
    };
    for (size_t i = 0; i < sizeof(shared_flags) / sizeof(shared_flags[0]); i++) {
        errno = 0;
        if (syscall(SYS_clone, SIGCHLD | shared_flags[i], 0, 0, 0, 0, 0) != -1 ||
            errno != EINVAL) {
            puts("cc_proc: clone flags failed");
            return 1;
        }
    }

    errno = 0;
    if (syscall(SYS_clone, SIGCHLD | CLONE_SETTLS, 0, 0, 0, 0x70000000L, 0) != -1 ||
        errno != EFAULT) {
        puts("cc_proc: clone tls fault failed");
        return 1;
    }

    errno = 0;
    if (syscall(SYS_clone, SIGCHLD, 0x70000000L, 0, 0, 0, 0) != -1 ||
        errno != EFAULT) {
        puts("cc_proc: clone stack failed");
        return 1;
    }

    int parent_tid = 0;
    errno = 0;
    if (syscall(SYS_clone, SIGCHLD, 0, (long)&parent_tid, 0, 0, 0) != -1 ||
        errno != EINVAL) {
        puts("cc_proc: clone parent tid failed");
        return 1;
    }

    int child_tid = 0;
    errno = 0;
    if (syscall(SYS_clone, SIGCHLD, 0, 0, (long)&child_tid, 0, 0) != -1 ||
        errno != EINVAL) {
        puts("cc_proc: clone child tid failed");
        return 1;
    }

    unsigned long tls = 0x70000000UL;
    errno = 0;
    if (syscall(SYS_clone, SIGCHLD, 0, 0, 0, (long)tls, 0) != -1 ||
        errno != EINVAL) {
        puts("cc_proc: clone tls failed");
        return 1;
    }

    puts("cc_proc: clone unsupported forms ok");

    errno = 0;
    long cloned = syscall(SYS_clone, SIGCHLD, 0, 0, 0, 0, 0);
    if (cloned < 0) {
        puts("cc_proc: clone sigchld failed");
        return 1;
    }
    if (cloned == 0) {
        _exit(0);
    }

    int status = 0;
    if (waitpid((pid_t)cloned, &status, 0) != (pid_t)cloned ||
        !WIFEXITED(status) || WEXITSTATUS(status) != 0) {
        puts("cc_proc: clone wait failed");
        return 1;
    }

    puts("cc_proc: clone sigchld ok");
    return 0;
}

static int check_clone_tls(void) {
    unsigned long *tls_page = mmap(NULL, 4096, PROT_READ | PROT_WRITE,
                                   MAP_PRIVATE | MAP_ANONYMOUS, -1, 0);
    if (tls_page == MAP_FAILED) {
        puts("cc_proc: clone tls mmap failed");
        return 1;
    }
    tls_page[0] = TLS_SENTINEL;

    errno = 0;
    long cloned = syscall(SYS_clone, SIGCHLD | CLONE_SETTLS, 0, 0, 0,
                          (long)tls_page, 0);
    if (cloned < 0) {
        munmap(tls_page, 4096);
        puts("cc_proc: clone tls syscall failed");
        return 1;
    }
    if (cloned == 0) {
        _exit(read_fs_zero() == TLS_SENTINEL ? 0 : 77);
    }

    int status = 0;
    if (waitpid((pid_t)cloned, &status, 0) != (pid_t)cloned ||
        !WIFEXITED(status) || WEXITSTATUS(status) != 0) {
        munmap(tls_page, 4096);
        puts("cc_proc: clone tls wait failed");
        return 1;
    }
    if (munmap(tls_page, 4096) < 0) {
        puts("cc_proc: clone tls munmap failed");
        return 1;
    }

    puts("cc_proc: clone tls ok");
    return 0;
}

static int check_preemptive_scheduling(void) {
    pid_t spinner = fork();
    if (spinner < 0) {
        puts("cc_proc: preempt spinner fork failed");
        return 1;
    }
    if (spinner == 0) {
        for (;;) {
            __asm__ volatile("" ::: "memory");
        }
    }

    pid_t observer = fork();
    if (observer < 0) {
        kill(spinner, SIGKILL);
        waitpid(spinner, NULL, 0);
        puts("cc_proc: preempt observer fork failed");
        return 1;
    }
    if (observer == 0) {
        _exit(0);
    }

    int status = 0;
    if (waitpid(observer, &status, 0) != observer || !WIFEXITED(status) ||
        WEXITSTATUS(status) != 0) {
        kill(spinner, SIGKILL);
        waitpid(spinner, NULL, 0);
        puts("cc_proc: preempt observer wait failed");
        return 1;
    }
    if (kill(spinner, SIGKILL) < 0) {
        puts("cc_proc: preempt spinner kill failed");
        return 1;
    }
    if (waitpid(spinner, &status, 0) != spinner || !WIFSIGNALED(status) ||
        WTERMSIG(status) != SIGKILL) {
        puts("cc_proc: preempt spinner wait failed");
        return 1;
    }

    puts("cc_proc: preemptive scheduling ok");
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
    if (check_clone_sigchld() != 0) {
        return 1;
    }
    if (check_clone_tls() != 0) {
        return 1;
    }
    if (check_preemptive_scheduling() != 0) {
        return 1;
    }
    if (check_elf_runtime_permissions() != 0) {
        return 1;
    }
    if (check_user_fault_containment() != 0) {
        return 1;
    }
    if (check_exec_vector_limits() != 0) {
        return 1;
    }
    if (check_exec_long_strings() != 0) {
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
    if (check_exec_reserved_segment() != 0) {
        return 1;
    }
    if (check_exec_overlapping_segment() != 0) {
        return 1;
    }
    if (check_exec_wx_segment() != 0) {
        return 1;
    }
    puts("cc_proc: done");
    return 0;
}
