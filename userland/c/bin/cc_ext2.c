#include <errno.h>
#include <fcntl.h>
#include <stdio.h>
#include <string.h>
#include <sys/stat.h>
#include <unistd.h>

int main(void) {
    const char *dir = "/home/ext2_torture";
    const char *original = "/home/ext2_torture/original";
    const char *moved = "/home/ext2_torture/moved";
    const char *hard = "/home/ext2_torture/hard";
    const char *marker_tmp = "/home/ext2_reboot_marker.tmp";
    const char *marker = "/home/ext2_reboot_marker";

    unlink(original);
    unlink(moved);
    unlink(hard);
    rmdir(dir);
    unlink(marker_tmp);
    unlink(marker);

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
