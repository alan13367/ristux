#include <fcntl.h>
#include <stdio.h>
#include <string.h>
#include <sys/stat.h>
#include <unistd.h>

static const char *persist_dir = "/home/ext2_persist";
static const char *persist_file = "/home/ext2_persist/file";
static const char *persist_moved = "/home/ext2_persist/moved";
static const char *persist_hard = "/home/ext2_persist/hard";
static const char *persist_link = "/home/ext2_persist/link";
static const char *persist_removed = "/home/ext2_persist/removed";
static const char *persist_payload = "hardlink persisted\n";
static const char *marker_tmp = "/home/ext2_reboot_marker.tmp";
static const char *marker = "/home/ext2_reboot_marker";

static void cleanup_persist_tree(void) {
    unlink(persist_link);
    unlink(persist_removed);
    unlink(persist_file);
    unlink(persist_moved);
    unlink(persist_hard);
    rmdir(persist_dir);
}

static int read_exact_file(const char *path, const char *expected) {
    char buf[64];
    int fd = open(path, O_RDONLY, 0);
    if (fd < 0) {
        return -1;
    }
    ssize_t nread = read(fd, buf, sizeof(buf) - 1);
    close(fd);
    if (nread < 0) {
        return -1;
    }
    buf[nread] = '\0';
    return strcmp(buf, expected) == 0 ? 0 : -1;
}

static int setup_reboot_torture(void) {
    cleanup_persist_tree();
    unlink(marker_tmp);
    unlink(marker);

    if (mkdir(persist_dir, 0755) != 0) {
        puts("cc_ext2: persist mkdir failed");
        return 1;
    }

    int fd = open(persist_file, O_CREAT | O_TRUNC | O_WRONLY, 0644);
    if (fd < 0 || write(fd, "initial\n", 8) != 8) {
        puts("cc_ext2: persist create failed");
        return 1;
    }
    close(fd);

    fd = open(persist_removed, O_CREAT | O_TRUNC | O_WRONLY, 0644);
    if (fd < 0 || write(fd, "remove\n", 7) != 7) {
        puts("cc_ext2: persist removed create failed");
        return 1;
    }
    close(fd);

    if (link(persist_file, persist_hard) != 0) {
        puts("cc_ext2: persist hardlink failed");
        return 1;
    }
    fd = open(persist_hard, O_WRONLY | O_TRUNC, 0);
    if (fd < 0 || write(fd, persist_payload, strlen(persist_payload)) != (ssize_t)strlen(persist_payload)) {
        puts("cc_ext2: persist hardlink write failed");
        return 1;
    }
    close(fd);

    if (symlink(persist_hard, persist_link) != 0) {
        puts("cc_ext2: persist symlink failed");
        return 1;
    }
    if (rename(persist_file, persist_moved) != 0) {
        puts("cc_ext2: persist rename failed");
        return 1;
    }
    if (chmod(persist_moved, 0600) != 0 || chown(persist_moved, 1000, 1000) != 0) {
        puts("cc_ext2: persist metadata failed");
        return 1;
    }
    if (unlink(persist_removed) != 0) {
        puts("cc_ext2: persist unlink failed");
        return 1;
    }

    puts("cc_ext2: persist setup ok");
    return 0;
}

static int verify_reboot_torture(void) {
    struct stat st;
    if (stat(persist_moved, &st) != 0 || st.st_nlink < 2 ||
        (st.st_mode & 0777) != 0600 || st.st_uid != 1000 || st.st_gid != 1000) {
        puts("cc_ext2: persisted metadata failed");
        return 1;
    }
    if (read_exact_file(persist_moved, persist_payload) != 0 ||
        read_exact_file(persist_hard, persist_payload) != 0 ||
        read_exact_file(persist_link, persist_payload) != 0) {
        puts("cc_ext2: persisted data failed");
        return 1;
    }
    char target[128];
    ssize_t len = readlink(persist_link, target, sizeof(target) - 1);
    if (len < 0) {
        puts("cc_ext2: persisted readlink failed");
        return 1;
    }
    target[len] = '\0';
    if (strcmp(target, persist_hard) != 0) {
        puts("cc_ext2: persisted symlink target failed");
        return 1;
    }
    if (access(persist_removed, F_OK) == 0) {
        puts("cc_ext2: persisted unlink failed");
        return 1;
    }
    if (read_exact_file(marker, "ext2 persisted\n") != 0) {
        puts("cc_ext2: persisted marker failed");
        return 1;
    }

    cleanup_persist_tree();
    unlink(marker);
    puts("cc_ext2: reboot persistence ok");
    puts("cc_ext2: verify done");
    return 0;
}

static int run_setup(void) {
    const char *dir = "/home/ext2_torture";
    const char *original = "/home/ext2_torture/original";
    const char *moved = "/home/ext2_torture/moved";
    const char *hard = "/home/ext2_torture/hard";

    unlink(original);
    unlink(moved);
    unlink(hard);
    rmdir(dir);

    if (mkdir(dir, 0755) != 0) {
        puts("cc_ext2: mkdir failed");
        return 1;
    }
    struct stat st;
    if (stat(dir, &st) != 0 || (st.st_mode & 0777) != 0755) {
        puts("cc_ext2: mkdir stat failed");
        return 1;
    }

    int fd = open(original, O_CREAT | O_TRUNC | O_WRONLY, 0644);
    if (fd < 0 || write(fd, "rootfs", 6) != 6) {
        puts("cc_ext2: create failed");
        return 1;
    }
    close(fd);

    if (link(original, hard) != 0) {
        puts("cc_ext2: link failed");
        return 1;
    }
    if (stat(original, &st) != 0 || st.st_nlink < 2) {
        puts("cc_ext2: link stat failed");
        return 1;
    }

    if (rename(original, moved) != 0 || access(moved, F_OK) != 0 || access(original, F_OK) == 0) {
        puts("cc_ext2: rename failed");
        return 1;
    }
    if (chmod(moved, 0600) != 0) {
        puts("cc_ext2: chmod failed");
        return 1;
    }
    if (chown(moved, 1000, 1000) != 0) {
        puts("cc_ext2: chown failed");
        return 1;
    }
    if (stat(moved, &st) != 0 || (st.st_mode & 0777) != 0600 || st.st_uid != 1000 || st.st_gid != 1000) {
        puts("cc_ext2: metadata stat failed");
        return 1;
    }

    if (unlink(hard) != 0 || unlink(moved) != 0 || rmdir(dir) != 0 || access(dir, F_OK) == 0) {
        puts("cc_ext2: cleanup failed");
        return 1;
    }
    puts("cc_ext2: ops ok");

    if (setup_reboot_torture() != 0) {
        return 1;
    }

    fd = open(marker_tmp, O_CREAT | O_TRUNC | O_WRONLY, 0644);
    if (fd < 0 || write(fd, "ext2 persisted\n", 15) != 15) {
        puts("cc_ext2: marker write failed");
        return 1;
    }
    close(fd);
    if (rename(marker_tmp, marker) != 0 || chmod(marker, 0644) != 0 || chown(marker, 1000, 1000) != 0) {
        puts("cc_ext2: marker metadata failed");
        return 1;
    }
    puts("cc_ext2: marker ok");
    puts("cc_ext2: done");
    return 0;
}

int main(int argc, char **argv) {
    if (argc > 1 && strcmp(argv[1], "verify") == 0) {
        return verify_reboot_torture();
    }
    return run_setup();
}
