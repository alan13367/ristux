#include <errno.h>
#include <grp.h>
#include <stdio.h>
#include <sys/ioctl.h>
#include <sys/types.h>
#include <unistd.h>

static int check_res_id_faults(void) {
    uid_t ruid = 77;
    uid_t suid = 88;
    errno = 0;
    if (getresuid(&ruid, (uid_t *)~0UL, &suid) != -1 || errno != EFAULT ||
        ruid != 77 || suid != 88) {
        puts("cc_cred: getresuid fault failed");
        return 1;
    }

    gid_t rgid = 77;
    gid_t sgid = 88;
    errno = 0;
    if (getresgid(&rgid, (gid_t *)~0UL, &sgid) != -1 || errno != EFAULT ||
        rgid != 77 || sgid != 88) {
        puts("cc_cred: getresgid fault failed");
        return 1;
    }

    puts("cc_cred: res id faults ok");
    return 0;
}

int main(void) {
    uid_t uid = getuid();
    uid_t euid = geteuid();
    gid_t gid = getgid();
    gid_t egid = getegid();

    if (uid != euid || gid != egid) {
        puts("cc_cred: id mismatch");
        return 1;
    }
    uid_t ruid = 99;
    uid_t reuid = 99;
    uid_t suid = 99;
    gid_t rgid = 99;
    gid_t regid = 99;
    gid_t sgid = 99;
    if (getresuid(&ruid, &reuid, &suid) < 0 ||
        getresgid(&rgid, &regid, &sgid) < 0 ||
        ruid != uid ||
        reuid != euid ||
        suid != uid ||
        rgid != gid ||
        regid != egid ||
        sgid != gid) {
        puts("cc_cred: res ids failed");
        return 1;
    }
    puts("cc_cred: ids ok");
    if (check_res_id_faults() != 0) {
        return 1;
    }

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
    if (getresuid(&ruid, &reuid, &suid) < 0 ||
        ruid != uid ||
        reuid != euid ||
        suid != uid) {
        puts("cc_cred: getresuid after set failed");
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
    if (getresgid(&rgid, &regid, &sgid) < 0 ||
        rgid != gid ||
        regid != egid ||
        sgid != gid) {
        puts("cc_cred: getresgid after set failed");
        return 1;
    }
    gid_t groups[4];
    int ngroups = 4;
    if (getgrouplist("root", gid, groups, &ngroups) < 0 ||
        ngroups < 1 || groups[0] != gid) {
        puts("cc_cred: grouplist failed");
        return 1;
    }
    gid_t current_groups[8];
    int group_count = getgroups(0, NULL);
    if (group_count < 1 || group_count > 8) {
        puts("cc_cred: getgroups count failed");
        return 1;
    }
    if (getgroups(group_count, current_groups) != group_count ||
        current_groups[0] != gid) {
        puts("cc_cred: getgroups list failed");
        return 1;
    }
    if (uid == 0) {
        gid_t root_groups[2] = { gid, 7 };
        if (setgroups(2, root_groups) < 0 ||
            getgroups(2, current_groups) != 2 ||
            current_groups[0] != gid ||
            current_groups[1] != 7) {
            puts("cc_cred: getgroups after set failed");
            return 1;
        }
        errno = 0;
        if (getgroups(1, current_groups) != -1 || errno != EINVAL) {
            puts("cc_cred: getgroups small buffer failed");
            return 1;
        }
        gid_t restore[1] = { gid };
        if (setgroups(1, restore) < 0) {
            puts("cc_cred: restore groups failed");
            return 1;
        }
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
