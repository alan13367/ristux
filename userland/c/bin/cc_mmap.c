#include <errno.h>
#include <fcntl.h>
#include <limits.h>
#include <stdio.h>
#include <string.h>
#include <sys/mman.h>
#include <sys/wait.h>
#include <unistd.h>

static int execute_probe_status(void *entry) {
    pid_t child = fork();
    if (child < 0) {
        return -1;
    }
    if (child == 0) {
        void (*fn)(void) = (void (*)(void))entry;
        fn();
        _exit(0);
    }

    int status = 0;
    if (waitpid(child, &status, 0) != child) {
        return -1;
    }
    if (WIFEXITED(status)) {
        return WEXITSTATUS(status);
    }
    if (WIFSIGNALED(status)) {
        return 128 + WTERMSIG(status);
    }
    return -1;
}

static int check_brk_shrink(void) {
    char *original = sbrk(0);
    unsigned long page = ((unsigned long)original + 4095UL) & ~4095UL;
    char *kept = (char *)page;
    char *freed = kept + 4096;

    if (brk(freed + 4096) < 0) {
        printf("cc_mmap: brk grow failed errno=%d\n", errno);
        return 1;
    }
    kept[0] = 'h';
    freed[0] = 'x';
    if (brk(freed) < 0) {
        printf("cc_mmap: brk shrink failed errno=%d\n", errno);
        return 1;
    }
    if (kept[0] != 'h') {
        puts("cc_mmap: brk kept page failed");
        return 1;
    }

    int zero_fd = open("/dev/zero", O_RDONLY, 0);
    if (zero_fd < 0) {
        puts("cc_mmap: brk zero open failed");
        return 1;
    }
    errno = 0;
    int stale_rejected = read(zero_fd, freed, 1) == -1 && errno == EFAULT;
    close(zero_fd);
    if (brk(original) < 0) {
        printf("cc_mmap: brk restore failed errno=%d\n", errno);
        return 1;
    }
    if (!stale_rejected) {
        printf("cc_mmap: brk stale page errno=%d\n", errno);
        return 1;
    }
    puts("cc_mmap: brk shrink ok");
    return 0;
}

static int check_brk_bounds(void) {
    void *original = sbrk(0);
    errno = 0;
    if (brk((void *)~0UL) != -1 || errno != ENOMEM) {
        printf("cc_mmap: brk high bound errno=%d\n", errno);
        return 1;
    }
    if (sbrk(0) != original) {
        puts("cc_mmap: brk high changed break");
        return 1;
    }
    errno = 0;
    if (brk((void *)0x1000) != -1 || errno != ENOMEM) {
        printf("cc_mmap: brk low bound errno=%d\n", errno);
        return 1;
    }
    if (sbrk(0) != original) {
        puts("cc_mmap: brk low changed break");
        return 1;
    }
    puts("cc_mmap: brk bounds ok");
    return 0;
}

static int check_high_user_pointer_rejected(void) {
    int zero_fd = open("/dev/zero", O_RDONLY, 0);
    if (zero_fd < 0) {
        puts("cc_mmap: high pointer zero open failed");
        return 1;
    }
    errno = 0;
    int ok = read(zero_fd, (void *)(~0UL - 1), 1) == -1 && errno == EFAULT;
    close(zero_fd);
    if (!ok) {
        printf("cc_mmap: high pointer errno=%d\n", errno);
        return 1;
    }
    puts("cc_mmap: high pointer ok");
    return 0;
}

int main(void) {
    if (check_brk_shrink() != 0) {
        return 1;
    }
    if (check_brk_bounds() != 0) {
        return 1;
    }
    if (check_high_user_pointer_rejected() != 0) {
        return 1;
    }

    char *anon = mmap(NULL, 8192, PROT_READ | PROT_WRITE, MAP_PRIVATE | MAP_ANONYMOUS, -1, 0);
    if (anon == MAP_FAILED) {
        printf("cc_mmap: anonymous failed errno=%d\n", errno);
        return 1;
    }
    anon[0] = 'r';
    anon[4096] = 'x';
    if (anon[0] != 'r' || anon[4096] != 'x') {
        puts("cc_mmap: anonymous contents failed");
        return 1;
    }
    puts("cc_mmap: anonymous ok");

    if (mprotect(anon + 4096, 4096, PROT_READ) < 0) {
        printf("cc_mmap: mprotect failed errno=%d\n", errno);
        return 1;
    }
    int zero_fd = open("/dev/zero", O_RDONLY, 0);
    if (zero_fd < 0) {
        puts("cc_mmap: zero open failed");
        return 1;
    }
    errno = 0;
    if (read(zero_fd, anon + 4096, 1) != -1 || errno != EFAULT) {
        printf("cc_mmap: readonly read target errno=%d\n", errno);
        return 1;
    }
    close(zero_fd);
    puts("cc_mmap: readonly syscall protection ok");
    puts("cc_mmap: mprotect ok");

    if (mprotect(anon + 4096, 4096, PROT_NONE) < 0) {
        printf("cc_mmap: prot none failed errno=%d\n", errno);
        return 1;
    }
    zero_fd = open("/dev/zero", O_RDONLY, 0);
    if (zero_fd < 0) {
        puts("cc_mmap: zero reopen failed");
        return 1;
    }
    errno = 0;
    if (read(zero_fd, anon + 4096, 1) != -1 || errno != EFAULT) {
        printf("cc_mmap: prot none read target errno=%d\n", errno);
        return 1;
    }
    close(zero_fd);
    if (mprotect(anon + 4096, 4096, PROT_READ | PROT_WRITE) < 0) {
        printf("cc_mmap: prot restore failed errno=%d\n", errno);
        return 1;
    }
    anon[4096] = 'n';
    if (anon[4096] != 'n') {
        puts("cc_mmap: prot restore write failed");
        return 1;
    }
    puts("cc_mmap: prot none ok");

    if (munmap(anon, 8192) < 0) {
        printf("cc_mmap: munmap failed errno=%d\n", errno);
        return 1;
    }
    puts("cc_mmap: munmap ok");

    unsigned char *nx = mmap(NULL, 4096, PROT_READ | PROT_WRITE,
                             MAP_PRIVATE | MAP_ANONYMOUS, -1, 0);
    if (nx == MAP_FAILED) {
        printf("cc_mmap: nx mmap failed errno=%d\n", errno);
        return 1;
    }
    nx[0] = 0xc3; /* ret */
    errno = 0;
    if (mprotect(nx, 4096, PROT_READ | PROT_WRITE | PROT_EXEC) != -1 ||
        errno != EINVAL) {
        printf("cc_mmap: wx rejection failed errno=%d\n", errno);
        return 1;
    }
    if (mprotect(nx, 4096, PROT_READ | PROT_EXEC) < 0) {
        printf("cc_mmap: rx mprotect failed errno=%d\n", errno);
        return 1;
    }
    int rx_status = execute_probe_status(nx);
    if (rx_status != 0) {
        printf("cc_mmap: rx execute status=%d\n", rx_status);
        return 1;
    }
    if (munmap(nx, 4096) < 0) {
        puts("cc_mmap: nx munmap failed");
        return 1;
    }
    puts("cc_mmap: nx wx ok");

    char *fixed_base = mmap(NULL, 4096, PROT_READ | PROT_WRITE,
                            MAP_PRIVATE | MAP_ANONYMOUS, -1, 0);
    if (fixed_base == MAP_FAILED) {
        printf("cc_mmap: fixed base failed errno=%d\n", errno);
        return 1;
    }
    fixed_base[0] = 'o';
    char *fixed = mmap(fixed_base, 4096, PROT_READ | PROT_WRITE,
                       MAP_PRIVATE | MAP_ANONYMOUS | MAP_FIXED, -1, 0);
    if (fixed != fixed_base) {
        printf("cc_mmap: fixed failed errno=%d\n", errno);
        return 1;
    }
    if (fixed[0] != 0) {
        puts("cc_mmap: fixed replacement failed");
        return 1;
    }
    fixed[0] = 'f';
    if (fixed[0] != 'f') {
        puts("cc_mmap: fixed write failed");
        return 1;
    }
    if (munmap(fixed, 4096) < 0) {
        puts("cc_mmap: fixed munmap failed");
        return 1;
    }
    puts("cc_mmap: fixed ok");

    const char *payload = "file backed mmap ok";
    int fd = open("/tmp/cc_mmap.txt", O_CREAT | O_TRUNC | O_RDWR, 0644);
    if (fd < 0) {
        puts("cc_mmap: open failed");
        return 1;
    }
    if (write(fd, payload, strlen(payload)) != (ssize_t)strlen(payload)) {
        puts("cc_mmap: write failed");
        return 1;
    }
    errno = 0;
    if (mmap(NULL, 4096, PROT_READ, MAP_PRIVATE, fd, -4096) != MAP_FAILED ||
        errno != EINVAL) {
        printf("cc_mmap: negative offset errno=%d\n", errno);
        return 1;
    }
    puts("cc_mmap: offset ok");

    char *file = mmap(NULL, 4096, PROT_READ, MAP_PRIVATE, fd, 0);
    close(fd);
    if (file == MAP_FAILED) {
        printf("cc_mmap: file mmap failed errno=%d\n", errno);
        return 1;
    }
    if (memcmp(file, payload, strlen(payload)) != 0) {
        puts("cc_mmap: file contents failed");
        return 1;
    }
    puts("cc_mmap: file ok");

    if (munmap(file, 4096) < 0) {
        puts("cc_mmap: file munmap failed");
        return 1;
    }

    fd = open("/tmp/cc_mmap_multi.txt", O_CREAT | O_TRUNC | O_RDWR, 0644);
    if (fd < 0) {
        puts("cc_mmap: multi open failed");
        return 1;
    }
    char page[4096];
    memset(page, 'a', sizeof(page));
    page[0] = '0';
    if (write(fd, page, sizeof(page)) != (ssize_t)sizeof(page)) {
        close(fd);
        puts("cc_mmap: multi write first failed");
        return 1;
    }
    memset(page, 'b', sizeof(page));
    page[0] = '1';
    if (write(fd, page, sizeof(page)) != (ssize_t)sizeof(page)) {
        close(fd);
        puts("cc_mmap: multi write second failed");
        return 1;
    }
    memset(page, 'c', sizeof(page));
    page[0] = '2';
    page[sizeof(page) - 1] = 'z';
    if (write(fd, page, sizeof(page)) != (ssize_t)sizeof(page)) {
        close(fd);
        puts("cc_mmap: multi write third failed");
        return 1;
    }
    char *multi = mmap(NULL, sizeof(page) * 3, PROT_READ, MAP_PRIVATE, fd, 0);
    close(fd);
    if (multi == MAP_FAILED) {
        printf("cc_mmap: multi mmap failed errno=%d\n", errno);
        return 1;
    }
    if (multi[0] != '0' || multi[4096] != '1' || multi[8192] != '2' ||
        multi[12287] != 'z') {
        puts("cc_mmap: multi contents failed");
        return 1;
    }
    if (munmap(multi, sizeof(page) * 3) < 0) {
        puts("cc_mmap: multi munmap failed");
        return 1;
    }
    puts("cc_mmap: file multi ok");

    const char *shared_initial = "shared mmap initial";
    const char *shared_changed = "shared mmap changed";
    fd = open("/tmp/cc_mmap_shared.txt", O_CREAT | O_TRUNC | O_RDWR, 0644);
    if (fd < 0) {
        puts("cc_mmap: shared open failed");
        return 1;
    }
    if (write(fd, shared_initial, strlen(shared_initial)) != (ssize_t)strlen(shared_initial)) {
        puts("cc_mmap: shared write failed");
        return 1;
    }
    char *shared = mmap(NULL, 4096, PROT_READ | PROT_WRITE, MAP_SHARED, fd, 0);
    close(fd);
    if (shared == MAP_FAILED) {
        printf("cc_mmap: shared mmap failed errno=%d\n", errno);
        return 1;
    }
    memcpy(shared, shared_changed, strlen(shared_changed));
    if (msync(shared, 4096, MS_SYNC) < 0) {
        printf("cc_mmap: msync failed errno=%d\n", errno);
        return 1;
    }
    if (munmap(shared, 4096) < 0) {
        puts("cc_mmap: shared munmap failed");
        return 1;
    }
    fd = open("/tmp/cc_mmap_shared.txt", O_RDONLY, 0);
    if (fd < 0) {
        puts("cc_mmap: shared reopen failed");
        return 1;
    }
    char shared_buf[32];
    int shared_read = read(fd, shared_buf, sizeof(shared_buf));
    close(fd);
    if (shared_read < (int)strlen(shared_changed) ||
        memcmp(shared_buf, shared_changed, strlen(shared_changed)) != 0) {
        puts("cc_mmap: shared contents failed");
        return 1;
    }
    puts("cc_mmap: shared ok");

    fd = open("/tmp/cc_mmap_shared.txt", O_RDWR, 0);
    if (fd < 0) {
        puts("cc_mmap: shared range open failed");
        return 1;
    }
    long huge_offset = LONG_MAX & ~4095L;
    errno = 0;
    void *bad_shared = mmap(NULL, 4096, PROT_READ | PROT_WRITE, MAP_SHARED, fd, huge_offset);
    close(fd);
    if (bad_shared != MAP_FAILED || errno != EINVAL) {
        if (bad_shared != MAP_FAILED) {
            munmap(bad_shared, 4096);
        }
        printf("cc_mmap: shared range failed errno=%d\n", errno);
        return 1;
    }
    puts("cc_mmap: shared range ok");

    puts("cc_mmap: done");
    return 0;
}
