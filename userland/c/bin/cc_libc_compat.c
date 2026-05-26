#include <assert.h>
#include <ctype.h>
#include <errno.h>
#include <libgen.h>
#include <limits.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <strings.h>
#include <sys/resource.h>
#include <sys/types.h>
#include <syslog.h>

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
             strcasecmp(copy, "dropbear") == 0 &&
             strncasecmp(copy, "DROP", 4) == 0 &&
             strlen(strerror(EINVAL)) > 0;
    free(copy);
    if (!ok) {
        puts("cc_libc_compat: string failed");
        return 1;
    }
    puts("cc_libc_compat: string ok");
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

static int check_resource_syslog(void) {
    struct rlimit lim;
    if (getrlimit(RLIMIT_CORE, &lim) < 0 || lim.rlim_cur != 0 || lim.rlim_max != 0 ||
        setrlimit(RLIMIT_CORE, &lim) < 0) {
        puts("cc_libc_compat: rlimit failed");
        return 1;
    }
    openlog("cc_libc_compat", LOG_PID, LOG_AUTHPRIV);
    syslog(LOG_INFO, "syslog ok %d", 42);
    int old = setlogmask(LOG_UPTO(LOG_ERR));
    syslog(LOG_INFO, "hidden syslog line");
    setlogmask(old);
    closelog();
    puts("cc_libc_compat: resource syslog ok");
    return 0;
}

int main(void) {
    assert(PATH_MAX >= 1024);
    if (check_ctype() != 0 ||
        check_parse() != 0 ||
        check_string() != 0 ||
        check_format() != 0 ||
        check_path() != 0 ||
        check_resource_syslog() != 0) {
        return 1;
    }
    puts("cc_libc_compat: done");
    return 0;
}
