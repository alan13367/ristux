#include <stdio.h>
#include <termios.h>
#include <unistd.h>

int main(void) {
    struct termios original;
    if (tcgetattr(STDIN_FILENO, &original) < 0) {
        puts("cc_tty: tcgetattr failed");
        return 1;
    }
    if ((original.c_lflag & (ISIG | ICANON | ECHO)) != (ISIG | ICANON | ECHO)) {
        puts("cc_tty: default flags mismatch");
        return 1;
    }
    if (original.c_cc[VERASE] != '\b' || original.c_cc[VMIN] != 1 ||
        original.c_cc[VTIME] != 0) {
        puts("cc_tty: default cc mismatch");
        return 1;
    }
    puts("cc_tty: tcgetattr ok");

    struct termios raw = original;
    cfmakeraw(&raw);
    if ((raw.c_lflag & (ISIG | ICANON | ECHO | IEXTEN)) != 0 || raw.c_cc[VMIN] != 1) {
        puts("cc_tty: cfmakeraw mismatch");
        return 1;
    }
    puts("cc_tty: cfmakeraw ok");

    if (tcsetattr(STDIN_FILENO, TCSANOW, &raw) < 0) {
        puts("cc_tty: raw tcsetattr failed");
        return 1;
    }
    struct termios after_raw;
    if (tcgetattr(STDIN_FILENO, &after_raw) < 0) {
        puts("cc_tty: raw tcgetattr failed");
        return 1;
    }
    if ((after_raw.c_lflag & (ISIG | ICANON | ECHO | IEXTEN)) != 0) {
        puts("cc_tty: raw flags mismatch");
        return 1;
    }
    puts("cc_tty: tcsetattr ok");

    struct termios timed = raw;
    timed.c_cc[VMIN] = 0;
    timed.c_cc[VTIME] = 1;
    if (tcsetattr(STDIN_FILENO, TCSANOW, &timed) < 0) {
        puts("cc_tty: timed tcsetattr failed");
        return 1;
    }
    char byte;
    ssize_t nread = read(STDIN_FILENO, &byte, 1);
    if (nread != 0) {
        puts("cc_tty: vtime read failed");
        return 1;
    }
    puts("cc_tty: vtime ok");

    if (tcsetattr(STDIN_FILENO, TCSANOW, &original) < 0) {
        puts("cc_tty: restore failed");
        return 1;
    }
    struct termios restored;
    if (tcgetattr(STDIN_FILENO, &restored) < 0) {
        puts("cc_tty: restore tcgetattr failed");
        return 1;
    }
    if ((restored.c_lflag & (ISIG | ICANON | ECHO)) != (ISIG | ICANON | ECHO)) {
        puts("cc_tty: restore flags mismatch");
        return 1;
    }
    puts("cc_tty: restore ok");
    puts("cc_tty: done");
    return 0;
}
