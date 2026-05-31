#ifndef _RISTUX_UNISTD_H
#define _RISTUX_UNISTD_H

#include <stddef.h>
#include <sys/types.h>

extern char **environ;

#define F_OK 0
#define X_OK 1
#define W_OK 2
#define R_OK 4

#define STDIN_FILENO 0
#define STDOUT_FILENO 1
#define STDERR_FILENO 2

#define SEEK_SET 0
#define SEEK_CUR 1
#define SEEK_END 2

ssize_t read(int fd, void *buf, size_t len);
ssize_t write(int fd, const void *buf, size_t len);
ssize_t pread(int fd, void *buf, size_t len, off_t offset);
ssize_t pwrite(int fd, const void *buf, size_t len, off_t offset);
int close(int fd);
off_t lseek(int fd, off_t offset, int whence);
int pipe(int pipefd[2]);
int pipe2(int pipefd[2], int flags);
int dup(int oldfd);
int dup2(int oldfd, int newfd);
int dup3(int oldfd, int newfd, int flags);
int fsync(int fd);
int ftruncate(int fd, off_t length);
pid_t fork(void);
pid_t vfork(void);
int execve(const char *path, char *const argv[], char *const envp[]);
int execv(const char *path, char *const argv[]);
int execvp(const char *file, char *const argv[]);
pid_t getpid(void);
pid_t getppid(void);
pid_t getpgrp(void);
int setpgid(pid_t pid, pid_t pgid);
pid_t setsid(void);
uid_t getuid(void);
uid_t geteuid(void);
gid_t getgid(void);
gid_t getegid(void);
int setuid(uid_t uid);
int seteuid(uid_t euid);
int setgid(gid_t gid);
int setresuid(uid_t ruid, uid_t euid, uid_t suid);
int getresuid(uid_t *ruid, uid_t *euid, uid_t *suid);
int setegid(gid_t egid);
int setresgid(gid_t rgid, gid_t egid, gid_t sgid);
int getresgid(gid_t *rgid, gid_t *egid, gid_t *sgid);
int getgroups(int size, gid_t list[]);
int setgroups(size_t size, const gid_t *list);
int chdir(const char *path);
char *getcwd(char *buf, size_t size);
int access(const char *path, int mode);
int faccessat(int dirfd, const char *path, int mode, int flags);
int isatty(int fd);
char *ttyname(int fd);
int unlink(const char *path);
int rmdir(const char *path);
int link(const char *oldpath, const char *newpath);
int symlink(const char *target, const char *linkpath);
ssize_t readlink(const char *path, char *buf, size_t bufsiz);
int chown(const char *path, uid_t owner, gid_t group);
int brk(void *addr);
void *sbrk(long increment);
int daemon(int nochdir, int noclose);
unsigned int sleep(unsigned int seconds);
void _exit(int status) __attribute__((noreturn));

#endif
