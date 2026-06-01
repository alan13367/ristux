#include <fcntl.h>
#include <poll.h>
#include <pty.h>
#include <signal.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/ioctl.h>
#include <sys/syscall.h>
#include <sys/wait.h>
#include <termios.h>
#include <unistd.h>

static int expect_bytes(int fd, const char *expected, size_t len) {
    char buf[16];
    if (len > sizeof(buf)) {
        return 0;
    }
    ssize_t got = read(fd, buf, len);
    return got == (ssize_t)len && memcmp(buf, expected, len) == 0;
}

static int expect_no_read_ready(int fd) {
    struct pollfd pfd = { fd, POLLIN, 0 };
    return poll(&pfd, 1, 0) == 0;
}

static int wait_for_exit(pid_t child, int expected_status) {
    int status = 0;
    for (int i = 0; i < 100; i++) {
        pid_t waited = waitpid(child, &status, WNOHANG);
        if (waited == child) {
            return WIFEXITED(status) && WEXITSTATUS(status) == expected_status;
        }
        if (waited < 0) {
            return 0;
        }
        syscall(SYS_sched_yield);
    }
    return 0;
}

static int check_signal_chars(int master, int slave) {
    struct termios term;
    if (tcgetattr(slave, &term) < 0) {
        puts("cc_pty: signal termios get failed");
        return 1;
    }
    term.c_lflag |= ISIG;
    term.c_cc[VINTR] = 0x03;
    if (tcsetattr(slave, TCSANOW, &term) < 0) {
        puts("cc_pty: signal termios set failed");
        return 1;
    }

    pid_t child = fork();
    if (child < 0) {
        puts("cc_pty: signal fork failed");
        return 1;
    }
    if (child == 0) {
        close(master);
        setpgid(0, 0);
        char byte;
        for (;;) {
            read(slave, &byte, 1);
        }
    }

    setpgid(child, child);
    int foreground = (int)child;
    if (ioctl(slave, TIOCSPGRP, &foreground) < 0) {
        puts("cc_pty: signal foreground failed");
        kill(child, SIGKILL);
        waitpid(child, NULL, 0);
        return 1;
    }
    char intr = 0x03;
    if (write(master, &intr, 1) != 1 || !wait_for_exit(child, 128 + SIGINT)) {
        puts("cc_pty: signal char failed");
        kill(child, SIGKILL);
        waitpid(child, NULL, 0);
        return 1;
    }
    puts("cc_pty: signal char ok");
    return 0;
}

static int check_line_discipline(int master, int slave) {
    if (write(master, "ab", 2) != 2 || !expect_bytes(master, "ab", 2)) {
        puts("cc_pty: echo failed");
        return 1;
    }
    if (!expect_no_read_ready(slave)) {
        puts("cc_pty: canonical premature read failed");
        return 1;
    }
    if (write(master, "c\n", 2) != 2 || !expect_bytes(master, "c\r\n", 3)) {
        puts("cc_pty: newline echo failed");
        return 1;
    }
    struct pollfd slave_ready = { slave, POLLIN, 0 };
    if (poll(&slave_ready, 1, 0) != 1 || (slave_ready.revents & POLLIN) == 0) {
        puts("cc_pty: canonical poll failed");
        return 1;
    }
    if (!expect_bytes(slave, "abc\n", 4)) {
        puts("cc_pty: canonical read failed");
        return 1;
    }
    puts("cc_pty: line discipline ok");
    return 0;
}

static int set_raw(int slave) {
    struct termios term;
    if (tcgetattr(slave, &term) < 0) {
        return 0;
    }
    cfmakeraw(&term);
    return tcsetattr(slave, TCSANOW, &term) == 0;
}

static int check_openpty(void) {
    int master = -1;
    int slave = -1;
    char name[32];
    struct termios term;
    memset(&term, 0, sizeof(term));
    term.c_iflag = ICRNL;
    term.c_oflag = OPOST;
    term.c_cflag = CREAD | CS8;
    term.c_lflag = ISIG;
    term.c_cc[VMIN] = 7;
    term.c_cc[VTIME] = 2;
    struct winsize ws = { 30, 100, 0, 0 };
    if (openpty(&master, &slave, name, &term, &ws) < 0) {
        puts("cc_pty: openpty failed");
        return 1;
    }
    if (!isatty(master) || !isatty(slave)) {
        puts("cc_pty: isatty failed");
        return 1;
    }
    char *slave_name = ttyname(slave);
    if (slave_name == NULL || strcmp(slave_name, name) != 0) {
        puts("cc_pty: ttyname failed");
        return 1;
    }
    struct termios got_term;
    if (tcgetattr(slave, &got_term) < 0 ||
        memcmp(&got_term, &term, sizeof(term)) != 0) {
        puts("cc_pty: termios state failed");
        return 1;
    }
    struct winsize got_ws;
    memset(&got_ws, 0, sizeof(got_ws));
    if (ioctl(master, TIOCGWINSZ, &got_ws) < 0 ||
        got_ws.ws_row != ws.ws_row || got_ws.ws_col != ws.ws_col) {
        puts("cc_pty: winsize state failed");
        return 1;
    }
    int pgrp = (int)getpgrp();
    int got_pgrp = -1;
    if (ioctl(slave, TIOCSPGRP, &pgrp) < 0 ||
        ioctl(master, TIOCGPGRP, &got_pgrp) < 0 ||
        got_pgrp != pgrp) {
        puts("cc_pty: foreground pgrp failed");
        return 1;
    }
    if (write(master, "op", 2) != 2 || !expect_bytes(slave, "op", 2)) {
        puts("cc_pty: openpty transfer failed");
        return 1;
    }
    close(slave);
    close(master);
    puts("cc_pty: openpty ok");
    return 0;
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

    if (check_line_discipline(master, slave) != 0) {
        return 1;
    }
    if (!set_raw(slave)) {
        puts("cc_pty: raw mode failed");
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

    struct termios cooked;
    if (tcgetattr(slave, &cooked) < 0) {
        puts("cc_pty: cooked termios get failed");
        return 1;
    }
    cooked.c_oflag |= OPOST | ONLCR;
    if (tcsetattr(slave, TCSANOW, &cooked) < 0) {
        puts("cc_pty: cooked termios set failed");
        return 1;
    }
    if (write(slave, "nl\n", 3) != 3 || !expect_bytes(master, "nl\r\n", 4)) {
        puts("cc_pty: output newline translation failed");
        return 1;
    }
    puts("cc_pty: output processing ok");

    if (!set_raw(slave)) {
        puts("cc_pty: raw mode restore failed");
        return 1;
    }

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

    if (check_signal_chars(master, slave) != 0) {
        return 1;
    }

    close(slave);
    close(master);
    if (check_openpty() != 0) {
        return 1;
    }
    puts("cc_pty: done");
    return 0;
}
