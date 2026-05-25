#include <dirent.h>
#include <fcntl.h>
#include <stdio.h>
#include <string.h>
#include <unistd.h>

static int contains(const char *haystack, const char *needle) {
    size_t needle_len = strlen(needle);
    if (needle_len == 0) {
        return 1;
    }
    for (const char *cursor = haystack; *cursor; cursor++) {
        size_t i = 0;
        while (needle[i] && cursor[i] == needle[i]) {
            i++;
        }
        if (i == needle_len) {
            return 1;
        }
    }
    return 0;
}

static int read_file(const char *path, char *buf, int len) {
    int fd = open(path, O_RDONLY, 0);
    if (fd < 0) {
        return -1;
    }
    int nread = read(fd, buf, len - 1);
    close(fd);
    if (nread < 0) {
        return -1;
    }
    buf[nread] = '\0';
    return nread;
}

static int check_file(const char *path, const char *needle, const char *label) {
    char buf[512];
    if (read_file(path, buf, sizeof(buf)) <= 0) {
        printf("cc_procfs: %s read failed\n", label);
        return 1;
    }
    if (!contains(buf, needle)) {
        printf("cc_procfs: %s missing %s\n", label, needle);
        return 1;
    }
    printf("cc_procfs: %s ok\n", label);
    return 0;
}

static int dir_contains(const char *storage, int nread, const char *needle) {
    for (int off = 0; off < nread;) {
        const struct linux_dirent64 *ent = (const struct linux_dirent64 *)(storage + off);
        if (strcmp(ent->d_name, needle) == 0) {
            return 1;
        }
        off += ent->d_reclen;
    }
    return 0;
}

static int check_proc_dir(void) {
    char storage[512];
    int fd = open("/proc", O_RDONLY, 0);
    if (fd < 0) {
        puts("cc_procfs: opendir failed");
        return 1;
    }
    int nread = getdents64(fd, (struct linux_dirent64 *)storage, sizeof(storage));
    close(fd);
    if (nread < 0) {
        puts("cc_procfs: getdents failed");
        return 1;
    }
    int ok = dir_contains(storage, nread, "self") && dir_contains(storage, nread, "mounts")
        && dir_contains(storage, nread, "meminfo") && dir_contains(storage, nread, "uptime")
        && dir_contains(storage, nread, "stat");
    if (!ok) {
        puts("cc_procfs: dir entries missing");
        return 1;
    }
    puts("cc_procfs: dir ok");
    return 0;
}

int main(void) {
    if (check_proc_dir() != 0) {
        return 1;
    }
    if (check_file("/proc/mounts", "procfs /proc", "mounts") != 0) {
        return 1;
    }
    if (check_file("/proc/meminfo", "MemFree:", "meminfo") != 0) {
        return 1;
    }
    if (check_file("/proc/uptime", ".", "uptime") != 0) {
        return 1;
    }
    if (check_file("/proc/stat", "processes", "stat") != 0) {
        return 1;
    }
    if (check_file("/proc/self/status", "state:", "self") != 0) {
        return 1;
    }
    puts("cc_procfs: done");
    return 0;
}
