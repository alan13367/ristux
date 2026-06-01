#include <stdio.h>
#include <string.h>
#include <sys/ioctl.h>
#include <termios.h>
#include <unistd.h>

static void reset_sane(struct termios *term) {
    term->c_iflag |= ICRNL;
    term->c_oflag |= OPOST | ONLCR;
    term->c_cflag |= CREAD | CS8;
    term->c_lflag |= ISIG | ICANON | ECHO | ECHOE | ECHOK | IEXTEN;
    term->c_cc[VINTR] = 0x03;
    term->c_cc[VERASE] = 0x7f;
    term->c_cc[VEOF] = 0x04;
    term->c_cc[VMIN] = 1;
    term->c_cc[VTIME] = 0;
    term->c_cc[VSUSP] = 0x1a;
}

static void print_cc(const char *name, unsigned char value) {
    printf("%s = ", name);
    if (value == 0) {
        printf("<undef>");
    } else if (value == 0x7f) {
        printf("^?");
    } else if (value < 0x20) {
        printf("^%c", (char)(value + '@'));
    } else {
        printf("%c", value);
    }
    printf("; ");
}

static void print_flag(tcflag_t flags, tcflag_t bit, const char *name) {
    if ((flags & bit) != 0) {
        printf("%s ", name);
    } else {
        printf("-%s ", name);
    }
}

static int print_all(void) {
    struct termios term;
    struct winsize ws;
    if (tcgetattr(STDIN_FILENO, &term) < 0) {
        puts("stty: tcgetattr failed");
        return 1;
    }
    if (ioctl(STDIN_FILENO, TIOCGWINSZ, &ws) < 0) {
        ws.ws_row = 0;
        ws.ws_col = 0;
    }
    printf("speed %u baud; rows %u; columns %u;\n", term.c_ospeed, ws.ws_row, ws.ws_col);
    print_flag(term.c_lflag, ISIG, "isig");
    print_flag(term.c_lflag, ICANON, "icanon");
    print_flag(term.c_lflag, ECHO, "echo");
    print_flag(term.c_lflag, IEXTEN, "iexten");
    printf("min %u time %u\n", term.c_cc[VMIN], term.c_cc[VTIME]);
    print_cc("intr", term.c_cc[VINTR]);
    print_cc("erase", term.c_cc[VERASE]);
    print_cc("eof", term.c_cc[VEOF]);
    print_cc("susp", term.c_cc[VSUSP]);
    printf("\n");
    return 0;
}

int main(int argc, char **argv) {
    if (argc == 1 || strcmp(argv[1], "-a") == 0) {
        return print_all();
    }

    struct termios term;
    if (tcgetattr(STDIN_FILENO, &term) < 0) {
        puts("stty: tcgetattr failed");
        return 1;
    }

    if (strcmp(argv[1], "raw") == 0) {
        cfmakeraw(&term);
        if (tcsetattr(STDIN_FILENO, TCSANOW, &term) < 0) {
            puts("stty: tcsetattr failed");
            return 1;
        }
        puts("stty: raw");
        return 0;
    }

    if (strcmp(argv[1], "sane") == 0) {
        reset_sane(&term);
        if (tcsetattr(STDIN_FILENO, TCSANOW, &term) < 0) {
            puts("stty: tcsetattr failed");
            return 1;
        }
        puts("stty: sane");
        return 0;
    }

    puts("usage: stty [-a|raw|sane]");
    return 1;
}
