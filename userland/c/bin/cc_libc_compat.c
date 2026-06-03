#include <assert.h>
#include <crypt.h>
#include <ctype.h>
#include <errno.h>
#include <fcntl.h>
#include <libgen.h>
#include <limits.h>
#include <setjmp.h>
#include <stdio.h>
#include <stdint.h>
#include <stdlib.h>
#include <string.h>
#include <strings.h>
#include <sys/resource.h>
#include <sys/time.h>
#include <sys/times.h>
#include <sys/types.h>
#include <sys/utsname.h>
#include <sys/wait.h>
#include <syslog.h>
#include <time.h>
#include <unistd.h>

static jmp_buf jump_env;

static int check_ctype(void) {
    if (!isalnum('A') || !isalnum('7') || isalnum('@') ||
        !isalpha('z') || !isdigit('9') || !isspace('\n') ||
        tolower('Q') != 'q' || toupper('q') != 'Q') {
        puts("cc_libc_compat: ctype failed");
        return 1;
    }
    puts("cc_libc_compat: ctype ok");
    return 0;
}

static int check_parse(void) {
    char *end = NULL;
    long neg = strtol(" -0x2a", &end, 0);
    if (neg != -42 || end == NULL || *end != '\0') {
        puts("cc_libc_compat: strtol failed");
        return 1;
    }
    unsigned long mode = strtoul("0755x", &end, 0);
    if (mode != 493 || end == NULL || *end != 'x' || atoi("123") != 123) {
        puts("cc_libc_compat: strtoul failed");
        return 1;
    }
    puts("cc_libc_compat: parse ok");
    return 0;
}

static int check_string(void) {
    char *copy = strdup("DropBear");
    if (copy == NULL) {
        puts("cc_libc_compat: strdup failed");
        return 1;
    }
    int ok = strcmp(copy, "DropBear") == 0 &&
             strncmp(copy, "Drop", 4) == 0 &&
             strrchr(copy, 'B') == copy + 4 &&
             memchr(copy, 'B', strlen(copy)) == copy + 4 &&
             strcasecmp(copy, "dropbear") == 0 &&
             strncasecmp(copy, "DROP", 4) == 0 &&
             strlen(strerror(EINVAL)) > 0 &&
             strcmp(strerror(EPIPE), "Broken pipe") == 0 &&
             strcmp(strerror(ENOSPC), "No space left on device") == 0 &&
             strcmp(strerror(EMLINK), "Too many links") == 0;
    free(copy);
    if (!ok) {
        puts("cc_libc_compat: string failed");
        return 1;
    }
    puts("cc_libc_compat: string ok");
    return 0;
}

static int check_malloc_free(void) {
    char *seed = malloc(4096);
    if (seed == NULL || ((uintptr_t)seed & 15) != 0) {
        puts("cc_libc_compat: malloc alignment failed");
        return 1;
    }
    void *break_after_seed = sbrk(0);
    strcpy(seed, "seed-block");
    free(seed);
    char *reuse = malloc(2048);
    if (reuse == NULL || sbrk(0) != break_after_seed) {
        puts("cc_libc_compat: free reuse failed");
        return 1;
    }
    strcpy(reuse, "reuse");

    char *left = malloc(1024);
    char *right = malloc(1024);
    if (left == NULL || right == NULL) {
        puts("cc_libc_compat: coalesce alloc failed");
        return 1;
    }
    void *break_after_pair = sbrk(0);
    free(left);
    free(right);
    char *combined = malloc(3072);
    if (combined == NULL || sbrk(0) != break_after_pair) {
        puts("cc_libc_compat: coalesce failed");
        return 1;
    }
    free(combined);
    free(reuse);

    char *zeroed = calloc(8, 4);
    if (zeroed == NULL) {
        puts("cc_libc_compat: calloc alloc failed");
        return 1;
    }
    for (int i = 0; i < 32; i++) {
        if (zeroed[i] != 0) {
            puts("cc_libc_compat: calloc zero failed");
            return 1;
        }
    }
    strcpy(zeroed, "allocator");
    char *grown = realloc(zeroed, 128);
    if (grown == NULL || strcmp(grown, "allocator") != 0) {
        puts("cc_libc_compat: realloc grow failed");
        return 1;
    }
    char *shrunk = realloc(grown, 16);
    if (shrunk == NULL || strcmp(shrunk, "allocator") != 0) {
        puts("cc_libc_compat: realloc shrink failed");
        return 1;
    }
    free(shrunk);
    free(NULL);
    puts("cc_libc_compat: malloc free ok");
    return 0;
}

static int check_format(void) {
    char buf[128];
    int n = snprintf(buf, sizeof(buf), "%04o %.3s %02x %lld %zd %.*s",
                     9, "abcdef", 10, (long long)-42, (ssize_t)7, 4, "wxyzq");
    const char *expect = "0011 abc 0a -42 7 wxyz";
    if (n != (int)strlen(expect) || strcmp(buf, expect) != 0) {
        puts("cc_libc_compat: snprintf failed");
        return 1;
    }
    if (snprintf(NULL, 0, "%.2s-%d", "abcd", 3) != 4) {
        puts("cc_libc_compat: snprintf count failed");
        return 1;
    }
    if (fprintf(stderr, "cc_libc_compat: fprintf ok %d\n", 7) < 0) {
        puts("cc_libc_compat: fprintf failed");
        return 1;
    }
    puts("cc_libc_compat: format ok");
    return 0;
}

static int check_path(void) {
    char base_path[] = "/usr/bin/dropbear";
    char dir_path[] = "/usr/bin/dropbear";
    if (strcmp(basename(base_path), "dropbear") != 0 ||
        strcmp(dirname(dir_path), "/usr/bin") != 0) {
        puts("cc_libc_compat: path failed");
        return 1;
    }
    puts("cc_libc_compat: path ok");
    return 0;
}

static int check_getopt(void) {
    char *argv1[] = { "prog", "-ab", "-c", "value", "plain", NULL };
    optind = 1;
    opterr = 0;
    int a = getopt(5, argv1, "abc:");
    int b = getopt(5, argv1, "abc:");
    int c = getopt(5, argv1, "abc:");
    char *c_arg = optarg;
    int done = getopt(5, argv1, "abc:");
    if (a != 'a' || b != 'b' || c != 'c' ||
        c_arg == NULL || strcmp(c_arg, "value") != 0 ||
        done != -1 || optind != 4) {
        puts("cc_libc_compat: getopt clustered failed");
        return 1;
    }

    char *argv2[] = { "prog", "-x", NULL };
    optind = 1;
    optarg = NULL;
    if (getopt(2, argv2, "a") != '?' || optopt != 'x') {
        puts("cc_libc_compat: getopt unknown failed");
        return 1;
    }

    char *argv3[] = { "prog", "-c", NULL };
    optind = 1;
    optarg = NULL;
    if (getopt(2, argv3, ":c:") != ':' || optopt != 'c') {
        puts("cc_libc_compat: getopt missing arg failed");
        return 1;
    }

    char *argv4[] = { "prog", "--", "-a", NULL };
    optind = 1;
    if (getopt(3, argv4, "a") != -1 || optind != 2) {
        puts("cc_libc_compat: getopt double dash failed");
        return 1;
    }

    puts("cc_libc_compat: getopt ok");
    return 0;
}

static int check_sysconf(void) {
    if (getpagesize() != 4096 ||
        sysconf(_SC_PAGESIZE) != 4096 ||
        sysconf(_SC_PAGE_SIZE) != 4096 ||
        sysconf(_SC_OPEN_MAX) != OPEN_MAX ||
        sysconf(_SC_CLK_TCK) != 100 ||
        sysconf(_SC_NPROCESSORS_CONF) < 1 ||
        sysconf(_SC_NPROCESSORS_ONLN) < 1) {
        puts("cc_libc_compat: sysconf values failed");
        return 1;
    }
    errno = 0;
    if (sysconf(99999) != -1 || errno != EINVAL) {
        puts("cc_libc_compat: sysconf invalid failed");
        return 1;
    }
    puts("cc_libc_compat: sysconf ok");
    return 0;
}

static int check_resource_syslog(void) {
    struct rlimit lim;
    if (getrlimit(RLIMIT_CORE, &lim) < 0 || lim.rlim_cur != 0 || lim.rlim_max != 0 ||
        setrlimit(RLIMIT_CORE, &lim) < 0) {
        puts("cc_libc_compat: rlimit failed");
        return 1;
    }
    if (getrlimit(RLIMIT_NOFILE, &lim) < 0 ||
        lim.rlim_cur != OPEN_MAX ||
        lim.rlim_max != OPEN_MAX) {
        puts("cc_libc_compat: nofile rlimit failed");
        return 1;
    }
    struct rlimit lowered = { 8, OPEN_MAX };
    if (setrlimit(RLIMIT_NOFILE, &lowered) < 0 ||
        getrlimit(RLIMIT_NOFILE, &lim) < 0 ||
        lim.rlim_cur != 8 ||
        lim.rlim_max != OPEN_MAX) {
        puts("cc_libc_compat: nofile setrlimit failed");
        return 1;
    }
    struct rlimit restored = { OPEN_MAX, OPEN_MAX };
    if (setrlimit(RLIMIT_NOFILE, &restored) < 0) {
        puts("cc_libc_compat: nofile restore failed");
        return 1;
    }
    errno = 0;
    if (getrlimit(999, &lim) != -1 || errno != EINVAL) {
        puts("cc_libc_compat: invalid getrlimit failed");
        return 1;
    }
    errno = 0;
    if (setrlimit(RLIMIT_NOFILE, NULL) != -1 || errno != EFAULT) {
        puts("cc_libc_compat: setrlimit fault failed");
        return 1;
    }
    errno = 0;
    if (setrlimit(999, NULL) != -1 || errno != EINVAL) {
        puts("cc_libc_compat: invalid setrlimit failed");
        return 1;
    }
    puts("cc_libc_compat: rlimit errors ok");
    openlog("cc_libc_compat", LOG_PID, LOG_AUTHPRIV);
    syslog(LOG_INFO, "syslog ok %d", 42);
    int old = setlogmask(LOG_UPTO(LOG_ERR));
    syslog(LOG_INFO, "hidden syslog line");
    setlogmask(old);
    closelog();
    puts("cc_libc_compat: resource syslog ok");
    return 0;
}

static int check_uname(void) {
    struct utsname uts;
    if (uname(&uts) != 0 ||
        strcmp(uts.sysname, "Ristux") != 0 ||
        strcmp(uts.nodename, "ristux") != 0 ||
        strcmp(uts.release, "0.1.0") != 0 ||
        strcmp(uts.machine, "x86_64") != 0 ||
        strcmp(uts.domainname, "localdomain") != 0) {
        puts("cc_libc_compat: uname failed");
        return 1;
    }
    errno = 0;
    if (uname(NULL) != -1 || errno != EFAULT) {
        puts("cc_libc_compat: uname fault failed");
        return 1;
    }
    char host[32];
    if (gethostname(host, sizeof(host)) != 0 || strcmp(host, "ristux") != 0) {
        puts("cc_libc_compat: gethostname failed");
        return 1;
    }
    if (sethostname("buildhost", 9) != 0) {
        puts("cc_libc_compat: sethostname failed");
        return 1;
    }
    if (gethostname(host, sizeof(host)) != 0 || strcmp(host, "buildhost") != 0) {
        puts("cc_libc_compat: gethostname updated failed");
        return 1;
    }
    if (uname(&uts) != 0 || strcmp(uts.nodename, "buildhost") != 0) {
        puts("cc_libc_compat: uname updated failed");
        return 1;
    }
    char tiny[4];
    errno = 0;
    if (gethostname(tiny, sizeof(tiny)) != -1 || errno != ENAMETOOLONG) {
        puts("cc_libc_compat: gethostname truncation failed");
        return 1;
    }
    if (sethostname("ristux", 6) != 0) {
        puts("cc_libc_compat: hostname restore failed");
        return 1;
    }
    puts("cc_libc_compat: uname ok");
    return 0;
}

static int check_time_format(void) {
    time_t epoch = 0;
    struct tm *tm = localtime(&epoch);
    char buf[32];
    if (tm == NULL ||
        strftime(buf, sizeof(buf), "%b %d %H:%M:%S %Y", tm) == 0 ||
        strcmp(buf, "Jan 01 00:00:00 1970") != 0 ||
        CLOCKS_PER_SEC != 1000000L ||
        clock() < 0) {
        puts("cc_libc_compat: time format failed");
        return 1;
    }
    puts("cc_libc_compat: time format ok");
    return 0;
}

static int check_gettimeofday_fault(void) {
    struct timeval tv = {1234, 5678};
    errno = 0;
    if (gettimeofday(&tv, (struct timezone *)1) != -1 ||
        errno != EFAULT ||
        tv.tv_sec != 1234 ||
        tv.tv_usec != 5678) {
        puts("cc_libc_compat: gettimeofday fault failed");
        return 1;
    }
    puts("cc_libc_compat: gettimeofday fault ok");
    return 0;
}

static int check_process_accounting(void) {
    struct rusage usage;
    if (getrusage(RUSAGE_SELF, &usage) != 0 ||
        usage.ru_utime.tv_sec < 0 ||
        usage.ru_utime.tv_usec < 0 ||
        usage.ru_utime.tv_usec >= 1000000) {
        puts("cc_libc_compat: getrusage self failed");
        return 1;
    }

    memset(&usage, 0xff, sizeof(usage));
    if (getrusage(RUSAGE_CHILDREN, &usage) != 0 ||
        usage.ru_utime.tv_sec != 0 ||
        usage.ru_utime.tv_usec != 0 ||
        usage.ru_stime.tv_sec != 0 ||
        usage.ru_stime.tv_usec != 0) {
        puts("cc_libc_compat: getrusage children failed");
        return 1;
    }

    errno = 0;
    if (getrusage(999, &usage) != -1 || errno != EINVAL) {
        puts("cc_libc_compat: getrusage invalid failed");
        return 1;
    }

    struct tms tms;
    clock_t ticks = times(&tms);
    if (ticks < 0 ||
        tms.tms_utime < 0 ||
        tms.tms_stime != 0 ||
        tms.tms_cutime != 0 ||
        tms.tms_cstime != 0) {
        puts("cc_libc_compat: times failed");
        return 1;
    }
    if (times(NULL) < 0) {
        puts("cc_libc_compat: times null failed");
        return 1;
    }

    puts("cc_libc_compat: process accounting ok");
    return 0;
}

static int check_setjmp(void) {
    volatile int armed = 0;
    int value = setjmp(jump_env);
    if (value == 0) {
        armed = 1;
        longjmp(jump_env, 7);
        puts("cc_libc_compat: longjmp returned");
        return 1;
    }
    if (value != 7 || armed != 1) {
        puts("cc_libc_compat: setjmp value failed");
        return 1;
    }

    value = setjmp(jump_env);
    if (value == 0) {
        longjmp(jump_env, 0);
    }
    if (value != 1) {
        puts("cc_libc_compat: setjmp zero value failed");
        return 1;
    }
    puts("cc_libc_compat: setjmp ok");
    return 0;
}

static int check_dropbear_types(void) {
    fd_set fds;
    FD_ZERO(&fds);
    FD_SET(7, &fds);
    if (!FD_ISSET(7, &fds)) {
        puts("cc_libc_compat: fd_set failed");
        return 1;
    }
    FD_CLR(7, &fds);
    if (FD_ISSET(7, &fds)) {
        puts("cc_libc_compat: fd_clr failed");
        return 1;
    }
    u_int8_t byte = 0x12;
    u_int16_t word = 0x3456;
    u_int32_t dword = 0x789abcdeU;
    clock_t ticks = 0;
    if (byte != 0x12 || word != 0x3456 || dword != 0x789abcdeU ||
        UINT64_MAX != 18446744073709551615UL ||
        INT64_MAX != 9223372036854775807L ||
        SIZE_MAX != UINT64_MAX ||
        ticks != 0) {
        puts("cc_libc_compat: type alias failed");
        return 1;
    }
    puts("cc_libc_compat: dropbear types ok");
    return 0;
}

static int check_crypt(void) {
    char *blank = crypt("", "");
    if (blank == NULL || strcmp(blank, "") != 0) {
        puts("cc_libc_compat: crypt failed");
        return 1;
    }
    errno = 0;
    if (crypt("password", "salt") != NULL || errno != ENOSYS) {
        puts("cc_libc_compat: crypt unsupported failed");
        return 1;
    }
    puts("cc_libc_compat: crypt ok");
    return 0;
}

static int check_stdio_file(void) {
    FILE *fp = fopen("/tmp/cc_libc_stdio.txt", "w");
    if (fp == NULL) {
        puts("cc_libc_compat: fopen write failed");
        return 1;
    }
    if (fprintf(fp, "alpha %d\n", 7) < 0 ||
        fputs("beta", fp) == EOF ||
        fputc('\n', fp) == EOF ||
        fwrite("gamma\n", 1, 6, fp) != 6 ||
        fileno(fp) < 0 ||
        fflush(fp) < 0 ||
        fclose(fp) < 0) {
        puts("cc_libc_compat: stdio write failed");
        return 1;
    }

    fp = fopen("/tmp/cc_libc_stdio.txt", "r");
    if (fp == NULL) {
        puts("cc_libc_compat: fopen read failed");
        return 1;
    }
    char line[32];
    if (fgets(line, sizeof(line), fp) == NULL ||
        strcmp(line, "alpha 7\n") != 0 ||
        fgetc(fp) != 'b' ||
        fseek(fp, 0, SEEK_SET) < 0 ||
        ftell(fp) != 0) {
        puts("cc_libc_compat: stdio read failed");
        fclose(fp);
        return 1;
    }
    char bytes[6];
    if (fread(bytes, 1, sizeof(bytes), fp) != sizeof(bytes) ||
        memcmp(bytes, "alpha ", sizeof(bytes)) != 0 ||
        fclose(fp) < 0) {
        puts("cc_libc_compat: stdio fread failed");
        return 1;
    }
    puts("cc_libc_compat: stdio file ok");
    return 0;
}

static int check_process_env_open(void) {
    char *fixture_env[] = { "HOME=/root", "EMPTY=", "PATH=/bin", NULL };
    environ = fixture_env;
    if (strcmp(getenv("HOME"), "/root") != 0 ||
        strcmp(getenv("EMPTY"), "") != 0 ||
        getenv("NO_SUCH_VAR") != NULL ||
        getenv("BAD=NAME") != NULL) {
        puts("cc_libc_compat: getenv failed");
        return 1;
    }
    if (clearenv() < 0 || getenv("HOME") != NULL ||
        putenv("HOME=/tmp") < 0 ||
        putenv("SHELL=/bin/sh") < 0 ||
        strcmp(getenv("HOME"), "/tmp") != 0 ||
        strcmp(getenv("SHELL"), "/bin/sh") != 0) {
        puts("cc_libc_compat: putenv failed");
        return 1;
    }
    if (setenv("HOME", "/root", 0) < 0 ||
        strcmp(getenv("HOME"), "/tmp") != 0 ||
        setenv("HOME", "/root", 1) < 0 ||
        strcmp(getenv("HOME"), "/root") != 0 ||
        setenv("EDITOR", "vi", 1) < 0 ||
        strcmp(getenv("EDITOR"), "vi") != 0 ||
        unsetenv("EDITOR") < 0 ||
        getenv("EDITOR") != NULL ||
        setenv("BAD=NAME", "x", 1) == 0 ||
        unsetenv("BAD=NAME") == 0) {
        puts("cc_libc_compat: setenv failed");
        return 1;
    }

    int fd = open("/etc/os-release", O_RDONLY);
    if (fd < 0) {
        puts("cc_libc_compat: two arg open failed");
        return 1;
    }
    char byte = 0;
    if (read(fd, &byte, 1) != 1 || close(fd) < 0) {
        puts("cc_libc_compat: two arg open read failed");
        return 1;
    }

    pid_t child = vfork();
    if (child < 0) {
        puts("cc_libc_compat: vfork failed");
        return 1;
    }
    if (child == 0) {
        char *argv[] = { "/bin/true", NULL };
        execv("/bin/true", argv);
        _exit(7);
    }
    int status = 0;
    if (waitpid(child, &status, 0) != child ||
        !WIFEXITED(status) ||
        WEXITSTATUS(status) != 0) {
        puts("cc_libc_compat: execv failed");
        return 1;
    }
    puts("cc_libc_compat: process env open ok");
    return 0;
}

int main(void) {
    assert(PATH_MAX >= 1024);
    if (check_ctype() != 0 ||
        check_parse() != 0 ||
        check_string() != 0 ||
        check_malloc_free() != 0 ||
        check_format() != 0 ||
        check_path() != 0 ||
        check_getopt() != 0 ||
        check_sysconf() != 0 ||
        check_resource_syslog() != 0 ||
        check_uname() != 0 ||
        check_time_format() != 0 ||
        check_gettimeofday_fault() != 0 ||
        check_process_accounting() != 0 ||
        check_setjmp() != 0 ||
        check_dropbear_types() != 0 ||
        check_crypt() != 0 ||
        check_stdio_file() != 0 ||
        check_process_env_open() != 0) {
        return 1;
    }
    puts("cc_libc_compat: done");
    return 0;
}
