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
uid_t getuid(void);
uid_t geteuid(void);
gid_t getgid(void);
gid_t getegid(void);
int setuid(uid_t uid);
int setgid(gid_t gid);
int setresuid(uid_t ruid, uid_t euid, uid_t suid);
int setgroups(size_t size, const gid_t *list);
int chdir(const char *path);
char *getcwd(char *buf, size_t size);
int access(const char *path, int mode);
int unlink(const char *path);
int rmdir(const char *path);
int link(const char *oldpath, const char *newpath);
int symlink(const char *target, const char *linkpath);
ssize_t readlink(const char *path, char *buf, size_t bufsiz);
int chown(const char *path, uid_t owner, gid_t group);
int brk(void *addr);
void *sbrk(long increment);
void _exit(int status) __attribute__((noreturn));

#endif
