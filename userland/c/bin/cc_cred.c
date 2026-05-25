#include <errno.h>
#include <stdio.h>
#include <sys/ioctl.h>
#include <sys/types.h>
#include <unistd.h>

int main(void) {
    uid_t uid = getuid();
    uid_t euid = geteuid();
    gid_t gid = getgid();
    gid_t egid = getegid();

    if (uid != euid || gid != egid) {
        puts("cc_cred: id mismatch");
        return 1;
    }
    puts("cc_cred: ids ok");

    if (setuid(uid) < 0 || setgid(gid) < 0) {
        puts("cc_cred: setid failed");
        return 1;
    }
    if (setresuid((uid_t)-1, euid, (uid_t)-1) < 0) {
        puts("cc_cred: setresuid failed");
        return 1;
    }
    if (uid != 0) {
        gid_t groups[1] = { gid };
        if (setgroups(1, groups) == 0 || errno != EACCES) {
            puts("cc_cred: setgroups permission failed");
            return 1;
        }
    }
    puts("cc_cred: setters ok");

    struct winsize ws;
    if (ioctl(0, TIOCGWINSZ, &ws) < 0) {
        puts("cc_cred: ioctl failed");
        return 1;
    }
    if (ws.ws_row != 24 || ws.ws_col != 80) {
        puts("cc_cred: winsize mismatch");
        return 1;
    }
    puts("cc_cred: ioctl ok");
    puts("cc_cred: done");
    return 0;
}
