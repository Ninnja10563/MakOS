/* SPDX-License-Identifier: MIT */
#include "syscall.h"

enum {
    SYS_YIELD = 1,
    SYS_EXIT = 5,
    SYS_OPEN = 11,
    SYS_READ = 12,
    SYS_CLOSE = 13,
    SYS_FILE_WRITE = 17,
    SYS_CLOCK_MONOTONIC = 27,
    SYS_CREATE = 43,
    SYS_SOCKET_CREATE = 47,
    SYS_SOCKET_CONNECT = 48,
    SYS_SOCKET_SEND = 49,
    SYS_SOCKET_RECEIVE = 50,
    SYS_SOCKET_CLOSE = 51,
    SYS_TTY_READ = 62,
    SYS_TTY_WRITE = 63,
    SYS_ISATTY = 64,
    SYS_TCGETATTR = 65,
    SYS_TCSETATTR = 66,
    SYS_TCFLUSH = 67,
    SYS_IOCTL = 68,
    SYS_SIGACTION = 69,
    SYS_RAISE = 70,
    SYS_GETRANDOM = 83,
    SYS_CLOCK_REALTIME = 84,
    SYS_PROCESS_IDENTITY = 85,
};

long __makos_raw_syscall4(makos_u64 number, makos_u64 first,
                          makos_u64 second, makos_u64 third,
                          makos_u64 fourth)
{
#if defined(__aarch64__)
    register makos_u64 x0 __asm__("x0") = first;
    register makos_u64 x1 __asm__("x1") = second;
    register makos_u64 x2 __asm__("x2") = third;
    register makos_u64 x3 __asm__("x3") = fourth;
    register makos_u64 x8 __asm__("x8") = number;
    __asm__ volatile("svc #0" : "+r"(x0)
                     : "r"(x1), "r"(x2), "r"(x3), "r"(x8)
                     : "memory", "cc");
    return (long)x0;
#else
#error "MakOS musl bootstrap syscall layer supports AArch64 only"
#endif
}

long __makos_read(long fd, void *buffer, makos_size_t length)
{
    return __makos_raw_syscall4(fd == 0 ? SYS_TTY_READ : SYS_READ,
        (makos_u64)fd, (makos_u64)buffer, length, 0);
}

long __makos_write(long fd, const void *buffer, makos_size_t length)
{
    return __makos_raw_syscall4(fd == 1 || fd == 2 ? SYS_TTY_WRITE : SYS_FILE_WRITE,
        (makos_u64)fd, (makos_u64)buffer, length, 0);
}

long __makos_open(const char *path, makos_size_t path_length, int write_access)
{
    return __makos_raw_syscall4(SYS_OPEN, (makos_u64)path, path_length,
                                write_access != 0, 0);
}

long __makos_close(long fd)
{
    return __makos_raw_syscall4(SYS_CLOSE, (makos_u64)fd, 0, 0, 0);
}

long __makos_create(const char *path, makos_size_t path_length)
{
    return __makos_raw_syscall4(SYS_CREATE, (makos_u64)path, path_length, 0, 0);
}

long __makos_yield(void)
{
    return __makos_raw_syscall4(SYS_YIELD, 0, 0, 0, 0);
}

makos_u64 __makos_clock_ticks(void)
{
    return (makos_u64)__makos_raw_syscall4(SYS_CLOCK_MONOTONIC, 0, 0, 0, 0);
}

long __makos_isatty(long fd)
{
    return __makos_raw_syscall4(SYS_ISATTY, (makos_u64)fd, 0, 0, 0);
}

long __makos_tcgetattr(long fd, struct makos_termios *value)
{
    return __makos_raw_syscall4(SYS_TCGETATTR, (makos_u64)fd,
                                (makos_u64)value, sizeof(*value), 0);
}

long __makos_tcsetattr(long fd, unsigned action,
                       const struct makos_termios *value)
{
    return __makos_raw_syscall4(SYS_TCSETATTR, (makos_u64)fd, action,
                                (makos_u64)value, sizeof(*value));
}

long __makos_tcflush(long fd, unsigned queue)
{
    return __makos_raw_syscall4(SYS_TCFLUSH, (makos_u64)fd, queue, 0, 0);
}

long __makos_ioctl_winsize(long fd, unsigned request,
                           struct makos_winsize *value)
{
    return __makos_raw_syscall4(SYS_IOCTL, (makos_u64)fd, request,
                                (makos_u64)value, sizeof(*value));
}

long __makos_sigaction(unsigned signal,
                       const struct makos_signal_action *replacement,
                       struct makos_signal_action *previous)
{
    return __makos_raw_syscall4(SYS_SIGACTION, signal,
        (makos_u64)replacement, (makos_u64)previous, sizeof(*previous));
}

long __makos_raise(unsigned signal)
{
    return __makos_raw_syscall4(SYS_RAISE, signal, 0, 0, 0);
}

long __makos_socket_create(unsigned domain, unsigned type, unsigned protocol)
{
    return __makos_raw_syscall4(SYS_SOCKET_CREATE, domain, type, protocol, 0);
}

long __makos_socket_connect(long socket,
                            const struct makos_sockaddr_in *address)
{
    return __makos_raw_syscall4(SYS_SOCKET_CONNECT, (makos_u64)socket,
                                (makos_u64)address, sizeof(*address), 0);
}

long __makos_socket_send(long socket, const void *buffer, makos_size_t length)
{
    return __makos_raw_syscall4(SYS_SOCKET_SEND, (makos_u64)socket,
                                (makos_u64)buffer, length, 0);
}

long __makos_socket_receive(long socket, void *buffer, makos_size_t length)
{
    return __makos_raw_syscall4(SYS_SOCKET_RECEIVE, (makos_u64)socket,
                                (makos_u64)buffer, length, 0);
}

long __makos_socket_close(long socket)
{
    return __makos_raw_syscall4(SYS_SOCKET_CLOSE, (makos_u64)socket, 0, 0, 0);
}

__attribute__((noreturn)) void __makos_exit(int status)
{
    (void)__makos_raw_syscall4(SYS_EXIT, (unsigned)status, 0, 0, 0);
    __builtin_trap();
}
