#define _POSIX_C_SOURCE 200809L

#include <dirent.h>
#include <errno.h>
#include <fcntl.h>
#include <signal.h>
#include <stdio.h>
#include <string.h>
#include <sys/stat.h>
#include <time.h>
#include <unistd.h>

static int expect(int condition, const char *message) {
    if (!condition) {
        printf("cc_newlib_posix: %s failed errno=%d\n", message, errno);
        return 1;
    }
    return 0;
}

static volatile int saw_usr1;

static void on_usr1(int signum) {
    if (signum == SIGUSR1) {
        saw_usr1 = 1;
    }
}

static int read_exact(int fd, const char *expected) {
    char buf[32];
    memset(buf, 0, sizeof(buf));
    ssize_t n = read(fd, buf, sizeof(buf) - 1);
    if (n < 0) {
        return 0;
    }
    return strcmp(buf, expected) == 0;
}

static int directory_contains(const char *path, const char *needle) {
    DIR *dir = opendir(path);
    if (dir == NULL) {
        return 0;
    }
    int found = 0;
    struct dirent *entry;
    while ((entry = readdir(dir)) != NULL) {
        if (strcmp(entry->d_name, needle) == 0) {
            found = 1;
            break;
        }
    }
    closedir(dir);
    return found;
}

int main(void) {
    puts("cc_newlib_posix: start");

    if (mkdir("/tmp/newlib_posix", 0700) < 0 && errno != EEXIST) {
        printf("cc_newlib_posix: mkdir failed errno=%d\n", errno);
        return 1;
    }
    if (expect(chdir("/tmp/newlib_posix") == 0, "chdir")) {
        return 1;
    }

    char cwd[128];
    if (expect(getcwd(cwd, sizeof(cwd)) == cwd, "getcwd")) {
        return 1;
    }
    if (expect(strcmp(cwd, "/tmp/newlib_posix") == 0, "cwd match")) {
        return 1;
    }
    if (expect(access(".", R_OK | W_OK | X_OK) == 0, "access")) {
        return 1;
    }
    int dir_item = open("dir_item", O_CREAT | O_TRUNC | O_RDWR, 0600);
    if (expect(dir_item >= 0, "dir item create")) {
        return 1;
    }
    close(dir_item);
    if (expect(directory_contains(".", "dir_item"), "readdir")) {
        return 1;
    }
    if (expect(unlink("dir_item") == 0, "dir item unlink")) {
        return 1;
    }

    mode_t old_mask = umask(0022);
    (void)umask(old_mask);
    if (expect(getuid() == geteuid() && getgid() == getegid(), "uid gid")) {
        return 1;
    }
    puts("cc_newlib_posix: cwd dirs ok");

    const char *target = "newlib symlink target";
    if (expect(symlink(target, "link0") == 0, "symlink")) {
        return 1;
    }
    char link_buf[64];
    memset(link_buf, 0, sizeof(link_buf));
    ssize_t link_len = readlink("link0", link_buf, sizeof(link_buf) - 1);
    if (expect(link_len == (ssize_t)strlen(target) && strcmp(link_buf, target) == 0, "readlink")) {
        return 1;
    }
    if (expect(unlink("link0") == 0, "unlink symlink")) {
        return 1;
    }
    puts("cc_newlib_posix: links ok");

    int fds[2];
    if (expect(pipe(fds) == 0, "pipe")) {
        return 1;
    }
    int write_dup = dup(fds[1]);
    if (expect(write_dup >= 0, "dup")) {
        return 1;
    }
    if (expect(write(write_dup, "pipe ok", 7) == 7, "pipe write")) {
        return 1;
    }
    close(write_dup);
    close(fds[1]);
    if (expect(read_exact(fds[0], "pipe ok"), "pipe read")) {
        return 1;
    }
    close(fds[0]);

    if (expect(pipe(fds) == 0, "pipe2")) {
        return 1;
    }
    if (expect(dup2(fds[1], 12) == 12, "dup2")) {
        return 1;
    }
    if (expect(write(12, "dup2 ok", 7) == 7, "dup2 write")) {
        return 1;
    }
    close(12);
    close(fds[1]);
    if (expect(read_exact(fds[0], "dup2 ok"), "dup2 read")) {
        return 1;
    }
    close(fds[0]);
    puts("cc_newlib_posix: pipes ok");

    sigset_t set;
    sigset_t oldset;
    sigset_t pending;
    sigemptyset(&set);
    sigaddset(&set, SIGUSR1);
    if (expect(sigprocmask(SIG_BLOCK, &set, &oldset) == 0, "sig block")) {
        return 1;
    }
    if (expect(sigpending(&pending) == 0, "sigpending")) {
        return 1;
    }
    if (expect(!sigismember(&pending, SIGUSR1), "pending clear")) {
        return 1;
    }
    if (expect(sigprocmask(SIG_SETMASK, &oldset, NULL) == 0, "sig restore")) {
        return 1;
    }
    if (expect(kill(getpid(), 0) == 0, "kill self 0")) {
        return 1;
    }

    struct sigaction act;
    struct sigaction oldact;
    sigemptyset(&act.sa_mask);
    act.sa_flags = 0;
    act.sa_handler = on_usr1;
    if (expect(sigaction(SIGUSR1, &act, &oldact) == 0, "sigaction install")) {
        return 1;
    }
    if (expect(kill(getpid(), SIGUSR1) == 0, "kill usr1")) {
        return 1;
    }
    if (expect(saw_usr1 == 1, "handler seen")) {
        return 1;
    }
    if (expect(sigaction(SIGUSR1, NULL, &oldact) == 0 && oldact.sa_handler == on_usr1, "sigaction query")) {
        return 1;
    }
    puts("cc_newlib_posix: signals ok");

    time_t stored = 0;
    time_t now = time(&stored);
    if (expect(now != (time_t)-1 && stored == now, "time")) {
        return 1;
    }
    struct timespec ts;
    if (expect(clock_gettime(CLOCK_REALTIME, &ts) == 0 && ts.tv_sec > 0, "clock realtime")) {
        return 1;
    }
    struct timespec zero = { 0, 0 };
    if (expect(nanosleep(&zero, NULL) == 0, "nanosleep")) {
        return 1;
    }
    puts("cc_newlib_posix: time ok");

    if (expect(chdir("/tmp") == 0, "chdir tmp")) {
        return 1;
    }
    if (expect(rmdir("/tmp/newlib_posix") == 0, "rmdir")) {
        return 1;
    }
    puts("cc_newlib_posix: done");
    return 0;
}
