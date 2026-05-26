#include <errno.h>
#include <grp.h>
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
    if (getuid() != uid || geteuid() != euid) {
        puts("cc_cred: setresuid changed wrong ids");
        return 1;
    }
    if (seteuid(euid) < 0) {
        puts("cc_cred: seteuid failed");
        return 1;
    }
    if (setegid(egid) < 0 || setresgid((gid_t)-1, egid, (gid_t)-1) < 0) {
        puts("cc_cred: setresgid failed");
        return 1;
    }
    if (getgid() != gid || getegid() != egid) {
        puts("cc_cred: setresgid changed wrong ids");
        return 1;
    }
    gid_t groups[4];
    int ngroups = 4;
    if (getgrouplist("root", gid, groups, &ngroups) < 0 ||
        ngroups < 1 || groups[0] != gid) {
        puts("cc_cred: grouplist failed");
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
