#include <errno.h>
#include <fcntl.h>
#include <math.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/stat.h>
#include <sys/wait.h>
#include <unistd.h>

static int cmp_ints(const void *a, const void *b) {
    int left = *(const int *)a;
    int right = *(const int *)b;
    return (left > right) - (left < right);
}

static int check_parse_math(void) {
    char *end = NULL;
    unsigned long long ull = strtoull("0xfftail", &end, 0);
    long long sll = strtoll("-9223372036854775807", NULL, 10);
    double d = strtod(" -12.25e1x", &end);
    long double ld = ldexpl(1.5L, 3);
    if (ull != 255ULL || end == NULL || *end != 'x' ||
        sll != -9223372036854775807LL ||
        d < -122.6 || d > -122.4 ||
        strtof("3.5", NULL) < 3.49f ||
        strtold("4.25", NULL) > 4.26L ||
        ld < 11.99L || ld > 12.01L) {
        puts("cc_libc_hosted: parse math failed");
        return 1;
    }
    puts("cc_libc_hosted: parse math ok");
    return 0;
}

static int check_sort_string_format(void) {
    int values[] = { 4, 1, 3, 2 };
    qsort(values, 4, sizeof(values[0]), cmp_ints);
    char text[64];
    strcpy(text, "tiny");
    strcat(text, "cc");
    char formatted[128];
    sprintf(formatted, "%s-%llu-%02x", text, strtoull("42", NULL, 10), values[0]);
    if (values[0] != 1 || values[3] != 4 ||
        strcmp(formatted, "tinycc-42-01") != 0 ||
        strstr(formatted, "cc-42") == NULL ||
        strpbrk(formatted, "1") != formatted + strlen(formatted) - 1) {
        puts("cc_libc_hosted: sort string format failed");
        return 1;
    }
    puts("cc_libc_hosted: sort string format ok");
    return 0;
}

static int check_stdio_paths(void) {
    mkdir("/tmp/hosted", 0755);
    FILE *fp = fopen("/tmp/hosted/file.txt", "w");
    if (fp == NULL || fputs("alpha\n", fp) == EOF) {
        puts("cc_libc_hosted: fopen write failed");
        return 1;
    }
    if (freopen("/tmp/hosted/file.txt", "r", fp) == NULL) {
        puts("cc_libc_hosted: freopen failed");
        return 1;
    }
    char line[16];
    if (fgets(line, sizeof(line), fp) == NULL || strcmp(line, "alpha\n") != 0 ||
        fclose(fp) < 0) {
        puts("cc_libc_hosted: fread failed");
        return 1;
    }

    char *resolved = realpath("/tmp/hosted/../hosted/file.txt", NULL);
    if (resolved == NULL || strcmp(resolved, "/tmp/hosted/file.txt") != 0) {
        puts("cc_libc_hosted: realpath failed");
        return 1;
    }
    free(resolved);

    if (remove("/tmp/hosted/file.txt") < 0 || access("/tmp/hosted/file.txt", F_OK) == 0) {
        puts("cc_libc_hosted: remove failed");
        return 1;
    }
    puts("cc_libc_hosted: stdio paths ok");
    return 0;
}

static int check_execvp(void) {
    putenv("PATH=/bin");
    pid_t child = vfork();
    if (child < 0) {
        puts("cc_libc_hosted: vfork failed");
        return 1;
    }
    if (child == 0) {
        char *argv[] = { "true", NULL };
        execvp("true", argv);
        _exit(9);
    }
    int status = 0;
    if (waitpid(child, &status, 0) != child ||
        !WIFEXITED(status) ||
        WEXITSTATUS(status) != 0) {
        puts("cc_libc_hosted: execvp failed");
        return 1;
    }
    puts("cc_libc_hosted: execvp ok");
    return 0;
}

int main(void) {
    if (check_parse_math() != 0 ||
        check_sort_string_format() != 0 ||
        check_stdio_paths() != 0 ||
        check_execvp() != 0) {
        return 1;
    }
    puts("cc_libc_hosted: done");
    return 0;
}
