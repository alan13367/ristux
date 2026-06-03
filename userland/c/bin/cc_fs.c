#include <dirent.h>
#include <errno.h>
#include <fcntl.h>
#include <stdio.h>
#include <string.h>
#include <sys/stat.h>
#include <sys/time.h>
#include <time.h>
#include <unistd.h>
#include <utime.h>

static int dir_contains(int fd, const char *needle) {
    char storage[512];
    int nread = getdents64(fd, (struct linux_dirent64 *)storage, sizeof(storage));
    if (nread < 0) {
        return 0;
    }
    for (int off = 0; off < nread;) {
        struct linux_dirent64 *ent = (struct linux_dirent64 *)(storage + off);
        if (strcmp(ent->d_name, needle) == 0) {
            return 1;
        }
        off += ent->d_reclen;
    }
    return 0;
}

static int touch_file(const char *path) {
    int fd = open(path, O_CREAT | O_TRUNC | O_WRONLY, 0644);
    if (fd < 0) {
        return -1;
    }
    close(fd);
    return 0;
}

static int check_getdents_short_buffer(void) {
    const char *dir = "/tmp/cc_fs_partial";
    const char *short_path = "/tmp/cc_fs_partial/a";
    char long_name[72];
    char long_path[96];
    memset(long_name, 'z', sizeof(long_name) - 1);
    long_name[sizeof(long_name) - 1] = '\0';
    strcpy(long_path, dir);
    strcat(long_path, "/");
    strcat(long_path, long_name);

    if (mkdir(dir, 0755) < 0 && errno != EEXIST) {
        puts("cc_fs: getdents partial mkdir failed");
        return 1;
    }
    if (touch_file(short_path) < 0 || touch_file(long_path) < 0) {
        puts("cc_fs: getdents partial create failed");
        return 1;
    }

    int fd = open(dir, O_RDONLY, 0);
    if (fd < 0) {
        puts("cc_fs: getdents partial open failed");
        return 1;
    }

    char small[24];
    memset(small, 0, sizeof(small));
    errno = 0;
    int nread = getdents64(fd, (struct linux_dirent64 *)small, sizeof(small));
    if (nread != (int)sizeof(small)) {
        close(fd);
        printf("cc_fs: getdents partial first errno=%d nread=%d\n", errno, nread);
        return 1;
    }
    struct linux_dirent64 *first = (struct linux_dirent64 *)small;
    if (first->d_reclen != sizeof(small) || strcmp(first->d_name, "a") != 0) {
        close(fd);
        puts("cc_fs: getdents partial first entry failed");
        return 1;
    }

    char large[256];
    memset(large, 0, sizeof(large));
    nread = getdents64(fd, (struct linux_dirent64 *)large, sizeof(large));
    close(fd);
    if (nread <= 0) {
        printf("cc_fs: getdents partial second nread=%d\n", nread);
        return 1;
    }
    struct linux_dirent64 *second = (struct linux_dirent64 *)large;
    if (strcmp(second->d_name, long_name) != 0) {
        puts("cc_fs: getdents partial resume failed");
        return 1;
    }

    if (unlink(short_path) != 0 || unlink(long_path) != 0 || rmdir(dir) != 0) {
        puts("cc_fs: getdents partial cleanup failed");
        return 1;
    }
    puts("cc_fs: getdents partial ok");
    return 0;
}

int main(void) {
    const char *dir = "/tmp/cc_fs";
    const char *path = "/tmp/cc_fs/item";
    const char *masked = "/tmp/cc_fs/masked";
    const char *maskdir = "/tmp/cc_fs/maskdir";
    const char *missing = "/tmp/cc_fs/missing_trunc";
    const char *exclusive = "/tmp/cc_fs/exclusive";

    if (mkdir(dir, 0755) < 0 && errno != EEXIST) {
        puts("cc_fs: mkdir failed");
        return 1;
    }

    int fd = open(path, O_CREAT | O_TRUNC | O_WRONLY, 0644);
    if (fd < 0) {
        puts("cc_fs: create failed");
        return 1;
    }
    if (write(fd, "ok", 2) != 2) {
        puts("cc_fs: write failed");
        return 1;
    }
    close(fd);

    if (access(path, F_OK | R_OK) != 0) {
        puts("cc_fs: access failed");
        return 1;
    }
    puts("cc_fs: access ok");

    fd = open(dir, O_RDONLY, 0);
    if (fd < 0) {
        puts("cc_fs: opendir failed");
        return 1;
    }
    int found = dir_contains(fd, "item");
    close(fd);
    if (!found) {
        puts("cc_fs: getdents missing item");
        return 1;
    }
    puts("cc_fs: getdents ok");
    if (check_getdents_short_buffer() != 0) {
        return 1;
    }

    int dirfd = open(dir, O_RDONLY, 0);
    if (dirfd < 0) {
        puts("cc_fs: openat dir failed");
        return 1;
    }
    fd = openat(dirfd, "atfile", O_CREAT | O_TRUNC | O_RDWR, 0644);
    if (fd < 0) {
        puts("cc_fs: openat create failed");
        return 1;
    }
    if (write(fd, "at-ok", 5) != 5) {
        puts("cc_fs: openat write failed");
        return 1;
    }
    close(fd);

    struct stat st;
    if (fstatat(dirfd, "atfile", &st, 0) != 0 || st.st_size != 5) {
        puts("cc_fs: fstatat failed");
        return 1;
    }
    if (faccessat(dirfd, "atfile", R_OK | W_OK, 0) != 0) {
        puts("cc_fs: faccessat failed");
        return 1;
    }
    int notdir = open(path, O_RDONLY, 0);
    if (notdir < 0) {
        puts("cc_fs: notdir open failed");
        return 1;
    }
    errno = 0;
    int bad_at = openat(notdir, "child", O_RDONLY, 0);
    if (bad_at >= 0 || errno != ENOTDIR) {
        if (bad_at >= 0) {
            close(bad_at);
        }
        puts("cc_fs: openat notdir failed");
        return 1;
    }
    close(notdir);
    if (fstatat(AT_FDCWD, "/tmp/cc_fs/atfile", &st, AT_SYMLINK_NOFOLLOW) != 0 ||
        st.st_size != 5) {
        puts("cc_fs: fstatat at_fdcwd failed");
        return 1;
    }
    if (mkdirat(dirfd, "atdir", 0755) != 0) {
        puts("cc_fs: mkdirat failed");
        return 1;
    }
    if (renameat(dirfd, "atfile", dirfd, "atdir/moved") != 0) {
        puts("cc_fs: renameat failed");
        return 1;
    }
    if (linkat(dirfd, "atdir/moved", dirfd, "hardat", 0) != 0) {
        puts("cc_fs: linkat failed");
        return 1;
    }
    if (symlinkat("atdir/moved", dirfd, "symat") != 0) {
        puts("cc_fs: symlinkat failed");
        return 1;
    }
    char linkbuf[32];
    int link_len = readlinkat(dirfd, "symat", linkbuf, sizeof(linkbuf) - 1);
    if (link_len != 11) {
        puts("cc_fs: readlinkat failed");
        return 1;
    }
    linkbuf[link_len] = '\0';
    if (strcmp(linkbuf, "atdir/moved") != 0) {
        puts("cc_fs: readlinkat target failed");
        return 1;
    }
    if (fchmodat(dirfd, "atdir/moved", 0600, 0) != 0 ||
        fstatat(dirfd, "atdir/moved", &st, 0) != 0 ||
        (st.st_mode & 0777) != 0600) {
        puts("cc_fs: fchmodat failed");
        return 1;
    }
    if (fchownat(dirfd, "atdir/moved", 0, 0, 0) != 0) {
        puts("cc_fs: fchownat failed");
        return 1;
    }
    fd = openat(dirfd, "atdir/moved", O_RDWR, 0);
    if (fd < 0) {
        puts("cc_fs: fd metadata open failed");
        return 1;
    }
    if (fchmod(fd, 0644) != 0 ||
        fchown(fd, 0, 0) != 0 ||
        fstat(fd, &st) != 0 ||
        (st.st_mode & 0777) != 0644) {
        puts("cc_fs: fd metadata syscalls failed");
        return 1;
    }
    close(fd);
    puts("cc_fs: fd metadata syscalls ok");

    struct timespec ts[2] = {{1111, 0}, {1234, 0}};
    if (utimensat(AT_FDCWD, "/tmp/cc_fs/atdir/moved", ts, 0) != 0 ||
        stat("/tmp/cc_fs/atdir/moved", &st) != 0 ||
        st.st_mtime != 1234) {
        puts("cc_fs: utimensat failed");
        return 1;
    }
    fd = openat(dirfd, "atdir/moved", O_RDWR, 0);
    if (fd < 0) {
        puts("cc_fs: futimens open failed");
        return 1;
    }
    ts[1].tv_sec = 2345;
    if (futimens(fd, ts) != 0) {
        puts("cc_fs: futimens failed");
        return 1;
    }
    close(fd);
    if (stat("/tmp/cc_fs/atdir/moved", &st) != 0 || st.st_mtime != 2345) {
        puts("cc_fs: futimens stat failed");
        return 1;
    }
    struct utimbuf ub = {3333, 3456};
    if (utime("/tmp/cc_fs/atdir/moved", &ub) != 0 ||
        stat("/tmp/cc_fs/atdir/moved", &st) != 0 ||
        st.st_mtime != 3456) {
        puts("cc_fs: utime failed");
        return 1;
    }
    struct timeval tv[2] = {{4444, 0}, {4567, 0}};
    if (utimes("/tmp/cc_fs/atdir/moved", tv) != 0 ||
        stat("/tmp/cc_fs/atdir/moved", &st) != 0 ||
        st.st_mtime != 4567) {
        puts("cc_fs: utimes failed");
        return 1;
    }
    puts("cc_fs: timestamps ok");

    if (unlinkat(dirfd, "hardat", 0) != 0 ||
        unlinkat(dirfd, "symat", 0) != 0 ||
        unlinkat(dirfd, "atdir/moved", 0) != 0 ||
        unlinkat(dirfd, "atdir", AT_REMOVEDIR) != 0) {
        puts("cc_fs: unlinkat cleanup failed");
        return 1;
    }
    close(dirfd);
    puts("cc_fs: at syscalls ok");

    mode_t old_mask = umask(0027);
    fd = open(masked, O_CREAT | O_TRUNC | O_WRONLY, 0666);
    if (fd < 0) {
        puts("cc_fs: umask create failed");
        return 1;
    }
    close(fd);

    if (stat(masked, &st) != 0 || (st.st_mode & 0777) != 0640) {
        puts("cc_fs: umask file mode failed");
        return 1;
    }
    if (mkdir(maskdir, 0777) != 0) {
        puts("cc_fs: umask mkdir failed");
        return 1;
    }
    if (stat(maskdir, &st) != 0 || (st.st_mode & 0777) != 0750) {
        puts("cc_fs: umask dir mode failed");
        return 1;
    }
    umask(old_mask);
    puts("cc_fs: umask ok");

    fd = open(missing, O_WRONLY | O_TRUNC, 0644);
    if (fd >= 0 || errno != ENOENT) {
        if (fd >= 0) {
            close(fd);
        }
        puts("cc_fs: trunc missing failed");
        return 1;
    }
    puts("cc_fs: trunc missing ok");

    fd = open(exclusive, O_CREAT | O_EXCL | O_WRONLY, S_IRUSR | S_IWUSR);
    if (fd < 0) {
        puts("cc_fs: exclusive create failed");
        return 1;
    }
    close(fd);
    if (stat(exclusive, &st) != 0 || (st.st_mode & 0777) != 0600) {
        puts("cc_fs: exclusive mode failed");
        return 1;
    }
    fd = open(exclusive, O_CREAT | O_EXCL | O_WRONLY, S_IRUSR | S_IWUSR);
    if (fd >= 0 || errno != EEXIST) {
        if (fd >= 0) {
            close(fd);
        }
        puts("cc_fs: exclusive existing failed");
        return 1;
    }
    puts("cc_fs: exclusive create ok");

    if (unlink(masked) != 0 || rmdir(maskdir) != 0) {
        puts("cc_fs: cleanup failed");
        return 1;
    }
    if (unlink(exclusive) != 0) {
        puts("cc_fs: exclusive cleanup failed");
        return 1;
    }
    if (unlink(path) != 0) {
        puts("cc_fs: unlink failed");
        return 1;
    }
    if (access(path, F_OK) == 0) {
        puts("cc_fs: unlink left file");
        return 1;
    }

    puts("cc_fs: done");
    return 0;
}
