#include <grp.h>
#include <pwd.h>
#include <shadow.h>
#include <stdio.h>
#include <string.h>
#include <unistd.h>

static int check_passwd(void) {
    struct passwd *root = getpwnam("root");
    if (root == NULL ||
        root->pw_uid != 0 ||
        root->pw_gid != 0 ||
        strcmp(root->pw_dir, "/root") != 0 ||
        strcmp(root->pw_shell, "/bin/sh") != 0) {
        puts("cc_passwd: root lookup failed");
        return 1;
    }
    struct passwd *alice = getpwuid(1000);
    if (alice == NULL ||
        strcmp(alice->pw_name, "alice") != 0 ||
        alice->pw_gid != 1000 ||
        strcmp(alice->pw_dir, "/home/alice") != 0) {
        puts("cc_passwd: uid lookup failed");
        return 1;
    }
    puts("cc_passwd: passwd ok");
    return 0;
}

static int check_group(void) {
    struct group *root = getgrnam("root");
    if (root == NULL || root->gr_gid != 0 || strcmp(root->gr_name, "root") != 0) {
        puts("cc_passwd: root group failed");
        return 1;
    }
    struct group *alice = getgrgid(1000);
    if (alice == NULL ||
        strcmp(alice->gr_name, "alice") != 0 ||
        alice->gr_mem == NULL ||
        alice->gr_mem[0] != NULL) {
        puts("cc_passwd: gid lookup failed");
        return 1;
    }
    if (initgroups("alice", 1000) < 0) {
        puts("cc_passwd: initgroups failed");
        return 1;
    }
    puts("cc_passwd: group ok");
    return 0;
}

static int check_shadow(void) {
    struct spwd *root = getspnam("root");
    if (root == NULL ||
        strcmp(root->sp_namp, "root") != 0 ||
        root->sp_pwdp == NULL ||
        root->sp_pwdp[0] != '\0') {
        puts("cc_passwd: shadow failed");
        return 1;
    }
    puts("cc_passwd: shadow ok");
    return 0;
}

int main(void) {
    if (check_passwd() != 0) {
        return 1;
    }
    if (check_group() != 0) {
        return 1;
    }
    if (check_shadow() != 0) {
        return 1;
    }
    puts("cc_passwd: done");
    return 0;
}
