#include <fcntl.h>
#include <poll.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/ioctl.h>
#include <unistd.h>

static int expect_bytes(int fd, const char *expected, size_t len) {
    char buf[16];
    if (len > sizeof(buf)) {
        return 0;
    }
    ssize_t got = read(fd, buf, len);
    return got == (ssize_t)len && memcmp(buf, expected, len) == 0;
}

int main(void) {
    int master = posix_openpt(O_RDWR);
    if (master < 0) {
        puts("cc_pty: ptmx open failed");
        return 1;
    }

    unsigned int number = 9999;
    if (ioctl(master, TIOCGPTN, &number) < 0 || number > 255) {
        puts("cc_pty: pty number failed");
        return 1;
    }
    if (grantpt(master) < 0 || unlockpt(master) < 0) {
        puts("cc_pty: unlock failed");
        return 1;
    }
    char *slave_path = ptsname(master);
    if (slave_path == NULL) {
        puts("cc_pty: ptsname failed");
        return 1;
    }
    int slave = open(slave_path, O_RDWR, 0);
    if (slave < 0) {
        puts("cc_pty: slave open failed");
        return 1;
    }
    puts("cc_pty: open ok");

    struct pollfd writable[2] = {
        { master, POLLOUT, 0 },
        { slave, POLLOUT, 0 },
    };
    if (poll(writable, 2, 0) != 2 || (writable[0].revents & POLLOUT) == 0 ||
        (writable[1].revents & POLLOUT) == 0) {
        puts("cc_pty: poll write failed");
        return 1;
    }

    if (write(master, "abc", 3) != 3) {
        puts("cc_pty: master write failed");
        return 1;
    }
    struct pollfd slave_ready = { slave, POLLIN, 0 };
    if (poll(&slave_ready, 1, 0) != 1 || (slave_ready.revents & POLLIN) == 0) {
        puts("cc_pty: slave poll failed");
        return 1;
    }
    if (!expect_bytes(slave, "abc", 3)) {
        puts("cc_pty: slave read failed");
        return 1;
    }
    puts("cc_pty: master-to-slave ok");

    if (write(slave, "xyz", 3) != 3) {
        puts("cc_pty: slave write failed");
        return 1;
    }
    struct pollfd master_ready = { master, POLLIN, 0 };
    if (poll(&master_ready, 1, 0) != 1 || (master_ready.revents & POLLIN) == 0) {
        puts("cc_pty: master poll failed");
        return 1;
    }
    if (!expect_bytes(master, "xyz", 3)) {
        puts("cc_pty: master read failed");
        return 1;
    }
    puts("cc_pty: slave-to-master ok");

    close(slave);
    close(master);
    puts("cc_pty: done");
    return 0;
}
