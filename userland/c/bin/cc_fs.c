#include <dirent.h>
#include <errno.h>
#include <fcntl.h>
#include <stdio.h>
#include <string.h>
#include <sys/stat.h>
#include <unistd.h>

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
