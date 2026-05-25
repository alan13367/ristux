#ifndef _RISTUX_UNISTD_H
#define _RISTUX_UNISTD_H

#include <stddef.h>
#include <sys/types.h>

extern char **environ;

#define F_OK 0
#define X_OK 1
#define W_OK 2
#define R_OK 4

ssize_t read(int fd, void *buf, size_t len);
ssize_t write(int fd, const void *buf, size_t len);
int close(int fd);
off_t lseek(int fd, off_t offset, int whence);
int pipe(int pipefd[2]);
int dup(int oldfd);
int dup2(int oldfd, int newfd);
pid_t fork(void);
int execve(const char *path, char *const argv[], char *const envp[]);
pid_t getpid(void);
pid_t getppid(void);
int chdir(const char *path);
char *getcwd(char *buf, size_t size);
int access(const char *path, int mode);
int unlink(const char *path);
int brk(void *addr);
void *sbrk(long increment);
void _exit(int status) __attribute__((noreturn));

#endif
